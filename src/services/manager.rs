use std::{
    borrow::{BorrowMut, Cow},
    collections::{HashMap, HashSet},
    sync::Arc,
};

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures::{stream::SplitSink, SinkExt};
use mongodb::bson::oid::ObjectId;
use tokio::sync::Mutex;

use crate::{
    infra::{self, auth::UserClaims, GetLobbyDto, ServerMessage},
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

    pub async fn create_lobby(&self, _user: UserClaims) -> ObjectId {
        let mut manager = self.inner.lobby.lock().await;

        // TODO create a way that a user can't spam create lobbies

        let lobby_id = ObjectId::new();

        manager.lobbies.insert(lobby_id, Lobby::new());

        lobby_id
    }

    pub async fn join_lobby(
        &self,
        lobby_id: ObjectId,
        user_claims: UserClaims,
    ) -> Result<Vec<PlayerStatus>, LobbyError> {
        let mut manager = self.inner.lobby.lock().await;

        let players = {
            let lobby = manager
                .lobbies
                .get_mut(&lobby_id)
                .ok_or(LobbyError::InvalidLobby)?;

            let status = PlayerStatus::new(user_claims.clone());

            lobby.players.insert(user_claims.id(), status);

            lobby.get_players()
        };

        manager.players_lobby.insert(user_claims.id(), lobby_id);

        Ok(players)
    }

    pub async fn play_turn(&self, card: Card, player_id: String) -> Result<(), LobbyError> {
        let (players, turn) = {
            let mut manager = self.inner.lobby.lock().await;

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

            // TODO figure this 'state return' situation out
            let _state = game
                .advance(turn.clone())
                .map_err(|e| LobbyError::GameError(GameError::InvalidTurn(e)))?;

            (lobby.get_players_id(), turn)
        };

        let msg = ServerMessage::TurnPlayed { turn };
        self.broadcast_msg(&players, &msg).await;

        Ok(())
    }

    pub async fn bid(&self, bid: usize, player_id: String) -> Result<(), LobbyError> {
        let players = {
            let mut manager = self.inner.lobby.lock().await;

            let lobby_id = {
                *manager
                    .players_lobby
                    .get(&player_id)
                    .ok_or(LobbyError::WrongLobby)?
            };

            let lobby = manager
                .lobbies
                .get_mut(&lobby_id)
                .ok_or(LobbyError::InvalidLobby)?;

            let game = lobby.get_game()?;

            game.bid(&player_id, bid)
                .map_err(|e| LobbyError::GameError(GameError::InvalidBid(e)))?;

            lobby.get_players_id()
        };

        let msg = ServerMessage::PlayerBidded { player_id, bid };

        self.broadcast_msg(&players, &msg).await;

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

    pub async fn store_player_connection(
        &self,
        player_id: String,
        sender: Connection,
    ) -> Result<(), ManagerError> {
        let mut manager = self.inner.connections.lock().await;

        manager.insert(player_id, sender);

        Ok(())
    }

    pub async fn unicast_msg(
        &self,
        player_id: &str,
        message: &ServerMessage,
    ) -> Result<(), ManagerError> {
        let mut manager = self.inner.connections.lock().await;

        let connection = manager
            .get_mut(player_id)
            .ok_or(ManagerError::PlayerDisconnected)?;

        send_msg(message, connection).await
    }

    pub async fn send_disconnect(&self, player_id: &str, reason: ManagerError) {
        let mut manager = self.inner.connections.lock().await;

        let connection = match manager.get_mut(player_id) {
            Some(c) => c,
            None => {
                tracing::error!("{player_id} disconnected");
                return;
            }
        };

        let code = match reason {
            ManagerError::PlayerDisconnected => 1001,
            ManagerError::InvalidWebsocketMessageType => 1003,
            ManagerError::Lobby(_) => 1008,
            ManagerError::Turn(_) | ManagerError::Bid(_) => 1008,
            ManagerError::UnexpectedJsonMessage(_) => 1008,
            ManagerError::UnexpectedValidMessage(_) => 1008,
            ManagerError::Database(_) => 1011,
            ManagerError::Unauthorized(_) => 3000,
        };

        let send_close = connection
            .send(Message::Close(Some(CloseFrame {
                code,
                reason: Cow::Owned(reason.to_string()),
            })))
            .await;

        if let Err(e) = send_close {
            tracing::error!("{e} | while trying to send error message")
        }
    }

    pub async fn player_ready(&self, player_id: String, ready: bool) -> Result<(), LobbyError> {
        let (players, start_info) = {
            let mut manager = self.inner.lobby.lock().await;

            let lobby_id = {
                *manager
                    .players_lobby
                    .get(&player_id)
                    .ok_or(LobbyError::WrongLobby)?
            };

            let lobby = manager
                .lobbies
                .get_mut(&lobby_id)
                .ok_or(LobbyError::InvalidLobby)?;

            let players_ready = match lobby.game.borrow_mut() {
                GameState::NotStarted(p) => p,
                GameState::Running(_) => return Err(LobbyError::GameAlreadyStarted),
                GameState::Ended { winner: _, game: _ } => return Err(LobbyError::GameNotStarted),
            };

            players_ready.insert(player_id.clone());

            let players: Vec<_> = players_ready.iter().cloned().collect();

            let should_start = players_ready.len() == lobby.players.len();

            let start_info = if should_start {
                let game = Game::new(lobby.get_players_id())?;

                let decks = game.clone_decks();

                let first = game.players[0].clone();

                lobby.game = GameState::Running(game);

                Some((decks, first))
            } else {
                None
            };

            (players, start_info)
        };

        let msg = ServerMessage::PlayerStatusChange { player_id, ready };
        self.broadcast_msg(&players, &msg).await;

        if let Some((decks, first)) = start_info {
            self.start_game(decks, first).await;
        }

        Ok(())
    }

    async fn start_game(&self, decks: HashMap<String, Vec<Card>>, first: String) {
        let players: Vec<_> = decks.keys().cloned().collect();

        for (p, deck) in decks {
            let msg = ServerMessage::PlayerDeck(deck);

            if let Err(e) = self.unicast_msg(&p, &msg).await {
                tracing::error!("Error while unicasting: {p} | {e}");
            }
        }

        let msg = ServerMessage::PlayerTurn { player_id: first };
        self.broadcast_msg(&players, &msg).await;
    }

    async fn broadcast_msg(&self, players: &[String], msg: &ServerMessage) {
        for p in players {
            let mut connections = self.inner.connections.lock().await;

            if let Some(c) = connections.get_mut(p) {
                if let Err(e) = send_msg(msg, c).await {
                    tracing::error!("Error broadcasting to: {p} | {e}");
                }
            }
        }
    }
}

async fn send_msg(
    message: &ServerMessage,
    connection: &mut Connection,
) -> Result<(), ManagerError> {
    let message = serde_json::to_string(message)?;

    connection
        .send(Message::Text(message))
        .await
        .map_err(|_| ManagerError::PlayerDisconnected)
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
    players: HashMap<String, PlayerStatus>,
    game: GameState,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct PlayerStatus {
    pub ready: bool,
    pub player: UserClaims,
}
impl PlayerStatus {
    fn new(claims: UserClaims) -> Self {
        Self {
            ready: false,
            player: claims,
        }
    }
}

impl Lobby {
    fn new() -> Self {
        Self {
            players: HashMap::new(),
            game: GameState::NotStarted(HashSet::new()),
        }
    }

    fn get_players_id(&self) -> Vec<String> {
        self.players.keys().cloned().collect()
    }

    fn get_players(&self) -> Vec<PlayerStatus> {
        self.players.values().cloned().collect()
    }

    fn get_game(&mut self) -> Result<&mut Game, LobbyError> {
        match self.game.borrow_mut() {
            GameState::NotStarted(_) => Err(LobbyError::GameNotStarted),
            GameState::Running(g) => Ok(g),
            GameState::Ended { winner: _, game } => Ok(game),
        }
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
