use std::net::SocketAddr;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::IntoResponse,
};

use futures::{stream::SplitStream, SinkExt, StreamExt};
use mongodb::bson::oid::ObjectId;

use crate::{
    models::{GameState, Turn},
    services::manager::{Manager, ManagerError},
};

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

    let welcome = ServerMessage::Authorized(auth.clone());

    let welcome = serde_json::to_string(&welcome)?;

    sender
        .send(Message::Text(welcome))
        .await
        .map_err(|_| ManagerError::PlayerDisconnected)?;

    manager
        .store_player_connection(auth.clone(), sender)
        .await?;

    tokio::spawn(async move {
        while let Some(Ok(message)) = receiver.next().await {
            match handle_response(message, who, &manager, &auth).await {
                Ok(_) => {}
                Err(error) => {
                    tracing::error!("{error} | {who} closing connection");
                    let msg = ServerMessage::Error(error.to_string());
                    if let Err(error) = manager.send_message(&auth, msg).await {
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

async fn handle_response(
    message: Message,
    who: SocketAddr,
    manager: &Manager,
    auth: &str,
) -> Result<(), ManagerError> {
    let response = process_message(message, who, manager.clone()).await?;

    manager.send_message(auth, response).await
}

async fn get_auth(receiver: &mut SplitStream<WebSocket>) -> Result<String, ManagerError> {
    if let Some(Ok(message)) = receiver.next().await {
        match message {
            Message::Text(message) => {
                let message: ClientMessage = serde_json::from_str(&message)?;

                match message {
                    ClientMessage::Auth(auth_data) => Ok(auth_data),
                    ClientMessage::Lobby(_) | ClientMessage::Game(_) => {
                        Err(ManagerError::UnexpectedValidMessage)
                    }
                }
            }
            _ => Err(ManagerError::InvalidWebsocketMessageType),
        }
    } else {
        Err(ManagerError::Unauthorized)
    }
}

async fn process_message(
    msg: Message,
    who: SocketAddr,
    manager: Manager,
) -> Result<ServerMessage, ManagerError> {
    match msg {
        Message::Text(message) => {
            tracing::debug!(">>>> {who} sent text message: {message:?}");

            let message: ClientMessage = serde_json::from_str(&message)?;

            let result = match message {
                ClientMessage::Lobby(l) => {
                    ServerMessage::Lobby(handle_lobby_message(l, manager).await?)
                }
                ClientMessage::Game(g) => {
                    ServerMessage::Game(handle_game_message(g, manager).await?)
                }
                ClientMessage::Auth(a) => {
                    tracing::error!("Unexpected auth message {a}");
                    return Err(ManagerError::UnexpectedValidMessage);
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
) -> Result<ServerGameMessage, ManagerError> {
    let response = match message {
        ClientGameMessage::PlayTurn { game_id, turn } => {
            let state = manager.play_turn(game_id, turn).await?;
            ServerGameMessage::PlayerTurn { turn, state }
        }
        ClientGameMessage::PutBid {
            game_id,
            player_id,
            bid,
        } => {
            manager.bid(game_id, player_id, bid).await?;
            ServerGameMessage::PlayerBidded { player_id, bid }
        }
    };

    Ok(response)
}

async fn handle_lobby_message(
    message: ClientLobbyMessage,
    manager: Manager,
) -> Result<ServerLobbyMessage, ManagerError> {
    let response = match message {
        ClientLobbyMessage::RequestLobbies => {
            let lobbies = manager
                .get_lobbies()
                .await
                .into_iter()
                .map(|(id, players)| Lobby { id, players })
                .collect();

            ServerLobbyMessage::AvailableLobbies(lobbies)
        }
        ClientLobbyMessage::CreateLobby { player_id } => {
            manager.create_lobby(player_id).await;
            ServerLobbyMessage::LobbyCreated { game_id: player_id }
        }
        ClientLobbyMessage::JoinLobby {
            lobby_id,
            player_id,
        } => {
            let players = manager.join_lobby(lobby_id, player_id).await?;
            ServerLobbyMessage::LobbyJoined {
                game_id: lobby_id,
                players,
            }
        }
        ClientLobbyMessage::StartGame { game_id } => {
            manager.start_game(game_id).await?;
            ServerLobbyMessage::GameStarted { game_id }
        }
    };

    Ok(response)
}

pub async fn fallback_handler() -> (StatusCode, &'static str) {
    NOT_FOUND_RESPONSE
}

const NOT_FOUND_RESPONSE: (StatusCode, &str) =
    (StatusCode::NOT_FOUND, "this resource doesn't exist");

#[derive(serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientLobbyMessage {
    RequestLobbies,
    CreateLobby {
        player_id: ObjectId,
    },
    JoinLobby {
        player_id: ObjectId,
        lobby_id: ObjectId,
    },
    StartGame {
        game_id: ObjectId,
    },
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientGameMessage {
    PlayTurn {
        game_id: ObjectId,
        turn: Turn,
    },
    PutBid {
        game_id: ObjectId,
        player_id: ObjectId,
        bid: usize,
    },
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    Lobby(ClientLobbyMessage),
    Game(ClientGameMessage),
    Auth(String),
}

#[derive(serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerLobbyMessage {
    AvailableLobbies(Vec<Lobby>),
    GameStarted {
        game_id: ObjectId,
    },
    LobbyCreated {
        game_id: ObjectId,
    },
    LobbyJoined {
        game_id: ObjectId,
        players: Vec<ObjectId>,
    },
    PlayerJoined {
        player_id: ObjectId,
    },
}

#[derive(serde::Serialize)]
struct Lobby {
    id: ObjectId,
    players: Vec<ObjectId>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerGameMessage {
    PlayerTurn { turn: Turn, state: GameState },
    PlayerBidded { player_id: ObjectId, bid: usize },
}

#[derive(serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    Lobby(ServerLobbyMessage),
    Game(ServerGameMessage),
    Authorized(String),
    Error(String),
}
