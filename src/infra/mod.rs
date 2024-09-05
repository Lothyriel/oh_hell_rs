pub mod auth;
pub mod game;
pub mod lobby;

use auth::UserClaims;
use axum::http::StatusCode;
use mongodb::bson::oid::ObjectId;

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
    Auth(String),
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientGameMessage {
    PlayTurn { card: Card },
    PutBid { bid: usize },
    Ready,
}

#[derive(serde::Serialize)]
pub struct GetLobbyDto {
    pub id: ObjectId,
    pub player_count: usize,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct JoinLobbyDto {
    pub id: ObjectId,
    pub players: Vec<PlayerStatus>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerGameMessage {
    PlayerTurn { player_id: String },
    TurnPlayed { turn: Turn },
    PlayerBidded { player_id: String, bid: usize },
    PlayerBiddingTurn { player_id: String },
    PlayerReady { player_id: String },
    RoundEnded,
    PlayerDeck(Vec<Card>),
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    Authorized(UserClaims),
    Game(ServerGameMessage),
}
