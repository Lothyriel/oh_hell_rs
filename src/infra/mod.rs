pub mod auth;
pub mod game;
pub mod lobby;

use std::collections::HashMap;

use auth::UserClaims;
use axum::http::StatusCode;

use crate::{
    models::{Card, Turn},
    services::manager::PlayerStatus,
};

pub async fn fallback_handler() -> (StatusCode, &'static str) {
    NOT_FOUND_RESPONSE
}

const NOT_FOUND_RESPONSE: (StatusCode, &str) =
    (StatusCode::NOT_FOUND, "this resource doesn't exist");

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    Game(ClientGameMessage),
    Auth { token: String },
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy)]
#[serde(tag = "type", content = "data")]
pub enum ClientGameMessage {
    PlayTurn { card: Card },
    PutBid { bid: usize },
    PlayerStatusChange { ready: bool },
}

#[derive(serde::Serialize)]
pub struct GetLobbyDto {
    pub id: String,
    pub player_count: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct JoinLobbyDto {
    pub id: String,
    pub players: Vec<PlayerStatus>,
}

type PlayerPoints = HashMap<String, usize>;

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    PlayerTurn { player_id: String },
    TurnPlayed { turn: Turn },
    PlayerBidded { player_id: String, bid: usize },
    PlayerBiddingTurn { player_id: String },
    PlayerStatusChange { player_id: String, ready: bool },
    RoundEnded(PlayerPoints),
    PlayerDeck(Vec<Card>),
    SetStart { trump: Card },
    SetEnded(PlayerPoints),
    GameEnded,
    PlayerJoined(UserClaims),
}
