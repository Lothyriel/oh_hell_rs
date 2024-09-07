mod game;

use std::collections::HashSet;

pub use game::Game;

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use rand::seq::SliceRandom;
use strum_macros::{Display, EnumIter};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Turn {
    pub player_id: String,
    pub card: Card,
}

impl PartialOrd for Turn {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Turn {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.card.cmp(&other.card)
    }
}

pub enum BiddingRound {
    Active(String),
    Ended(String),
}

#[derive(Debug)]
pub struct Player {
    lifes: u8,
    deck: Vec<Card>,
    bid: Option<usize>,
    rounds: usize,
}

impl Player {
    pub fn new(deck: Vec<Card>) -> Self {
        Self {
            lifes: 5,
            deck,
            bid: None,
            rounds: 0,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.lifes != 0
    }

    pub fn loose_life(&mut self) {
        self.lifes -= 1;
    }
}

#[derive(
    Debug, serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, PartialOrd, Eq, Ord,
)]
pub struct Card {
    rank: Rank,
    suit: Suit,
}

impl Card {
    pub fn new(rank: Rank, suit: Suit) -> Self {
        Self { rank, suit }
    }

    pub fn deck() -> Vec<Card> {
        Rank::iter()
            .flat_map(|rank| Suit::iter().map(move |suit| Card { suit, rank }))
            .collect()
    }

    pub fn shuffled_deck() -> Vec<Card> {
        let mut deck = Self::deck();

        deck.shuffle(&mut rand::thread_rng());

        deck
    }
}

#[derive(Debug, Serialize, Deserialize, EnumIter, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub enum Rank {
    Four,
    Five,
    Six,
    Seven,

    Ten,
    Eleven,
    Twelve,

    One,
    Two,
    Three,
}

#[derive(Debug, Serialize, Deserialize, EnumIter, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub enum Suit {
    Golds,
    Swords,
    Cups,
    Clubs,
}

#[derive(Debug)]
pub enum GameState {
    NotStarted(HashSet<String>),
    Running(Game),
    Ended { winner: String, game: Game },
}

pub enum GameEvent {
    RoundEnded,
    GameEnded(String),
    TurnPlayed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DealingMode {
    Increasing,
    Decreasing,
}

#[derive(thiserror::Error, Debug)]
pub enum GameError {
    #[error("Not enough players")]
    NotEnoughPlayers,
    #[error("Too many players")]
    TooManyPlayers,
    #[error("Invalid turn")]
    InvalidTurn(#[from] TurnError),
    #[error("Invalid bid")]
    InvalidBid(#[from] BiddingError),
}

#[derive(Debug, thiserror::Error, Display)]
pub enum TurnError {
    PlayersNotBidded,
    NotYourTurn,
    NotYourCard,
}

#[derive(Debug, thiserror::Error, Display)]
pub enum BiddingError {
    InvalidPlayer,
    AlreadyBidded,
}

#[cfg(test)]
mod tests {
    use crate::models::{Card, Rank, Suit};

    #[test]
    fn test_rank() {
        let a = Card::new(Rank::Six, Suit::Clubs);
        let b = Card::new(Rank::Seven, Suit::Golds);

        assert!(a < b);
    }

    #[test]
    fn test_rank_2() {
        let a = Card::new(Rank::Twelve, Suit::Clubs);
        let b = Card::new(Rank::Three, Suit::Golds);

        assert!(a < b);
    }

    #[test]
    fn test_suit() {
        let a = Card::new(Rank::Six, Suit::Clubs);
        let b = Card::new(Rank::Six, Suit::Golds);

        assert!(a > b);
    }
}
