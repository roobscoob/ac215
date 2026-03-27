mod frame;
pub use frame::{Channel, Frame};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::{
    crypto::Cipher,
    packet::{
        Ac215Packet,
        address::Ac215Address,
        direction::Ac215PacketDirection,
        header::{Ac215Header, Ac215TransactionId, Checksum, ChecksumMode, EventFlag, ParseError},
    },
};

type PendingMap = Arc<Mutex<HashMap<Ac215TransactionId, oneshot::Sender<Frame>>>>;
type ChecksumOverrides = Arc<Mutex<HashMap<Ac215TransactionId, Checksum>>>;

pub struct Server {
    cipher: Cipher,
    primary_writer: Arc<Mutex<OwnedWriteHalf>>,
    primary_tx_id: u8,
    unsolicited: mpsc::Receiver<Frame>,
    pending: PendingMap,
    checksum_overrides: ChecksumOverrides,
    async_writer: Arc<Mutex<OwnedWriteHalf>>,
    async_tx_id: u8,
}

impl Server {
    /// Connect to a panel on its primary port and establish the async channel.
    ///
    /// 1. Bind the async listener on port + 1
    /// 2. Connect to the panel's primary port
    /// 3. Wait for the panel to connect back on port + 1 (in parallel with the primary handshake)
    /// 4. Spawn receive loops for both channels
    pub async fn connect(
        panel_addr: SocketAddr,
        cipher: Cipher,
        checksum_mode: ChecksumMode,
    ) -> std::io::Result<Self> {
        // 1. Listen on port + 1 before connecting, so it's ready when the panel tries
        let async_listen_addr = SocketAddr::new("0.0.0.0".parse().unwrap(), panel_addr.port() + 1);
        println!("[server] Binding async listener on {}", async_listen_addr);
        let async_listener = TcpListener::bind(async_listen_addr).await?;

        // 2. Connect to the panel's primary port
        println!("[server] Connecting to panel at {}...", panel_addr);
        let primary_stream = TcpStream::connect(panel_addr).await?;
        println!("[server] Primary channel connected");

        // 3. Accept the panel's async connection (panel connects as soon as it sees us)
        println!("[server] Waiting for panel to connect on async channel...");
        let (async_stream, async_peer) = async_listener.accept().await?;
        println!("[server] Async channel connected from {}", async_peer);

        // Split both streams
        let (primary_reader, primary_writer) = primary_stream.into_split();
        let (async_reader, async_writer) = async_stream.into_split();
        println!("[server] Both channels established, spawning receive loops");

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let checksum_overrides: ChecksumOverrides = Arc::new(Mutex::new(HashMap::new()));
        let (unsolicited_tx, unsolicited_rx) = mpsc::channel(64);

        // 4. Spawn receive loops for both channels
        Self::spawn_recv_loop(
            primary_reader,
            cipher.clone(),
            checksum_mode,
            Channel::Primary,
            pending.clone(),
            checksum_overrides.clone(),
            unsolicited_tx.clone(),
        );

        Self::spawn_recv_loop(
            async_reader,
            cipher.clone(),
            ChecksumMode::ForceModern,
            Channel::Async,
            pending.clone(),
            Arc::new(Mutex::new(HashMap::new())),
            unsolicited_tx,
        );

        Ok(Self {
            cipher,
            primary_writer: Arc::new(Mutex::new(primary_writer)),
            primary_tx_id: 0,
            unsolicited: unsolicited_rx,
            pending,
            checksum_overrides,
            async_writer: Arc::new(Mutex::new(async_writer)),
            async_tx_id: 0,
        })
    }

    /// Receive the next unsolicited frame from either channel.
    pub async fn recv(&mut self) -> Option<Frame> {
        self.unsolicited.recv().await
    }

