mod game;

use std::collections::HashMap;

pub use game::Game;
use mongodb::bson::oid::ObjectId;

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use rand::seq::SliceRandom;
use strum_macros::EnumIter;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Turn {
    pub player_id: ObjectId,
    pub card: Card,
}

impl Eq for Turn {}

impl PartialEq for Turn {
    fn eq(&self, other: &Self) -> bool {
        self.card == other.card
    }
}

impl PartialOrd for Turn {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.card.partial_cmp(&other.card)
    }
}

impl Ord for Turn {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.card.cmp(&other.card)
    }
}

struct GameManager {
    games: HashMap<ObjectId, Game>,
}

#[derive(Debug)]
pub struct Player {
    id: ObjectId,
    lifes: u8,
    deck: Vec<Card>,
    bid: Option<usize>,
    rounds: usize,
}

impl Player {
    pub fn new(id: ObjectId, deck: Vec<Card>) -> Self {
        Self {
            lifes: 5,
            deck,
            id,
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
