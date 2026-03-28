mod coordinator;
pub mod handlers;
pub mod interceptor;
pub mod message;
pub mod pipeline;
mod reader;
mod recording_reader;
mod transaction;
mod writer;

pub use coordinator::CoordinatorActor;
pub use interceptor::{InterceptorHandle, Transaction};
pub use message::{CoordinatorMsg, Side, WriterMsg};
pub use reader::spawn_reader;
pub use writer::WriterActor;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use log::{error, info};
use ractor::Actor;
use tokio::net::{TcpListener, TcpStream};

use crate::crypto::Cipher;
use crate::packet::direction::Ac215PacketDirection;
use crate::packet::header::ChecksumMode;
use crate::server::Channel;

use coordinator::CoordinatorArgs;
use writer::WriterArgs;

/// A transparent MITM proxy between an AC-215 server and panel.
///
/// Connection sequence:
///   1. Proxy listens on `listen_addr` for the server's primary connection
///   2. Server connects to proxy (thinks it's the panel)
///   3. Proxy connects to the real panel on `panel_addr`
///   4. Panel connects back to proxy on `panel_addr + 1` (async channel)
///   5. Proxy connects to the server's async listener on `listen_addr + 1`
///
/// Four reader tasks and four writer actors relay frames through
/// a central coordinator actor that logs and forwards traffic.
///
/// On disconnect from either side, the proxy tears down all connections
/// and re-runs the startup sequence.
pub struct Proxy {
    listen_addr: SocketAddr,
    panel_addr: SocketAddr,
    cipher: Cipher,
    checksum_mode: ChecksumMode,
    handle: InterceptorHandle,
    handlers: Vec<Arc<Mutex<dyn pipeline::FrameHandler>>>,
    on_listening: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl Proxy {
    pub fn new(
        listen_addr: SocketAddr,
        panel_addr: SocketAddr,
        cipher: Cipher,
        checksum_mode: ChecksumMode,
        handlers: Vec<Arc<Mutex<dyn pipeline::FrameHandler>>>,
    ) -> Self {
        Self {
            listen_addr,
            panel_addr,
            cipher,
            checksum_mode,
            handle: InterceptorHandle::empty(),
            handlers,
            on_listening: Mutex::new(None),
        }
    }

    /// Get a handle for sending commands to the proxy.
    ///
    /// The handle remains valid across resets — its inner coordinator ref
    /// is swapped each cycle.
    pub fn handle(&self) -> InterceptorHandle {
        self.handle.clone()
    }

    /// Returns a oneshot receiver that fires when the proxy first starts
    /// listening for the server connection.
    pub fn on_listening(&self) -> tokio::sync::oneshot::Receiver<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        *self.on_listening.lock().unwrap() = Some(tx);
        rx
    }

    /// Run the proxy in a loop, resetting on disconnect.
    ///
    /// Each cycle establishes all four connections and spawns actors.
    /// When either side disconnects, everything is torn down and the cycle
    /// restarts. The [`InterceptorHandle`] from [`handle()`] stays valid
    /// across resets.
    pub async fn run(&self) -> ! {
        loop {
            match self.cycle().await {
                Ok(coordinator_handle) => {
                    let _ = coordinator_handle.await;
                    info!("Disconnected, resetting...");
                }
                Err(e) => {
                    error!("Connection failed: {e}, retrying...");
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        }
    }

    /// A single connect → run → disconnect cycle.
    async fn cycle(&self) -> std::io::Result<tokio::task::JoinHandle<()>> {
        let listen_addr = self.listen_addr;
        let panel_addr = self.panel_addr;
        let cipher = &self.cipher;
        let checksum_mode = self.checksum_mode;

        // --- Phase 1: Accept the server's primary connection ---
        let server_listener = TcpListener::bind(listen_addr).await?;
        info!("Listening for server on {listen_addr}...");

        if let Some(tx) = self.on_listening.lock().unwrap().take() {
            let _ = tx.send(());
        }

        let (server_primary, server_peer) = server_listener.accept().await?;
        info!("Server connected from {server_peer}");

        // --- Phase 2: Connect to the real panel + listen for its async callback ---
        let panel_async_bind = SocketAddr::new("0.0.0.0".parse().unwrap(), panel_addr.port() + 1);
        let panel_async_listener = TcpListener::bind(panel_async_bind).await?;
        info!("Listening for panel async on {panel_async_bind}");

        info!("Connecting to panel at {panel_addr}...");
        let panel_primary = TcpStream::connect(panel_addr).await?;
        info!("Panel primary connected");

        // --- Phase 3: Accept the panel's async connection (2s timeout) ---
        info!("Waiting for panel async callback...");
        let (panel_async, panel_async_peer) = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            panel_async_listener.accept(),
        )
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "panel async callback timed out (2s)"))?
        ?;
        info!("Panel async connected from {panel_async_peer}");

