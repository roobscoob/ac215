mod credentials;
mod outputs;
pub mod response;
pub mod webhooks;

use std::net::SocketAddr;

use axum::routing::{delete, get, post};
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
        .route("/v1/users", get(credentials::list_users).post(credentials::create_user))
        .route(
            "/v1/users/{id}",
            get(credentials::get_user)
                .patch(credentials::update_user)
                .delete(credentials::delete_user),
        )
        .route("/v1/cards", get(credentials::list_cards).post(credentials::create_card))
        .route(
            "/v1/cards/lookup/{site_code}/{card_code}",
            get(credentials::lookup_card),
        )
        .route(
            "/v1/cards/{id}",
            get(credentials::get_card).delete(credentials::delete_card),
        )
        .route("/v1/cards/{id}/assign", post(credentials::assign_card))
        .route("/v1/cards/{id}/unassign", post(credentials::unassign_card))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("API server listening on {addr}");
    axum::serve(listener, app).await.unwrap();
}
