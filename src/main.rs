mod infra;
mod models;
mod services;

use std::net::{Ipv4Addr, SocketAddr};

use axum::{routing, Router};
use services::get_mongo_client;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or("debug,hyper=off".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    dotenv::dotenv().ok();

    let db = get_mongo_client()
        .await
        .expect("Expected to create mongo client")
        .database("oh_hell");

    let app = Router::new()
        .route("/ws", routing::get(infra::ws_handler))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .fallback(infra::fallback_handler)
        .with_state(db);

    let address = (Ipv4Addr::UNSPECIFIED, 3000);

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("Expected to bind to network address");

    tracing::info!("Listening on {:?}", address);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("Expected to start axum");
}