        // --- Phase 4: Connect to the server's async listener ---
        let server_async_addr = SocketAddr::new(server_peer.ip(), listen_addr.port() + 1);
        info!("Connecting to server async at {server_async_addr}...");
        let server_async = TcpStream::connect(server_async_addr).await?;
        info!("Server async connected");

        // --- Split all four streams ---
        let (sp_read, sp_write) = server_primary.into_split();
        let (sa_read, sa_write) = server_async.into_split();
        let (pp_read, pp_write) = panel_primary.into_split();
        let (pa_read, pa_write) = panel_async.into_split();

        // --- Spawn writer actors ---
        let (panel_primary_w, _) = Actor::spawn(
            Some("panel-primary-writer".into()),
            WriterActor,
            WriterArgs {
                writer: pp_write,
                cipher: cipher.clone(),
            },
        )
        .await
        .expect("failed to spawn panel primary writer");

        let (panel_async_w, _) = Actor::spawn(
            Some("panel-async-writer".into()),
            WriterActor,
            WriterArgs {
                writer: pa_write,
                cipher: cipher.clone(),
            },
        )
        .await
        .expect("failed to spawn panel async writer");

        let (server_primary_w, _) = Actor::spawn(
            Some("server-primary-writer".into()),
            WriterActor,
            WriterArgs {
                writer: sp_write,
                cipher: cipher.clone(),
            },
        )
        .await
        .expect("failed to spawn server primary writer");

        let (server_async_w, _) = Actor::spawn(
            Some("server-async-writer".into()),
            WriterActor,
            WriterArgs {
                writer: sa_write,
                cipher: cipher.clone(),
            },
        )
        .await
        .expect("failed to spawn server async writer");

        // --- Spawn the coordinator ---
        let (coordinator, coordinator_handle) = Actor::spawn(
            Some("coordinator".into()),
            CoordinatorActor,
            CoordinatorArgs {
                panel_primary: panel_primary_w,
                panel_async: panel_async_w,
                server_primary: server_primary_w,
                server_async: server_async_w,
                handlers: self.handlers.clone(),
            },
        )
        .await
        .expect("failed to spawn coordinator");

        // --- Spawn reader tasks ---
        spawn_reader(
            sp_read,
            cipher.clone(),
            Ac215PacketDirection::ToPanel,
            checksum_mode,
            Side::Server,
            Channel::Primary,
            coordinator.clone(),
        );
        spawn_reader(
            sa_read,
            cipher.clone(),
            Ac215PacketDirection::ToPanel,
            ChecksumMode::ForceModern,
            Side::Server,
            Channel::Async,
            coordinator.clone(),
        );
        spawn_reader(
            pp_read,
            cipher.clone(),
            Ac215PacketDirection::FromPanel,
            checksum_mode,
            Side::Panel,
            Channel::Primary,
            coordinator.clone(),
        );
        spawn_reader(
            pa_read,
            cipher.clone(),
            Ac215PacketDirection::FromPanel,
            ChecksumMode::ForceModern,
            Side::Panel,
            Channel::Async,
            coordinator.clone(),
        );

        info!("All actors running");

        self.handle.set_coordinator(coordinator);

        Ok(coordinator_handle)
    }
}