    /// Send on the primary channel without waiting for a reply.
    pub async fn send(
        &mut self,
        destination: Ac215Address,
        packet: impl Ac215Packet,
        checksum_mode: ChecksumMode,
    ) -> std::io::Result<()> {
        let (bytes, _, _) = Self::build_frame(
            &self.cipher,
            packet,
            destination,
            EventFlag::Unset,
            &mut self.primary_tx_id,
            checksum_mode,
        );

        let mut writer = self.primary_writer.lock().await;
        writer.write_all(&bytes).await
    }

    /// Send a packet on the primary channel and wait for the reply with matching tx ID.
    pub async fn request(
        &mut self,
        destination: Ac215Address,
        packet: impl Ac215Packet,
        checksum_mode: ChecksumMode,
    ) -> Result<Frame, ParseError> {
        let (bytes, tx_id, resolved_checksum) = Self::build_frame(
            &self.cipher,
            packet,
            destination,
            EventFlag::Unset,
            &mut self.primary_tx_id,
            checksum_mode,
        );

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(tx_id, tx);
        self.checksum_overrides
            .lock()
            .await
            .insert(tx_id, resolved_checksum);

        {
            let mut writer = self.primary_writer.lock().await;
            writer
                .write_all(&bytes)
                .await
                .map_err(ParseError::IoError)?;
        }

        rx.await.map_err(|_| {
            ParseError::IoError(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "receive loop exited before reply arrived",
            ))
        })
    }

    /// Send on the async channel.
    pub async fn send_async(
        &mut self,
        destination: Ac215Address,
        packet: impl Ac215Packet,
    ) -> std::io::Result<()> {
        let (bytes, _, _) = Self::build_frame(
            &self.cipher,
            packet,
            destination,
            EventFlag::Unset,
            &mut self.async_tx_id,
            ChecksumMode::ForceModern,
        );

        let mut writer = self.async_writer.lock().await;
        writer.write_all(&bytes).await
    }

    fn spawn_recv_loop(
        reader: tokio::net::tcp::OwnedReadHalf,
        cipher: Cipher,
        checksum_mode: ChecksumMode,
        channel: Channel,
        pending: PendingMap,
        checksum_overrides: ChecksumOverrides,
        unsolicited_tx: mpsc::Sender<Frame>,
    ) {
        tokio::spawn(async move {
            let mut reader = reader;
            loop {
                let overrides = checksum_overrides.lock().await.clone();
                let result = Ac215Header::parse_frame(
                    Ac215PacketDirection::FromPanel,
                    cipher.clone(),
                    &mut reader,
                    checksum_mode,
                    &overrides,
                )
                .await;

                match result {
                    Ok((header, payload)) => {
                        let tx_id = header.transaction_id();
                        checksum_overrides.lock().await.remove(&tx_id);
                        let frame = Frame::new(header, payload, channel);

                        let sender = pending.lock().await.remove(&tx_id);
                        if let Some(sender) = sender {
                            let _ = sender.send(frame);
                        } else {
                            let _ = unsolicited_tx.send(frame).await;
                        }
                    }
                    Err(e) => {
                        println!("[server] Error parsing frame: {:?}", e);
                        break;
                    }
                }
            }
        });
    }

    fn build_frame(
        cipher: &Cipher,
        packet: impl Ac215Packet,
        destination: Ac215Address,
        event_flag: EventFlag,
        tx_id: &mut u8,
        checksum: ChecksumMode,
    ) -> (Vec<u8>, Ac215TransactionId, Checksum) {
        let command_id = packet.packet_id();
        let tid = Ac215TransactionId::from_byte(*tx_id);
        *tx_id = tx_id.wrapping_add(1);

        let mut tmp = [0u8; 467];
        let data_length = packet.into_bytes(&mut tmp);

        let header = Ac215Header::new(
            Ac215PacketDirection::ToPanel,
            Ac215Address::SERVER,
            destination,
            tid,
            data_length,
            command_id,
            event_flag,
            checksum,
        )
        .expect("packet too large for frame");

        let resolved = header.checksum();
        let bytes = header.into_frame(cipher.clone(), &tmp[..data_length as usize]);
        (bytes, tid, resolved)
    }
}
