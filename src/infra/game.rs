use std::net::SocketAddr;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};

use crate::{
    infra::ClientMessage,
    services::manager::{Manager, ManagerError},
};

use super::{
    auth::{self, UserClaims},
    ClientGameMessage, ServerGameMessage, ServerMessage,
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

    ack_auth(auth.clone(), &mut sender).await?;

    manager.store_player_connection(auth.id(), sender).await?;

    tokio::spawn(async move {
        while let Some(Ok(message)) = receiver.next().await {
            let id = auth.id();
            match handle_response(message, who, &manager, &id).await {
                Ok(_) => {}
                Err(error) => {
                    tracing::error!("{error} | {who} closing connection");
                    manager.send_disconnect(&id, error).await;
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
    claims: UserClaims,
    sender: &mut SplitSink<WebSocket, Message>,
) -> Result<(), ManagerError> {
    let welcome = ServerMessage::Authorized(claims);

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
    player_id: &str,
) -> Result<(), ManagerError> {
    let response = process_message(message, who, manager.clone(), player_id.to_string()).await?;

    manager.send_message(player_id, response).await
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
    player_id: String,
) -> Result<ServerMessage, ManagerError> {
    match msg {
        Message::Text(message) => {
            tracing::debug!(">>>> {who} sent text message: {message:?}");

            let message = serde_json::from_str(&message)?;

            let result = match message {
                ClientMessage::Game(g) => {
                    ServerMessage::Game(handle_game_message(g, manager, player_id).await?)
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
    player_id: String,
) -> Result<ServerGameMessage, ManagerError> {
    let response = match message {
        ClientGameMessage::PlayTurn { card } => {
            let (turn, state) = manager.play_turn(card, player_id).await?;
            ServerGameMessage::PlayerTurn { turn, state }
        }
        ClientGameMessage::PutBid { bid } => {
            manager.bid(bid, &player_id).await?;
            ServerGameMessage::PlayerBidded { player_id, bid }
        }
    };

    Ok(response)
}
