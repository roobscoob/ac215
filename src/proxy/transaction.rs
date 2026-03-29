use std::collections::HashMap;

use tokio::sync::oneshot;

use crate::packet::header::{Ac215Header, Ac215TransactionId};
use crate::server::Frame;

/// Where a pending request originated from.
pub enum TxnOrigin {
    /// A passthrough request from the server — holds the original server-side
    /// transaction ID so we can remap the response.
    Server(Ac215TransactionId),
    /// A proxy-injected request — holds the oneshot sender to deliver the
    /// response back to the interceptor.
    Proxy(oneshot::Sender<Frame>),
}

/// Result of resolving a panel response through the rewriter.
pub enum ResponseRouting {
    /// Forward to the server (txn ID already remapped).
    ToServer(Frame),
    /// Deliver to the proxy interceptor via oneshot.
    ToProxy(Frame, oneshot::Sender<Frame>),
    /// Unknown origin — pass through unchanged.
    Unknown(Frame),
}

/// Rewrites transaction IDs on the primary channel so that both sides see
/// sequential IDs even when the proxy injects or withholds frames.
pub struct TransactionRewriter {
    /// The next transaction ID the panel expects on incoming requests.
    panel_next: Option<Ac215TransactionId>,
    /// In-flight requests keyed by the panel-side transaction ID.
    pending: HashMap<Ac215TransactionId, TxnOrigin>,
}

impl TransactionRewriter {
    pub fn new() -> Self {
        Self {
            panel_next: None,
            pending: HashMap::new(),
        }
    }

    /// Rewrite a server→panel request for forwarding to the panel.
    ///
    /// Records the mapping so the response can be remapped back.
    /// Returns the header with the panel-side transaction ID.
    pub fn forward_to_panel(&mut self, header: Ac215Header) -> Ac215Header {
        let server_txn = header.transaction_id();
        let panel_txn = self.next_panel_txn(server_txn);
        self.pending.insert(panel_txn, TxnOrigin::Server(server_txn));
        header.with_transaction_id(panel_txn)
    }

    /// Rewrite a proxy-injected request for sending to the panel.
    ///
    /// The response will be delivered through the oneshot sender instead of
    /// being forwarded to the server.
    pub fn inject_to_panel(
        &mut self,
        header: Ac215Header,
        response_tx: oneshot::Sender<Frame>,
    ) -> Ac215Header {
        let panel_txn = self
            .panel_next
            .expect("cannot inject before observing traffic");
        self.panel_next = Some(panel_txn.next());
        self.pending.insert(panel_txn, TxnOrigin::Proxy(response_tx));
        header.with_transaction_id(panel_txn)
    }

    /// Resolve a panel→server response: remap the transaction ID and determine
    /// where it should be routed.
    pub fn resolve_response(&mut self, frame: Frame) -> ResponseRouting {
        let panel_txn = frame.header().transaction_id();
        match self.pending.remove(&panel_txn) {
            Some(TxnOrigin::Server(server_txn)) => {
                let (header, payload, channel) = frame.into_parts();
                let header = header.with_transaction_id(server_txn);
                ResponseRouting::ToServer(Frame::new(header, payload, channel))
            }
            Some(TxnOrigin::Proxy(tx)) => ResponseRouting::ToProxy(frame, tx),
            None => ResponseRouting::Unknown(frame),
        }
    }

    /// Number of in-flight requests awaiting panel responses.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Whether the rewriter has observed at least one frame.
    pub fn is_tracking(&self) -> bool {
        self.panel_next.is_some()
    }

    /// Consume the next panel-side transaction ID. On the first call, we learn
    /// the starting point from the server's actual ID.
    fn next_panel_txn(&mut self, server_txn: Ac215TransactionId) -> Ac215TransactionId {
        let txn = self.panel_next.unwrap_or(server_txn);
        self.panel_next = Some(txn.next());
        txn
    }
}
