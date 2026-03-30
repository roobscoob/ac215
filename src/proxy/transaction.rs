use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::oneshot;

use crate::packet::header::{Ac215Header, Ac215TransactionId};
use crate::packet::named_id::NamedPacketId;
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

/// Metadata stored alongside each pending transaction.
struct PendingEntry {
    origin: TxnOrigin,
    command_id: u8,
    sent_at: u64,
}

/// Summary of a pending transaction for health reporting.
#[derive(Serialize)]
pub struct PendingSummary {
    pub command_id: u8,
    pub command_name: Option<&'static str>,
    pub origin: &'static str,
    pub sent_at: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
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
    pending: HashMap<Ac215TransactionId, PendingEntry>,
}

impl TransactionRewriter {
    pub fn new() -> Self {
        Self {
            panel_next: None,
            pending: HashMap::new(),
        }
    }

    /// Rewrite a server→panel frame's transaction ID.
    ///
    /// If `track` is true, the request is recorded as in-flight so the
    /// response can be routed back. Use `false` for fire-and-forget packets
    /// and all async-channel traffic.
    pub fn forward_to_panel(&mut self, header: Ac215Header, track: bool) -> Ac215Header {
        let server_txn = header.transaction_id();
        let command_id = header.command_id();
        let panel_txn = self.next_panel_txn(server_txn);

        if track {
            self.pending.insert(panel_txn, PendingEntry {
                origin: TxnOrigin::Server(server_txn),
                command_id,
                sent_at: now_ms(),
            });
        }

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
        let command_id = header.command_id();
        self.panel_next = Some(panel_txn.next());
        self.pending.insert(panel_txn, PendingEntry {
            origin: TxnOrigin::Proxy(response_tx),
            command_id,
            sent_at: now_ms(),
        });
        header.with_transaction_id(panel_txn)
    }

    /// Resolve a panel→server response: remap the transaction ID and determine
    /// where it should be routed.
    pub fn resolve_response(&mut self, frame: Frame) -> ResponseRouting {
        let panel_txn = frame.header().transaction_id();
        match self.pending.remove(&panel_txn) {
            Some(PendingEntry { origin: TxnOrigin::Server(server_txn), .. }) => {
                let (header, payload, channel) = frame.into_parts();
                let header = header.with_transaction_id(server_txn);
                ResponseRouting::ToServer(Frame::new(header, payload, channel))
            }
            Some(PendingEntry { origin: TxnOrigin::Proxy(tx), .. }) => {
                ResponseRouting::ToProxy(frame, tx)
            }
            None => ResponseRouting::Unknown(frame),
        }
    }

    /// Number of in-flight requests awaiting panel responses.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Summary of all pending transactions for health/debug reporting.
    pub fn pending_summary(&self) -> Vec<PendingSummary> {
        self.pending.values().map(|entry| {
            PendingSummary {
                command_id: entry.command_id,
                command_name: NamedPacketId(entry.command_id).name(),
                origin: match &entry.origin {
                    TxnOrigin::Server(_) => "server",
                    TxnOrigin::Proxy(_) => "proxy",
                },
                sent_at: entry.sent_at,
            }
        }).collect()
    }

    /// Remove pending entries older than `timeout_ms` and return summaries
    /// of what was pruned. Dropped `Proxy` oneshot senders will cause the
    /// caller to receive a `RecvError`, and dropped `Server` entries mean the
    /// response (if it ever arrives) will be routed as `Unknown`.
    pub fn prune_stale(&mut self, timeout_ms: u64) -> Vec<PendingSummary> {
        let cutoff = now_ms().saturating_sub(timeout_ms);
        let stale_keys: Vec<Ac215TransactionId> = self
            .pending
            .iter()
            .filter(|(_, entry)| entry.sent_at < cutoff)
            .map(|(k, _)| *k)
            .collect();

        let mut pruned = Vec::with_capacity(stale_keys.len());
        for key in stale_keys {
            if let Some(entry) = self.pending.remove(&key) {
                pruned.push(PendingSummary {
                    command_id: entry.command_id,
                    command_name: NamedPacketId(entry.command_id).name(),
                    origin: match &entry.origin {
                        TxnOrigin::Server(_) => "server",
                        TxnOrigin::Proxy(_) => "proxy",
                    },
                    sent_at: entry.sent_at,
                });
                // Proxy oneshot senders are dropped here, signalling RecvError.
            }
        }
        pruned
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
