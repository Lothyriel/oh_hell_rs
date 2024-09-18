use std::{
    borrow::{BorrowMut, Cow},
    collections::{HashMap, HashSet},
    sync::Arc,
};

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures::{stream::SplitSink, SinkExt};
use indexmap::IndexMap;
use tokio::sync::Mutex;

use crate::{
    infra::{self, auth::UserClaims, GetLobbyDto, ServerMessage},
    models::{
        BiddingError, Card, Game, GameError, GameEvent, LobbyState, RoundState, Turn, TurnError,
    },
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

    pub async fn create_lobby(&self, user_id: String) -> String {
        let mut manager = self.inner.lobby.lock().await;

        manager.lobbies.insert(user_id.clone(), Lobby::new());

        user_id
    }

    pub async fn join_lobby(
        &self,
        lobby_id: String,
        user_claims: UserClaims,
    ) -> Result<Vec<PlayerStatus>, LobbyError> {
        let (players_status, players) = {
            let mut manager = self.inner.lobby.lock().await;

            let (players_status, players) = {
                let lobby = manager
                    .lobbies
                    .get_mut(&lobby_id)
                    .ok_or(LobbyError::InvalidLobby)?;

                match lobby.state {
                    LobbyState::NotStarted(_) => {}
                    LobbyState::Playing(_) => return Err(LobbyError::GameAlreadyStarted),
                }

                let status = PlayerStatus::new(user_claims.clone());

                lobby.players.insert(user_claims.id(), status);

                (lobby.get_players(), lobby.get_players_id())
            };

            manager.players_lobby.insert(user_claims.id(), lobby_id);

            (players_status, players)
        };

        let msg = ServerMessage::PlayerJoined(user_claims);
        self.broadcast_msg(&players, &msg).await;

        Ok(players_status)
    }

    pub async fn play_turn(&self, card: Card, player_id: String) -> Result<(), LobbyError> {
        let (players, state) = {
            let mut manager = self.inner.lobby.lock().await;

            let game_id = {
                manager
                    .players_lobby
                    .get(&player_id)
                    .ok_or(LobbyError::WrongLobby)
                    .cloned()?
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
                .deal(turn)
                .map_err(|e| LobbyError::GameError(GameError::InvalidTurn(e)))?;

            (lobby.get_players_id(), state)
        };

        let msg = ServerMessage::TurnPlayed { pile: state.pile };
        self.broadcast_msg(&players, &msg).await;

        match state.event {
            Some(GameEvent::SetEnded {
                lifes,
                upcard,
                decks,
                first,
                possible,
            }) => {
                let msg = ServerMessage::SetEnded(lifes);
                self.broadcast_msg(&players, &msg).await;

                self.init_set(decks, first, upcard, possible).await;
            }
            Some(GameEvent::RoundEnded(points)) => {
                let msg = ServerMessage::RoundEnded(points);
                self.broadcast_msg(&players, &msg).await;

                let msg = ServerMessage::PlayerTurn {
                    player_id: state.info.next,
                };

                self.broadcast_msg(&players, &msg).await;
            }
            Some(GameEvent::Ended { winner, lifes }) => {
                let msg = ServerMessage::GameEnded { winner, lifes };
                self.broadcast_msg(&players, &msg).await;
            }
            None => {
                let msg = ServerMessage::PlayerTurn {
                    player_id: state.info.next,
                };

                self.broadcast_msg(&players, &msg).await;
            }
        }

        Ok(())
    }

    pub async fn bid(&self, bid: usize, player_id: String) -> Result<(), LobbyError> {
        let (players, (info, possible_bids)) = {
            let mut manager = self.inner.lobby.lock().await;

            let lobby_id = {
                manager
                    .players_lobby
                    .get(&player_id)
                    .ok_or(LobbyError::WrongLobby)
                    .cloned()?
            };

            let lobby = manager
                .lobbies
                .get_mut(&lobby_id)
                .ok_or(LobbyError::InvalidLobby)?;

            let game = lobby.get_game()?;

            let info = game
                .bid(&player_id, bid)
                .map_err(|e| LobbyError::GameError(GameError::InvalidBid(e)))?;

            (lobby.get_players_id(), info)
        };

        let msg = ServerMessage::PlayerBidded { player_id, bid };
        self.broadcast_msg(&players, &msg).await;

        let msg = match info.state {
            RoundState::Active => ServerMessage::PlayerBiddingTurn {
                player_id: info.next,
                possible_bids,
            },
            RoundState::Ended => ServerMessage::PlayerTurn {
                player_id: info.next,
            },
        };

        self.broadcast_msg(&players, &msg).await;

        Ok(())
    }

    pub async fn get_lobbies(&self) -> Vec<GetLobbyDto> {
        let manager = self.inner.lobby.lock().await;

        manager
            .lobbies
            .iter()
            .filter(|(_, lobby)| matches!(lobby.state, LobbyState::NotStarted(_)))
            .map(|(id, lobby)| GetLobbyDto {
                id: id.clone(),
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

    pub async fn unicast_msg(&self, player_id: &str, message: &ServerMessage) {
        let mut manager = self.inner.connections.lock().await;

        if let Some(connection) = manager.get_mut(player_id) {
            send_msg(message, player_id, connection).await
        }
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
            ManagerError::PlayerDisconnected(_) => 1001,
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
            tracing::error!("Failed to send close message: {e}")
        }
    }

    pub async fn player_status_change(
        &self,
        player_id: String,
        ready: bool,
    ) -> Result<(), LobbyError> {
        let (players, set_info) = {
            let mut manager = self.inner.lobby.lock().await;

            let lobby_id = {
                manager
                    .players_lobby
                    .get(&player_id)
                    .ok_or(LobbyError::WrongLobby)
                    .cloned()?
            };

            let lobby = manager
                .lobbies
                .get_mut(&lobby_id)
                .ok_or(LobbyError::InvalidLobby)?;

            let players_ready = match lobby.state.borrow_mut() {
                LobbyState::NotStarted(p) => p,
                LobbyState::Playing(_) => return Err(LobbyError::GameAlreadyStarted),
            };

            if ready {
                players_ready.insert(player_id.clone())
            } else {
                players_ready.remove(&player_id)
            };

            let should_start = players_ready.len() == lobby.players.len();

            let set_info = if should_start {
                let game = Game::new_default(lobby.get_players_id())?;

                let (decks, upcard) = game.clone_decks();

                let first = game.current_bidding_player();

                let possible = game.get_possible_bids();

                lobby.state = LobbyState::Playing(game);

                Some((decks, first, upcard, possible))
            } else {
                None
            };

            (lobby.get_players_id(), set_info)
        };

        let msg = ServerMessage::PlayerStatusChange { player_id, ready };
        self.broadcast_msg(&players, &msg).await;

        if let Some((decks, first, upcard, possible_bids)) = set_info {
            self.init_set(decks, first, upcard, possible_bids).await;
        }

        Ok(())
    }

    async fn init_set(
        &self,
        decks: IndexMap<String, Vec<Card>>,
        first: String,
        upcard: Card,
        possible_bids: Vec<usize>,
    ) {
        let players: Vec<_> = decks.keys().cloned().collect();

        let msg = ServerMessage::SetStart { upcard };
        self.broadcast_msg(&players, &msg).await;

        for (p, deck) in decks {
            let msg = ServerMessage::PlayerDeck(deck);

            self.unicast_msg(&p, &msg).await;
        }

        let msg = ServerMessage::PlayerBiddingTurn {
            player_id: first,
            possible_bids,
        };

        self.broadcast_msg(&players, &msg).await;
    }

    async fn broadcast_msg(&self, players: &[String], msg: &ServerMessage) {
        for p in players {
            let mut connections = self.inner.connections.lock().await;

            if let Some(c) = connections.get_mut(p) {
                send_msg(msg, p, c).await;
            }
        }
    }
}

async fn send_msg(msg: &ServerMessage, player: &str, connection: &mut Connection) {
    let msg = serde_json::to_string(msg).expect("Should be valid json");

    tracing::info!("Sending to {player}: {msg}");

    let send = connection
        .send(Message::Text(msg))
        .await
        .map_err(|e| ManagerError::PlayerDisconnected(e.to_string()));

    if let Err(e) = send {
        tracing::error!("Error sending msg to: {player} | {e}");
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ManagerError {
    #[error("Player disconnected | {0}")]
    PlayerDisconnected(String),
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
    lobbies: HashMap<String, Lobby>,
    // TODO make sure to remove entries of this guy wherever is needed
    players_lobby: HashMap<LobbyId, PlayerId>,
}

type LobbyId = String;
type PlayerId = String;

struct Lobby {
    players: IndexMap<String, PlayerStatus>,
    state: LobbyState,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
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
            players: IndexMap::new(),
            state: LobbyState::NotStarted(HashSet::new()),
        }
    }

    fn get_players_id(&self) -> Vec<String> {
        self.players.keys().cloned().collect()
    }

    fn get_players(&self) -> Vec<PlayerStatus> {
        self.players.values().cloned().collect()
    }

    fn get_game(&mut self) -> Result<&mut Game, LobbyError> {
        match self.state.borrow_mut() {
            LobbyState::NotStarted(_) => Err(LobbyError::GameNotStarted),
            LobbyState::Playing(g) => Ok(g),
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
