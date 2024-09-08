use std::net::SocketAddr;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{stream::SplitStream, StreamExt};

use crate::{
    infra::ClientMessage,
    services::manager::{Manager, ManagerError},
};

use super::{
    auth::{self, UserClaims},
    ClientGameMessage,
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
    let (sender, mut receiver) = socket.split();

    let auth = get_auth(&mut receiver).await?;

    manager.store_player_connection(auth.id(), sender).await?;

    tokio::spawn(async move {
        while let Some(Ok(message)) = receiver.next().await {
            let id = auth.id();
            match process_msg(message, who, manager.clone(), id.clone()).await {
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

async fn get_auth(receiver: &mut SplitStream<WebSocket>) -> Result<UserClaims, ManagerError> {
    if let Some(Ok(message)) = receiver.next().await {
        match message {
            Message::Text(message) => {
                let message: ClientMessage = serde_json::from_str(&message)?;

                match message {
                    ClientMessage::Auth { token } => Ok(auth::get_claims_from_token(&token).await?),
                    ClientMessage::Game(_) => Err(ManagerError::UnexpectedValidMessage(
                        "Expected auth message",
                    )),
                }
            }

            _ => Err(ManagerError::InvalidWebsocketMessageType),
        }
    } else {
        Err(ManagerError::PlayerDisconnected(
            "PlayerDisconnected during auth handshake".to_string(),
        ))
    }
}

async fn process_msg(
    msg: Message,
    who: SocketAddr,
    manager: Manager,
    player_id: String,
) -> Result<(), ManagerError> {
    match msg {
        Message::Text(message) => {
            tracing::debug!(">>>> {who} sent text message: {message:?}");

            let message = serde_json::from_str(&message)?;

            match message {
                ClientMessage::Game(g) => handle_game_msg(g, manager, player_id).await,
                ClientMessage::Auth { token: a } => {
                    tracing::error!("Unexpected auth message {a}");
                    Err(ManagerError::UnexpectedValidMessage(
                        "Expected game message",
                    ))
                }
            }
        }
        Message::Close(c) => {
            let reason = c
                .map(|c| format!(" | reason: {} {}", c.code, c.reason))
                .unwrap_or_default();

            tracing::warn!(">>>> {who} sent close message{}", reason);

            Err(ManagerError::PlayerDisconnected(reason))
        }
        _ => Err(ManagerError::InvalidWebsocketMessageType),
    }
}

async fn handle_game_msg(
    message: ClientGameMessage,
    manager: Manager,
    player_id: String,
) -> Result<(), ManagerError> {
    let result = match message {
        ClientGameMessage::PlayTurn { card } => manager.play_turn(card, player_id).await,
        ClientGameMessage::PutBid { bid } => manager.bid(bid, player_id).await,
        ClientGameMessage::PlayerStatusChange { ready } => {
            manager.player_ready(player_id, ready).await
        }
    };

    // TODO all these messages should be broadcasted cause every client needs to know them
    // maybe take a look at the `old` setup of sending the message here
    // and then send only specifics messages inside the manager (but is prob not worth the hassle)

    Ok(result?)
}
