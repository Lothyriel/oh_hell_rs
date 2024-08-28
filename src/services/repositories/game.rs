use chrono::{DateTime, Utc};
use mongodb::{
    bson::{doc, oid::ObjectId},
    error::Result,
    Collection, Database,
};

#[derive(Clone)]
pub struct GamesRepository {
    games: Collection<GameDto>,

    players: Collection<PlayerDto>,
    turns: Collection<TurnDto>,
}

impl GamesRepository {
    pub fn new(database: &Database) -> Self {
        Self {
            games: database.collection("Games"),
            players: database.collection("Players"),
            turns: database.collection("Turns"),
        }
    }

    pub async fn insert_game(&self, game: &GameDto) -> Result<()> {
        self.games.insert_one(game).await?;

        Ok(())
    }

    pub async fn insert_player(&self, player: &PlayerDto) -> Result<()> {
        self.players.insert_one(player).await?;

        Ok(())
    }

    pub async fn insert_turn(&self, turn: &TurnDto) -> Result<()> {
        self.turns.insert_one(turn).await?;

        Ok(())
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct GameDto {
    players: Vec<PlayerDto>,

    started_at: DateTime<Utc>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PlayerDto {
    nickname: String,
    ip: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]

pub struct TurnDto {
    game_id: ObjectId,
    player_id: ObjectId,
    time: DateTime<Utc>,
    data: (),
}
