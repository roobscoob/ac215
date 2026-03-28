use std::sync::Arc;

use hmac::{Hmac, Mac};
use log::{info, warn};
use sha2::Sha256;

use crate::local_db::{LocalDb, Webhook};

type HmacSha256 = Hmac<Sha256>;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

/// Sign a payload with the webhook's signing key using HMAC-SHA256.
/// Returns the hex-encoded signature.
fn sign(key: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Deliver a JSON payload to a single webhook with retries.
async fn deliver_one(client: &reqwest::Client, webhook: &Webhook, body: &[u8]) {
    let signature = sign(&webhook.signing_key, body);

    for attempt in 1..=MAX_RETRIES {
        let result = client
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Signature", &signature)
            .header("X-Webhook-Id", &webhook.id)
            .body(body.to_vec())
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                info!(
                    webhook_id:% = webhook.id,
                    url:% = webhook.url;
                    "webhook delivered",
                );
                return;
            }
            Ok(resp) => {
                warn!(
                    webhook_id:% = webhook.id,
                    url:% = webhook.url,
                    status:% = resp.status(),
                    attempt = attempt,
                    max_retries = MAX_RETRIES;
                    "webhook delivery got non-success status",
                );
            }
            Err(e) => {
                warn!(
                    webhook_id:% = webhook.id,
                    url:% = webhook.url,
                    error:% = e,
                    attempt = attempt,
                    max_retries = MAX_RETRIES;
                    "webhook delivery failed",
                );
            }
        }

        if attempt < MAX_RETRIES {
            tokio::time::sleep(RETRY_DELAY).await;
        }
    }

    warn!(
        webhook_id:% = webhook.id,
        url:% = webhook.url,
        max_retries = MAX_RETRIES;
        "webhook delivery gave up",
    );
}

/// Fan out a JSON payload to all registered webhooks.
///
/// Each delivery is spawned as an independent task so a slow/failing
/// endpoint doesn't block the others or the caller.
pub fn deliver_all(store: &Arc<LocalDb>, payload: serde_json::Value) {
    info!(
        payload:serde = payload;
        "delivering webhook event",
    );

    let webhooks = match store.list_webhooks() {
        Ok(w) => w,
        Err(e) => {
            warn!(error:% = e; "failed to list webhooks");
            return;
        }
    };

    if webhooks.is_empty() {
        info!("no webhooks registered, skipping delivery");
        return;
    }

    let body: Vec<u8> = serde_json::to_vec(&payload).expect("failed to serialize payload");
    let client = reqwest::Client::new();

    for webhook in webhooks {
        let client = client.clone();
        let body = body.clone();
        tokio::spawn(async move {
            deliver_one(&client, &webhook, &body).await;
        });
    }
}
