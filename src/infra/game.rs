use std::net::SocketAddr;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{stream::SplitStream, SinkExt, StreamExt};
use mongodb::bson::oid::ObjectId;

use crate::{
    models::{Card, GameState, Turn},
    services::manager::{Manager, ManagerError},
};

use super::auth::{self, UserClaims};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(who): ConnectInfo<SocketAddr>,
    State(manager): State<Manager>,
) -> impl IntoResponse {
    tracing::info!(">>>> {who} connected");

    ws.on_upgrade(move |socket| async move {
        match handle_connection(socket, who, manager).await {
            Ok(_) => tracing::warn!(">>>> {who} closed normally"),
            Err(e) => tracing::error!(">>>> exited because: {}", e),
        }
    })
}

async fn handle_connection(
    socket: WebSocket,
    who: SocketAddr,
    manager: Manager,
) -> Result<(), ManagerError> {
    let (mut sender, mut receiver) = socket.split();

    let auth = get_auth(&mut receiver).await?;

    ack_auth(&auth, &mut sender).await?;

    manager
        .store_player_connection(auth.clone(), sender)
        .await?;

    tokio::spawn(async move {
        while let Some(Ok(message)) = receiver.next().await {
            match handle_response(message, who, &manager, auth.clone()).await {
                Ok(_) => {}
                Err(error) => {
                    tracing::error!("{error} | {who} closing connection");
                    let msg = ServerMessage::Error(error.to_string());
                    if let Err(error) = manager.send_message(auth, msg).await {
                        tracing::error!("{error} | while trying to send error message")
                    }
                    break;
                }
            }
        }
    })
    .await
    .expect("This task should complete successfully");

    Ok(())
}

async fn ack_auth(
    auth: &UserClaims,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Result<(), ManagerError> {
    let welcome = ServerMessage::Authorized(auth.clone());
    let welcome = serde_json::to_string(&welcome)?;
    sender
        .send(Message::Text(welcome))
        .await
        .map_err(|_| ManagerError::PlayerDisconnected)?;
    Ok(())
}

async fn handle_response(
    message: Message,
    who: SocketAddr,
    manager: &Manager,
    auth: UserClaims,
) -> Result<(), ManagerError> {
    let response = process_message(message, who, manager.clone(), auth.clone()).await?;

    manager.send_message(auth, response).await
}

async fn get_auth(receiver: &mut SplitStream<WebSocket>) -> Result<UserClaims, ManagerError> {
    if let Some(Ok(message)) = receiver.next().await {
        match message {
            Message::Text(message) => {
                let message: ClientMessage = serde_json::from_str(&message)?;

                match message {
                    ClientMessage::Auth(token) => Ok(auth::get_claims_from_token(&token).await?),
                    ClientMessage::Game(_) => Err(ManagerError::UnexpectedValidMessage(
                        "Expected auth message",
                    )),
                }
            }

            _ => Err(ManagerError::InvalidWebsocketMessageType),
        }
    } else {
        Err(ManagerError::PlayerDisconnected)
    }
}

async fn process_message(
    msg: Message,
    who: SocketAddr,
    manager: Manager,
    auth: UserClaims,
) -> Result<ServerMessage, ManagerError> {
    match msg {
        Message::Text(message) => {
            tracing::debug!(">>>> {who} sent text message: {message:?}");

            let message: ClientMessage = serde_json::from_str(&message)?;

            let result = match message {
                ClientMessage::Game(g) => {
                    ServerMessage::Game(handle_game_message(g, manager, auth).await?)
                }
                ClientMessage::Auth(a) => {
                    tracing::error!("Unexpected auth message {a}");
                    return Err(ManagerError::UnexpectedValidMessage(
                        "Expected game message",
                    ));
                }
            };

            Ok(result)
        }
        Message::Close(c) => {
            let reason = c
                .map(|c| format!(" | reason: {} {}", c.code, c.reason))
                .unwrap_or_default();

            tracing::warn!(">>>> {who} sent close message{}", reason);

            Err(ManagerError::PlayerDisconnected)
        }
        _ => Err(ManagerError::InvalidWebsocketMessageType),
    }
}

async fn handle_game_message(
    message: ClientGameMessage,
    manager: Manager,
    auth: UserClaims,
) -> Result<ServerGameMessage, ManagerError> {
    let response = match message {
        ClientGameMessage::PlayTurn { card } => {
            let (turn, state) = manager.play_turn(card, auth).await?;
            ServerGameMessage::PlayerTurn { turn, state }
        }
        ClientGameMessage::PutBid { bid } => {
            let player_id = auth.id();
            manager.bid(bid, &player_id).await?;
            ServerGameMessage::PlayerBidded { player_id, bid }
        }
    };

    Ok(response)
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    Game(ClientGameMessage),
    Auth(String),
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientGameMessage {
    PlayTurn { card: Card },
    PutBid { bid: usize },
}

#[derive(serde::Serialize)]
pub struct GetLobbyDto {
    pub id: ObjectId,
    pub player_count: usize,
}

#[derive(serde::Serialize)]
pub struct JoinLobbyDto {
    pub id: ObjectId,
    pub players: Vec<UserClaims>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerGameMessage {
    PlayerTurn { turn: Turn, state: GameState },
    PlayerBidded { player_id: String, bid: usize },
}

#[derive(serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    Authorized(UserClaims),
    Game(ServerGameMessage),
    Error(String),
}
