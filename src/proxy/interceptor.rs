use std::future::Future;
use std::sync::{Arc, RwLock};

use ractor::ActorRef;
use tokio::sync::oneshot;

use crate::packet::Ac215Packet;
use crate::packet::address::Addressing;
use crate::packet::header::{ChecksumMode, EventFlag};
use crate::server::Channel;
use crate::server::Frame;

use super::message::{CoordinatorMsg, Side};

/// Serialize a packet into (command_id, data).
fn serialize_packet<P: Ac215Packet>(packet: P) -> (u8, Vec<u8>) {
    let mut buf = [0u8; 468];
    let command_id = packet.packet_id();
    let data_len = packet.into_bytes(&mut buf);
    let data = buf[..data_len as usize].to_vec();
    (command_id, data)
}

/// A discrete operation that may need recovery if the proxy resets mid-execution.
///
/// Implement this trait for any operation that touches device state through the
/// proxy. The struct fields capture whatever state `execute` needs to record so
/// that `recover` can undo or reconcile on reconnect.
pub trait Transaction: Send {
    type Result;

    /// Run the operation. Use `handle` to send packets, block channels, etc.
    /// Mutate `&mut self` to record progress for recovery.
    fn execute(
        &mut self,
        handle: &InterceptorHandle,
    ) -> impl Future<Output = Self::Result> + Send;

    /// Called after a proxy reset if `execute` did not complete.
    /// Use the recorded state in `self` to bring the device back to a known-good
    /// state.
    fn recover(
        &self,
        handle: &InterceptorHandle,
    ) -> impl Future<Output = ()> + Send;
}

/// Handle given to interceptor tasks to communicate with the coordinator.
///
/// All methods that send messages to the coordinator are ordered — the
/// coordinator processes them sequentially, so `block` followed by `request`
/// is guaranteed to block before the request is sent.
///
/// The inner coordinator ref is swapped on reset, so handles remain valid
/// across proxy cycles.
#[derive(Clone)]
pub struct InterceptorHandle {
    coordinator: Arc<RwLock<Option<ActorRef<CoordinatorMsg>>>>,
}

impl InterceptorHandle {
    /// Create a handle not yet connected to a coordinator.
    /// Call [`set_coordinator`] before using.
    pub(crate) fn empty() -> Self {
        Self {
            coordinator: Arc::new(RwLock::new(None)),
        }
    }

    pub fn new(coordinator: ActorRef<CoordinatorMsg>) -> Self {
        Self {
            coordinator: Arc::new(RwLock::new(Some(coordinator))),
        }
    }

    /// Update the inner coordinator ref. Called by the proxy on reset.
    pub(crate) fn set_coordinator(&self, coordinator: ActorRef<CoordinatorMsg>) {
        *self.coordinator.write().unwrap() = Some(coordinator);
    }

    fn coordinator(&self) -> ActorRef<CoordinatorMsg> {
        self.coordinator.read().unwrap().clone().expect("handle used before proxy is connected")
    }

    /// Run a transaction. Blocks the server→panel primary channel for the
    /// entire duration. The transaction's `recover` method will be called
    /// (also under a block) if the proxy resets before `execute` completes.
    pub async fn run_transaction<T: Transaction>(&self, txn: &mut T) -> T::Result {
        self.block_server_primary();
        let result = txn.execute(self).await;
        self.unblock_server_primary();
        result
    }

    /// Send a request to the panel on the primary channel and wait for the
    /// response. The coordinator assigns the correct transaction ID.
    pub async fn request<P: Ac215Packet>(
        &self,
        addressing: Addressing,
        event_flag: EventFlag,
        checksum_mode: ChecksumMode,
        packet: P,
    ) -> Frame {
        let (command_id, data) = serialize_packet(packet);

        let (tx, rx) = oneshot::channel::<Frame>();
        let _ = self.coordinator().send_message(CoordinatorMsg::SendRequest {
            addressing,
            command_id,
            data,
            event_flag,
            checksum_mode,
            response_tx: tx,
        });
        rx.await.expect("coordinator dropped before responding")
    }

    /// Fire-and-forget: send a packet to the specified target without tracking
    /// the response.
    pub fn send<P: Ac215Packet>(
        &self,
        target: Side,
        channel: Channel,
        addressing: Addressing,
        event_flag: EventFlag,
        checksum_mode: ChecksumMode,
        packet: P,
    ) {
        let (command_id, data) = serialize_packet(packet);
        let _ = self.coordinator().send_message(CoordinatorMsg::Send {
            target,
            channel,
            addressing,
            command_id,
            data,
            event_flag,
            checksum_mode,
        });
    }

    /// Start buffering server→panel primary frames.
    pub fn block_server_primary(&self) {
        let _ = self
            .coordinator()
            .send_message(CoordinatorMsg::BlockServerPrimary);
    }

    /// Flush buffered server→panel primary frames and resume normal flow.
    pub fn unblock_server_primary(&self) {
        let _ = self
            .coordinator()
            .send_message(CoordinatorMsg::UnblockServerPrimary);
    }
}
