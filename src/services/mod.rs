use crate::{infra::PlayerPoints, models::Card};

pub mod manager;
pub mod repositories;

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct GameInfoDto {
    pub points: PlayerPoints,
    pub lifes: PlayerPoints,
    pub deck: Vec<Card>,
    pub current_player: String,
}
