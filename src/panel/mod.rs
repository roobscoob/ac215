use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use log::info;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc};

use crate::{
    crypto::Cipher,
    packet::{
        Ac215Packet,
        address::Ac215Address,
        direction::Ac215PacketDirection,
        header::{Ac215Header, Ac215TransactionId, ChecksumMode, EventFlag},
    },
    server::{Channel, Frame},
};

pub struct Panel {
    cipher: Cipher,
    address: Ac215Address,
    primary_writer: Arc<Mutex<OwnedWriteHalf>>,
    primary_tx_id: u8,
    unsolicited: mpsc::Receiver<Frame>,
    async_writer: Arc<Mutex<OwnedWriteHalf>>,
    async_tx_id: u8,
}

impl Panel {
    /// Start a simulated panel.
    ///
    /// 1. Listen on `listen_addr` (primary channel)
    /// 2. Wait for the server to connect
    /// 3. Connect back to the server on port + 1 (async channel)
    /// 4. Spawn receive loops for both channels
    pub async fn listen(
        listen_addr: SocketAddr,
        cipher: Cipher,
        panel_address: Ac215Address,
        checksum_mode: ChecksumMode,
    ) -> std::io::Result<Self> {
        // 1. Listen on the primary port
        info!("Binding primary listener on {}", listen_addr);
        let primary_listener = TcpListener::bind(listen_addr).await?;

        // 2. Wait for the server to connect
        info!("Waiting for server to connect...");
        let (primary_stream, server_peer) = primary_listener.accept().await?;
        info!("Server connected from {}", server_peer);

        // 3. Connect back to the server on port + 1
        let async_addr = SocketAddr::new(server_peer.ip(), listen_addr.port() + 1);
        info!("Connecting async channel to {}...", async_addr);
        let async_stream = TcpStream::connect(async_addr).await?;
        info!("Async channel connected");

        // Split both streams
        let (primary_reader, primary_writer) = primary_stream.into_split();
        let (async_reader, async_writer) = async_stream.into_split();
        info!("Both channels established, spawning receive loops");

        let (unsolicited_tx, unsolicited_rx) = mpsc::channel(64);

        // 4. Spawn receive loops — panel receives ToPanel frames
        Self::spawn_recv_loop(
            primary_reader,
            cipher.clone(),
            checksum_mode,
            Channel::Primary,
            unsolicited_tx.clone(),
        );

        Self::spawn_recv_loop(
            async_reader,
            cipher.clone(),
            ChecksumMode::ForceModern,
            Channel::Async,
            unsolicited_tx,
        );

        Ok(Self {
            cipher,
            address: panel_address,
            primary_writer: Arc::new(Mutex::new(primary_writer)),
            primary_tx_id: 0,
            unsolicited: unsolicited_rx,
            async_writer: Arc::new(Mutex::new(async_writer)),
            async_tx_id: 0,
        })
    }

    /// Receive the next frame from either channel.
    pub async fn recv(&mut self) -> Option<Frame> {
        self.unsolicited.recv().await
    }

    /// Send a reply on the primary channel, echoing the request's transaction ID and checksum mode.
    pub async fn reply(
        &self,
        request: &Frame,
        packet: impl Ac215Packet,
    ) -> std::io::Result<()> {
        let command_id = packet.packet_id();
        let mut tmp = [0u8; 467];
        let data_length = packet.into_bytes(&mut tmp);

        let header = Ac215Header::new(
            Ac215PacketDirection::FromPanel,
            self.address,
            Ac215Address::SERVER,
            request.header().transaction_id(),
            data_length,
            command_id,
            EventFlag::Unset,
            request.header().checksum().into(),
        )
        .expect("packet too large for frame");

        let bytes = header.into_frame(self.cipher.clone(), &tmp[..data_length as usize]);
        let mut writer = self.primary_writer.lock().await;
        writer.write_all(&bytes).await
    }

    /// Send on the primary channel.
    pub async fn send(
        &mut self,
        packet: impl Ac215Packet,
        checksum_mode: ChecksumMode,
    ) -> std::io::Result<()> {
        let (bytes, _) = Self::build_frame(
            &self.cipher,
            self.address,
            packet,
            EventFlag::Unset,
            &mut self.primary_tx_id,
            checksum_mode,
        );

        let mut writer = self.primary_writer.lock().await;
        writer.write_all(&bytes).await
    }

    /// Send on the async channel.
    pub async fn send_async(
        &mut self,
        packet: impl Ac215Packet,
    ) -> std::io::Result<()> {
        let (bytes, _) = Self::build_frame(
            &self.cipher,
            self.address,
            packet,
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
        unsolicited_tx: mpsc::Sender<Frame>,
    ) {
        tokio::spawn(async move {
            let mut reader = reader;
            let no_overrides = HashMap::new();
            loop {
                let result = Ac215Header::parse_frame(
                    Ac215PacketDirection::ToPanel,
                    cipher.clone(),
                    &mut reader,
                    checksum_mode,
                    &no_overrides,
                )
                .await;

                match result {
                    Ok((header, payload)) => {
                        let frame = Frame::new(header, payload, channel);
                        let _ = unsolicited_tx.send(frame).await;
                    }
                    Err(_) => break,
                }
            }
        });
    }

    fn build_frame(
        cipher: &Cipher,
        address: Ac215Address,
        packet: impl Ac215Packet,
        event_flag: EventFlag,
        tx_id: &mut u8,
        checksum: ChecksumMode,
    ) -> (Vec<u8>, Ac215TransactionId) {
        let command_id = packet.packet_id();
        let tid = Ac215TransactionId::from_byte(*tx_id);
        *tx_id = tx_id.wrapping_add(1);

        let mut tmp = [0u8; 467];
        let data_length = packet.into_bytes(&mut tmp);

        let header = Ac215Header::new(
            Ac215PacketDirection::FromPanel,
            address,
            Ac215Address::SERVER,
            tid,
            data_length,
            command_id,
            event_flag,
            checksum,
        )
        .expect("packet too large for frame");

        let bytes = header.into_frame(cipher.clone(), &tmp[..data_length as usize]);
        (bytes, tid)
    }
}
