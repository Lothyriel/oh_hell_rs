use std::{collections::HashMap, sync::Arc};

use mongodb::bson::oid::ObjectId;
use tokio::sync::Mutex;

use crate::models::{BiddingError, Game, GameState, Turn, TurnError};

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
        };

        Self {
            inner: Arc::new(inner),
            repo,
        }
    }

    pub async fn create_lobby(&self, id: ObjectId, player_id: ObjectId) -> ObjectId {
        let mut manager = self.inner.lobby.lock().await;

        let id = ObjectId::new();

        manager.lobbies.insert(id, vec![player_id]);

        id
    }

    pub async fn join_lobby(&self, id: ObjectId, player_id: ObjectId) -> Option<()> {
        let mut manager = self.inner.lobby.lock().await;

        let lobby = manager.lobbies.get_mut(&id)?;

        lobby.push(player_id);

        Some(())
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
}
#[derive(thiserror::Error, Debug)]
pub enum ManagerError {
    #[error("This game doesn't exists")]
    InvalidGame,
    #[error("Error processing turn: {0:?}")]
    Turn(#[from] TurnError),
    #[error("Error processing bid: {0:?}")]
    Bid(#[from] BiddingError),
}

struct InnerManager {
    game: Mutex<GamesManager>,
    lobby: Mutex<LobbiesManager>,
}

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
