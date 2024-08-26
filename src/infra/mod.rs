use std::{net::SocketAddr, ops::ControlFlow};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::IntoResponse,
};

use mongodb::bson::oid::ObjectId;

use crate::{models::Turn, services::manager::Manager};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(who): ConnectInfo<SocketAddr>,
    State(manager): State<Manager>,
) -> impl IntoResponse {
    tracing::info!(">>>> {who} connected");

    ws.on_upgrade(move |socket| async move {
        match handle(socket, who, manager).await {
            Ok(_) => tracing::warn!(">>>> {who} closed normally"),
            Err(e) => tracing::error!(">>>> exited because: {}", e),
        }
    })
}

async fn handle(socket: WebSocket, who: SocketAddr, manager: Manager) -> Result<(), Error> {
    Ok(())
}

fn process_message(msg: Message, who: SocketAddr) -> ControlFlow<(), ()> {
    match msg {
        Message::Text(message) => {
            tracing::debug!(">>>> {who} sent str: {message:?}");

            let message: ClientMessage = match serde_json::from_str(&message) {
                Ok(m) => m,
                Err(_) => return ControlFlow::Break(()),
            };

            match message {
                ClientMessage::Lobby(l) => todo!(),
                ClientMessage::Game(g) => todo!(),
            };
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

#[derive(serde::Deserialize)]
pub enum ClientLobbyMessage {
    RequestLobbies,
    CreateLobby,
    JoinLobby { lobby_id: ObjectId },
    StartGame { game_id: ObjectId },
}

#[derive(serde::Deserialize)]
pub enum ClientGameMessage {
    PlayTurn(Turn),
    Bid { player_id: ObjectId, bid: usize },
}

#[derive(serde::Deserialize)]
pub enum ClientMessage {
    Lobby(ClientLobbyMessage),
    Game(ClientGameMessage),
}

#[derive(serde::Serialize)]
pub enum ServerLobbyMessage {
    GameStarted { game_id: ObjectId },
}

#[derive(serde::Serialize)]
pub enum ServerGameMessage {
    PlayerTurn(Turn),
    PlayerBid { plaeyr_id: ObjectId, bid: usize },
}

#[derive(serde::Serialize)]
pub enum ServerMessage {
    Lobby(ServerLobbyMessage),
    Game(ServerGameMessage),
}
