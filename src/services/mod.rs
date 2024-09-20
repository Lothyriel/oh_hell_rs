use crate::models::Card;

pub mod manager;
pub mod repositories;

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct GameInfoDto {
    pub info: Vec<PlayerInfoDto>,
    pub deck: Vec<Card>,
    pub current_player: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct PlayerInfoDto {
    pub id: String,
    pub lifes: usize,
    pub rounds: usize,
    pub bid: usize,
}
