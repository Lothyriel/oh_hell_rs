use crate::models::{Card, GameStage};

pub mod manager;
pub mod repositories;

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct GameInfoDto {
    pub info: Vec<PlayerInfoDto>,
    pub deck: Vec<Card>,
    pub upcard: Card,
    pub current_player: String,
    pub stage: GameStage,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct PlayerInfoDto {
    pub id: String,
    pub lifes: usize,
    pub rounds: usize,
    pub bid: Option<usize>,
}
