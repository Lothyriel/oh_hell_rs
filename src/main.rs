mod infra;
mod models;
mod services;

use std::net::{Ipv4Addr, SocketAddr};

use axum::{routing, Router};
use infra::auth::JWT_KEY;
use services::{
    manager::Manager,
    repositories::{auth::AuthRepository, game::GamesRepository, get_mongo_client},
};

use tower_http::cors::{AllowOrigin, Any, CorsLayer};
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

    JWT_KEY
        .set(std::env::var("JWT_KEY").expect("JWT_KEY var is missing"))
        .expect("Should set jwt key value");

    let db = get_mongo_client()
        .await
        .expect("Expected to create mongo client")
        .database("oh_hell");

    let manager = Manager::new(GamesRepository::new(&db), AuthRepository::new(&db));

    let auth_layer = axum::middleware::from_fn_with_state(manager.clone(), infra::auth::middleware);

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(vec![
            "https://fodinha.click".parse().expect("Valid url"),
            "http://localhost:4200".parse().expect("Valid url"),
        ]))
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/game", routing::get(infra::game::ws_handler))
        .nest("/lobby", infra::lobby::router().layer(auth_layer))
        .nest("/auth", infra::auth::router())
        .fallback(infra::fallback_handler)
        .with_state(manager)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors);

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
