mod outputs;
pub mod response;
pub mod webhooks;

use std::net::SocketAddr;

use axum::routing::{delete, get};
use axum::Router;
use log::info;

use crate::AppState;

pub async fn serve(addr: SocketAddr, state: AppState) {
    let app = Router::new()
        .route(
            "/v1/panel/outputs",
            get(outputs::get_outputs).post(outputs::post_outputs),
        )
        .route("/v1/webhooks", get(webhooks::list_webhooks).post(webhooks::create_webhook))
        .route("/v1/webhooks/{id}", delete(webhooks::delete_webhook))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("API server listening on {addr}");
    axum::serve(listener, app).await.unwrap();
}
