use std::{net::SocketAddr, ops::ControlFlow};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, WebSocketUpgrade,
    },
    http::StatusCode,
    response::IntoResponse,
};

use futures::stream::StreamExt;
use mongodb::bson::oid::ObjectId;

use crate::models::Game;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(who): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    tracing::info!(">>>> {who} connected");

    ws.on_upgrade(move |socket| async move {
        match handle(socket, who).await {
            Ok(_) => tracing::warn!(">>>> {who} closed normally"),
            Err(e) => tracing::error!(">>>> exited because: {}", e),
        }
    })
}

async fn handle(socket: WebSocket, who: SocketAddr) -> Result<(), Error> {
    let (mut _sender, mut receiver) = socket.split();

    let a = Game::new(vec![ObjectId::new()]);

    let mut send_task = tokio::spawn(async move { loop {} });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if process_message(msg, who).is_break() {
                break;
            }
        }
    });

    tokio::select! {
        rv_a = (&mut send_task) => {
            match rv_a {
                Ok(_) => {},
                Err(e) => tracing::error!("Error sending messages {e:?}")
            }
            recv_task.abort();
        },

        rv_b = (&mut recv_task) => {
            match rv_b {
                Ok(_) => {},
                Err(e) => tracing::error!("Error receiving messages {e:?}")
            }
            send_task.abort();
        }
    }

    Ok(())
}

fn process_message(msg: Message, who: SocketAddr) -> ControlFlow<(), ()> {
    match msg {
        Message::Text(t) => {
            tracing::debug!(">>>> {who} sent str: {t:?}");
        }
        Message::Close(c) => {
            let reason = c
                .map(|c| format!(" | reason: {} {}", c.code, c.reason))
                .unwrap_or_default();

            tracing::warn!(">>>> {who} sent close message{}", reason);

            return ControlFlow::Break(());
        }
        _ => {}
    }

    ControlFlow::Continue(())
}

pub async fn fallback_handler() -> (StatusCode, &'static str) {
    NOT_FOUND_RESPONSE
}

const NOT_FOUND_RESPONSE: (StatusCode, &str) =
    (StatusCode::NOT_FOUND, "this resource doesn't exist");

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0} disconnected from websocket")]
    Disconnected(SocketAddr),
    #[error("Database error: {0}")]
    Database(#[from] mongodb::error::Error),
}
