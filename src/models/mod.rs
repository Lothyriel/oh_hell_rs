mod game;
pub mod iter;

use std::collections::{HashMap, HashSet};

pub use game::Game;

use indexmap::IndexMap;
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

#[derive(Debug)]
pub struct Player {
    lifes: usize,
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
pub enum LobbyState {
    NotStarted(HashSet<String>),
    Playing(Game),
}

pub enum GameEvent {
    SetEnded {
        lifes: HashMap<String, usize>,
        trump: Card,
        decks: IndexMap<String, Vec<Card>>,
        first: String,
        possible: Vec<usize>,
    },
    RoundEnded(HashMap<String, usize>),
    Ended {
        winner: String,
        lifes: HashMap<String, usize>,
    },
}

pub struct DealState {
    pub info: RoundInfo,
    pub event: Option<GameEvent>,
    pub pile: Vec<Turn>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RoundInfo {
    pub next: String,
    pub state: RoundState,
    pub possible_bids: Vec<usize>,
}

impl RoundInfo {
    fn new(next: String, state: RoundState, possible_bids: Vec<usize>) -> Self {
        Self {
            next,
            state,
            possible_bids,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RoundState {
    Active,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    #[error("Invalid turn | {0}")]
    InvalidTurn(#[from] TurnError),
    #[error("Invalid bid | {0}")]
    InvalidBid(#[from] BiddingError),
}

#[derive(Debug, thiserror::Error, Display)]
pub enum TurnError {
    BiddingStageActive,
    NotYourTurn,
    NotYourCard,
    InvalidPlayer,
}

#[derive(Debug, thiserror::Error, Display, PartialEq, Eq)]
pub enum BiddingError {
    InvalidPlayer,
    AlreadyBidded,
    DealingStageActive,
    NotYourTurn,
    BidOutOfRange,
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
