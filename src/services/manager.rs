use std::{collections::HashMap, sync::Arc};

use axum::extract::ws::{Message, WebSocket};
use futures::{stream::SplitSink, SinkExt};
use mongodb::bson::oid::ObjectId;
use tokio::sync::Mutex;

use crate::{
    infra::{InfraError, ServerMessage},
    models::{BiddingError, Game, GameState, Turn, TurnError},
};

use super::GamesRepository;

#[derive(Clone)]
pub struct Manager {
    inner: Arc<InnerManager>,
    repo: GamesRepository,
}

impl Manager {
    pub fn new(repo: GamesRepository) -> Self {
        let inner = InnerManager {
            game: Mutex::new(GamesManager::new()),
            lobby: Mutex::new(LobbiesManager::new()),
            connections: Mutex::new(HashMap::new()),
        };

        Self {
            inner: Arc::new(inner),
            repo,
        }
    }

    pub async fn create_lobby(&self, player_id: ObjectId) -> ObjectId {
        let mut manager = self.inner.lobby.lock().await;

        manager.lobbies.insert(player_id, vec![player_id]);

        player_id
    }

    pub async fn join_lobby(
        &self,
        id: ObjectId,
        player_id: ObjectId,
    ) -> Result<Vec<ObjectId>, ManagerError> {
        let mut manager = self.inner.lobby.lock().await;

        let lobby = manager
            .lobbies
            .get_mut(&id)
            .ok_or(ManagerError::InvalidGame)?;

        lobby.push(player_id);

        Ok(lobby.clone())
    }

    pub async fn remove_lobby(&self, id: ObjectId) -> Option<()> {
        let mut manager = self.inner.lobby.lock().await;

        manager.lobbies.remove(&id).map(|_| ())
    }

    pub async fn play_turn(
        &self,
        game_id: ObjectId,
        turn: Turn,
    ) -> Result<GameState, ManagerError> {
        let mut manager = self.inner.game.lock().await;

        let game = manager
            .games
            .get_mut(&game_id)
            .ok_or(ManagerError::InvalidGame)?;

        let state = game.advance(turn)?;

        Ok(state)
    }

    pub async fn bid(
        &self,
        game_id: ObjectId,
        player_id: ObjectId,
        bid: usize,
    ) -> Result<(), ManagerError> {
        let mut manager = self.inner.game.lock().await;

        let game = manager
            .games
            .get_mut(&game_id)
            .ok_or(ManagerError::InvalidGame)?;

        game.bid(player_id, bid)?;

        Ok(())
    }

    pub async fn get_lobbies(&self) -> HashMap<ObjectId, Vec<ObjectId>> {
        let manager = self.inner.lobby.lock().await;

        manager.lobbies.clone()
    }

    pub async fn start_game(&self, game_id: ObjectId) -> Result<(), ManagerError> {
        let mut manager = self.inner.game.lock().await;

        let _game = manager
            .games
            .get_mut(&game_id)
            .ok_or(ManagerError::InvalidGame)?;

        // TODO game.start()

        Ok(())
    }

    pub async fn store_player_connection(
        &self,
        auth: String,
        sender: Connection,
    ) -> Result<(), ManagerError> {
        let mut manager = self.inner.connections.lock().await;

        manager.insert(auth, sender);

        Ok(())
    }

    pub async fn send_message(
        &self,
        auth: &str,
        message: ServerMessage,
    ) -> Result<(), ManagerError> {
        let mut manager = self.inner.connections.lock().await;

        let connection = manager.get_mut(auth).ok_or(ManagerError::Unauthorized)?;

        let message = serde_json::to_string(&message).expect("This serialization should not fail");

        match connection.send(Message::Text(message)).await {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!("Error sending message | {e}");
                Err(ManagerError::PlayerDisconnected)
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ManagerError {
    #[error("Player disconnected")]
    PlayerDisconnected,
    #[error("This game doesn't exists")]
    InvalidGame,
    #[error("Error processing turn: {0:?}")]
    Turn(#[from] TurnError),
    #[error("Error processing bid: {0:?}")]
    Bid(#[from] BiddingError),
    #[error("Invalid websocket message type")]
    InvalidWebsocketMessageType,
    #[error("Unexpected valid json message")]
    UnexpectedJsonMessage(#[from] serde_json::error::Error),
    #[error("Unexpected message")]
    UnexpectedValidMessage,
    #[error("Infra error {0}")]
    InfraError(#[from] InfraError),
    #[error("Unauthorized")]
    Unauthorized,
}

struct InnerManager {
    game: Mutex<GamesManager>,
    lobby: Mutex<LobbiesManager>,
    connections: Mutex<HashMap<String, Connection>>,
}

type Connection = SplitSink<WebSocket, Message>;

struct GamesManager {
    games: HashMap<ObjectId, Game>,
}

impl GamesManager {
    fn new() -> Self {
        Self {
            games: HashMap::new(),
        }
    }
}

struct LobbiesManager {
    lobbies: HashMap<ObjectId, Vec<ObjectId>>,
}

impl LobbiesManager {
    fn new() -> Self {
        Self {
            lobbies: HashMap::new(),
        }
    }
}
