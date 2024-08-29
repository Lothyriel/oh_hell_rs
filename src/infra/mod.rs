pub mod auth;
pub mod game;
pub mod lobby;

use auth::UserClaims;
use axum::http::StatusCode;
use mongodb::bson::oid::ObjectId;

use crate::models::{Card, GameState, Turn};

pub async fn fallback_handler() -> (StatusCode, &'static str) {
    NOT_FOUND_RESPONSE
}

const NOT_FOUND_RESPONSE: (StatusCode, &str) =
    (StatusCode::NOT_FOUND, "this resource doesn't exist");

#[derive(serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    Game(ClientGameMessage),
    Auth(String),
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientGameMessage {
    PlayTurn { card: Card },
    PutBid { bid: usize },
}

#[derive(serde::Serialize)]
pub struct GetLobbyDto {
    pub id: ObjectId,
    pub player_count: usize,
}

#[derive(serde::Serialize)]
pub struct JoinLobbyDto {
    pub id: ObjectId,
    pub players: Vec<UserClaims>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerGameMessage {
    PlayerTurn { turn: Turn, state: GameState },
    PlayerBidded { player_id: String, bid: usize },
}

#[derive(serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    Authorized(UserClaims),
    Game(ServerGameMessage),
}
