mod coordinator;
pub mod message;
mod reader;
mod writer;

pub use coordinator::CoordinatorActor;
pub use message::{CoordinatorMsg, Side, WriterMsg};
pub use reader::spawn_reader;
pub use writer::WriterActor;

use std::net::SocketAddr;

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
pub struct Proxy {
    coordinator_handle: tokio::task::JoinHandle<()>,
}

impl Proxy {
    pub async fn start(
        listen_addr: SocketAddr,
        panel_addr: SocketAddr,
        cipher: Cipher,
        checksum_mode: ChecksumMode,
    ) -> std::io::Result<Self> {
        // --- Phase 1: Accept the server's primary connection ---
        let server_listener = TcpListener::bind(listen_addr).await?;
        println!("[proxy] Listening for server on {listen_addr}...");

        let (server_primary, server_peer) = server_listener.accept().await?;
        println!("[proxy] Server connected from {server_peer}");

        // --- Phase 2: Connect to the real panel + listen for its async callback ---
        let panel_async_bind = SocketAddr::new("0.0.0.0".parse().unwrap(), panel_addr.port() + 1);
        let panel_async_listener = TcpListener::bind(panel_async_bind).await?;
        println!("[proxy] Listening for panel async on {panel_async_bind}");

        println!("[proxy] Connecting to panel at {panel_addr}...");
        let panel_primary = TcpStream::connect(panel_addr).await?;
        println!("[proxy] Panel primary connected");

        // --- Phase 3: Accept the panel's async connection ---
        println!("[proxy] Waiting for panel async callback...");
        let (panel_async, panel_async_peer) = panel_async_listener.accept().await?;
        println!("[proxy] Panel async connected from {panel_async_peer}");

        // --- Phase 4: Connect to the server's async listener ---
        let server_async_addr = SocketAddr::new(server_peer.ip(), listen_addr.port() + 1);
        println!("[proxy] Connecting to server async at {server_async_addr}...");
        let server_async = TcpStream::connect(server_async_addr).await?;
        println!("[proxy] Server async connected");

        // --- Split all four streams ---
        let (sp_read, sp_write) = server_primary.into_split();
        let (sa_read, sa_write) = server_async.into_split();
        let (pp_read, pp_write) = panel_primary.into_split();
        let (pa_read, pa_write) = panel_async.into_split();

        // --- Spawn writer actors ---
        let (panel_primary_w, _) = Actor::spawn(
            Some("panel-primary-writer".into()),
            WriterActor,
            WriterArgs { writer: pp_write, cipher: cipher.clone() },
        )
        .await
        .expect("failed to spawn panel primary writer");

        let (panel_async_w, _) = Actor::spawn(
            Some("panel-async-writer".into()),
            WriterActor,
            WriterArgs { writer: pa_write, cipher: cipher.clone() },
        )
        .await
        .expect("failed to spawn panel async writer");

        let (server_primary_w, _) = Actor::spawn(
            Some("server-primary-writer".into()),
            WriterActor,
            WriterArgs { writer: sp_write, cipher: cipher.clone() },
        )
        .await
        .expect("failed to spawn server primary writer");

        let (server_async_w, _) = Actor::spawn(
            Some("server-async-writer".into()),
            WriterActor,
            WriterArgs { writer: sa_write, cipher: cipher.clone() },
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
            },
        )
        .await
        .expect("failed to spawn coordinator");

        // --- Spawn reader tasks ---
        spawn_reader(
            sp_read, cipher.clone(),
            Ac215PacketDirection::ToPanel, checksum_mode,
            Side::Server, Channel::Primary,
            coordinator.clone(),
        );
        spawn_reader(
            sa_read, cipher.clone(),
            Ac215PacketDirection::ToPanel, ChecksumMode::ForceModern,
            Side::Server, Channel::Async,
            coordinator.clone(),
        );
        spawn_reader(
            pp_read, cipher.clone(),
            Ac215PacketDirection::FromPanel, checksum_mode,
            Side::Panel, Channel::Primary,
            coordinator.clone(),
        );
        spawn_reader(
            pa_read, cipher.clone(),
            Ac215PacketDirection::FromPanel, ChecksumMode::ForceModern,
            Side::Panel, Channel::Async,
            coordinator,
        );

        println!("[proxy] All actors running");

        Ok(Self { coordinator_handle })
    }

    /// Block until the proxy shuts down (due to a disconnect on either side).
    pub async fn run(self) {
        let _ = self.coordinator_handle.await;
        println!("[proxy] Shut down");
    }
}
