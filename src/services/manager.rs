use std::{collections::HashMap, sync::Arc};

use axum::extract::ws::{Message, WebSocket};
use futures::{stream::SplitSink, SinkExt};
use mongodb::bson::oid::ObjectId;
use tokio::sync::Mutex;

use crate::{
    infra::{
        self,
        auth::UserClaims,
        game::{GetLobbyDto, ServerMessage},
    },
    models::{BiddingError, Card, Game, GameError, GameState, Turn, TurnError},
};

use super::repositories::{auth::AuthRepository, game::GamesRepository};

#[derive(Clone)]
pub struct Manager {
    inner: Arc<InnerManager>,
    pub games_repo: GamesRepository,
    pub auth_repo: AuthRepository,
}

impl Manager {
    pub fn new(games: GamesRepository, auth: AuthRepository) -> Self {
        let inner = InnerManager {
            lobby: Mutex::new(LobbiesManager::new()),
            connections: Mutex::new(HashMap::new()),
        };

        Self {
            inner: Arc::new(inner),
            games_repo: games,
            auth_repo: auth,
        }
    }

    pub async fn create_lobby(&self, user: UserClaims) -> ObjectId {
        let mut manager = self.inner.lobby.lock().await;

        let id = ObjectId::new();

        manager.lobbies.insert(id, Lobby::new(user));

        id
    }

    pub async fn join_lobby(
        &self,
        lobby_id: ObjectId,
        user_claims: UserClaims,
    ) -> Result<Vec<UserClaims>, LobbyError> {
        let mut manager = self.inner.lobby.lock().await;

        let players = {
            let lobby = manager
                .lobbies
                .get_mut(&lobby_id)
                .ok_or(LobbyError::InvalidLobby)?;

            lobby.players.insert(user_claims.id(), user_claims.clone());

            lobby.get_players()
        };

        manager.players_lobby.insert(user_claims.id(), lobby_id);

        Ok(players)
    }

    pub async fn play_turn(
        &self,
        card: Card,
        auth: UserClaims,
    ) -> Result<(Turn, GameState), LobbyError> {
        let mut manager = self.inner.lobby.lock().await;

        let player_id = auth.id();

        let game_id = {
            *manager
                .players_lobby
                .get(&player_id)
                .ok_or(LobbyError::WrongLobby)?
        };

        let lobby = manager
            .lobbies
            .get_mut(&game_id)
            .ok_or(LobbyError::InvalidLobby)?;

        if !lobby.players.contains_key(&player_id) {
            return Err(LobbyError::WrongLobby);
        }

        let game = lobby.get_game()?;

        let turn = Turn { player_id, card };

        let state = game
            .advance(turn.clone())
            .map_err(|e| LobbyError::GameError(GameError::InvalidTurn(e)))?;

        Ok((turn, state))
    }

    pub async fn bid(&self, bid: usize, player_id: &str) -> Result<(), LobbyError> {
        let mut manager = self.inner.lobby.lock().await;

        let lobby_id = {
            *manager
                .players_lobby
                .get(player_id)
                .ok_or(LobbyError::WrongLobby)?
        };

        let lobby = manager
            .lobbies
            .get_mut(&lobby_id)
            .ok_or(LobbyError::InvalidLobby)?;

        let game = lobby.get_game()?;

        game.bid(player_id, bid)
            .map_err(|e| LobbyError::GameError(GameError::InvalidBid(e)))?;

        Ok(())
    }

    pub async fn get_lobbies(&self) -> Vec<GetLobbyDto> {
        let manager = self.inner.lobby.lock().await;

        manager
            .lobbies
            .iter()
            .map(|(&id, lobby)| GetLobbyDto {
                id,
                player_count: lobby.players.len(),
            })
            .collect()
    }

    pub async fn start_game(&self, game_id: ObjectId) -> Result<(), LobbyError> {
        let mut manager = self.inner.lobby.lock().await;

        let lobby = manager
            .lobbies
            .get_mut(&game_id)
            .ok_or(LobbyError::InvalidLobby)?;

        if lobby.game.is_some() {
            return Err(LobbyError::GameAlreadyStarted);
        }

        let game = Game::new(lobby.get_players_id())?;

        lobby.game = Some(game);

        Ok(())
    }

    pub async fn store_player_connection(
        &self,
        player_id: String,
        sender: Connection,
    ) -> Result<(), ManagerError> {
        let mut manager = self.inner.connections.lock().await;

        manager.insert(player_id, sender);

        Ok(())
    }

    pub async fn send_message(
        &self,
        auth: UserClaims,
        message: ServerMessage,
    ) -> Result<(), ManagerError> {
        let mut manager = self.inner.connections.lock().await;

        let connection = manager
            .get_mut(&auth.id())
            .ok_or(ManagerError::PlayerDisconnected)?;

        let message = serde_json::to_string(&message)?;

        connection
            .send(Message::Text(message))
            .await
            .map_err(|_| ManagerError::PlayerDisconnected)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ManagerError {
    #[error("Player disconnected")]
    PlayerDisconnected,
    #[error("Error processing turn: {0:?}")]
    Turn(#[from] TurnError),
    #[error("Error processing bid: {0:?}")]
    Bid(#[from] BiddingError),
    #[error("Invalid websocket message type")]
    InvalidWebsocketMessageType,
    #[error("Unexpected valid json message: {0}")]
    UnexpectedJsonMessage(#[from] serde_json::error::Error),
    #[error("Unexpected message | {0}")]
    UnexpectedValidMessage(&'static str),
    #[error("Database error: {0}")]
    Database(#[from] mongodb::error::Error),
    #[error("Unauthorized | {0}")]
    Unauthorized(#[from] infra::auth::AuthError),
    #[error("Lobby error | {0}")]
    Lobby(#[from] LobbyError),
}

#[derive(thiserror::Error, Debug)]
pub enum LobbyError {
    #[error("Invalid lobby id")]
    InvalidLobby,
    #[error("This lobby is already playing")]
    GameAlreadyStarted,
    #[error("Game didn't started yet")]
    GameNotStarted,
    #[error("This is not your lobby")]
    WrongLobby,
    #[error("Game error | {0}")]
    GameError(#[from] GameError),
}

struct InnerManager {
    lobby: Mutex<LobbiesManager>,
    connections: Mutex<HashMap<String, Connection>>,
}

type Connection = SplitSink<WebSocket, Message>;

struct LobbiesManager {
    lobbies: HashMap<ObjectId, Lobby>,
    // TODO make sure to remove entries of this guy wherever is needed
    players_lobby: HashMap<String, ObjectId>,
}

struct Lobby {
    players: HashMap<String, UserClaims>,
    game: Option<Game>,
}

impl Lobby {
    fn new(owner: UserClaims) -> Self {
        Self {
            players: vec![(owner.id(), owner)].into_iter().collect(),
            game: None,
        }
    }

    fn get_players_id(&self) -> Vec<String> {
        self.players.keys().cloned().collect()
    }

    fn get_players(&self) -> Vec<UserClaims> {
        self.players.values().cloned().collect()
    }

    fn get_game(&mut self) -> Result<&mut Game, LobbyError> {
        self.game.as_mut().ok_or(LobbyError::GameNotStarted)
    }
}

impl LobbiesManager {
    fn new() -> Self {
        Self {
            lobbies: HashMap::new(),
            players_lobby: HashMap::new(),
        }
    }
}
