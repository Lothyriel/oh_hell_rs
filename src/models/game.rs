use std::collections::{BinaryHeap, HashMap};

use super::{Card, Player, Turn};
use mongodb::bson::oid::ObjectId;
use strum_macros::Display;

#[derive(Debug)]
pub struct Game {
    decks: HashMap<ObjectId, Player>,
    players: Vec<ObjectId>,
    hand_index: usize,
    current_player_index: usize,
    turn_cards: BinaryHeap<Turn>,
    cards_mode: CardsMode,
    current_cards_count: usize,
}

const MAX_AVAILABLE_CARDS: usize = 40 - 1;

impl Game {
    pub fn new(players: Vec<ObjectId>) -> Result<Self, GameError> {
        const INITIAL_HAND_INDEX: usize = 0;

        if players.len() < 2 {
            return Err(GameError::NotEnoughPlayers);
        }

        if players.len() > 30 {
            return Err(GameError::TooManyPlayers);
        }

        let decks = Self::get_decks(&players, INITIAL_HAND_INDEX + 1);

        Ok(Self {
            decks,
            players,
            turn_cards: BinaryHeap::new(),
            current_player_index: INITIAL_HAND_INDEX,
            hand_index: INITIAL_HAND_INDEX,
            cards_mode: CardsMode::Increasing,
            current_cards_count: 0,
        })
    }

    pub fn advance(&mut self, turn: Turn) -> Result<GameState, TurnError> {
        let current_player_id = self.get_current_player_id();

        if current_player_id != turn.player_id {
            return Err(TurnError::NotYourTurn);
        }

        let player = &self.decks[&current_player_id];

        if !player.deck.contains(&turn.card) {
            return Err(TurnError::NotYourCard);
        }

        if self.decks.values().any(|p| p.bid.is_none()) {
            return Err(TurnError::PlayersNotBidded);
        }

        if self.players.len() == 1 {
            return Ok(GameState::Ended(self.players[0]));
        }

        self.current_player_index += 1;
        self.turn_cards.push(turn);

        if self.turn_cards.len() == self.players.len() {
            self.remove_lifes();
            self.remove_losers();
            self.start_new_round();
            // todo needs to adjust the self.current_player_index and hand_index when a player is
            //removed
            return Ok(GameState::Running);
        }

        Ok(GameState::Running)
    }

    pub fn bid(&mut self, player: ObjectId, bid: usize) -> Result<(), BiddingError> {
        let player = self
            .decks
            .get_mut(&player)
            .ok_or(BiddingError::InvalidPlayer)?;

        if player.bid.is_some() {
            return Err(BiddingError::AlreadyBidded);
        }

        player.bid = Some(bid);

        Ok(())
    }

    fn get_current_player_id(&mut self) -> ObjectId {
        if self.current_player_index == self.players.len() {
            self.current_player_index = 0;
        }

        self.players[self.current_player_index]
    }

    fn start_new_round(&mut self) {
        self.hand_index += 1;

        if self.hand_index == self.players.len() {
            self.hand_index = 0;
        }

        self.current_player_index = self.hand_index;

        let (mode, count) =
            Self::get_new_cards_mode(self.cards_mode, self.hand_index, self.players.len());

        self.cards_mode = mode;

        self.decks = Self::get_decks(&self.players, count);
    }

    fn get_new_cards_mode(
        mode: CardsMode,
        count: usize,
        player_count: usize,
    ) -> (CardsMode, usize) {
        match mode {
            CardsMode::Increasing => {
                if count + 1 < MAX_AVAILABLE_CARDS / player_count {
                    (CardsMode::Increasing, count + 1)
                } else {
                    (CardsMode::Decreasing, count - 1)
                }
            }
            CardsMode::Decreasing => {
                if count - 1 == 0 {
                    (CardsMode::Increasing, count + 1)
                } else {
                    (CardsMode::Decreasing, count - 1)
                }
            }
        }
    }

    fn get_decks(players: &[ObjectId], cards: usize) -> HashMap<ObjectId, Player> {
        let mut deck = Card::shuffled_deck();

        players
            .iter()
            .map(|p| (*p, Player::new(*p, deck.drain(..cards).collect())))
            .collect()
    }

    fn remove_lifes(&mut self) {
        let lost = self
            .decks
            .iter_mut()
            .filter(|(_, p)| p.bid != Some(p.rounds));

        for (_, player) in lost {
            player.lifes -= 1;
        }
    }

    fn remove_losers(&mut self) {
        let dead_indexes = self
            .decks
            .iter()
            .enumerate()
            .filter(|(_, (_, p))| p.lifes == 0)
            .map(|(i, _)| i);

        for i in dead_indexes {
            self.players.remove(i);
        }

        self.decks.retain(|_, p| p.lifes != 0)
    }
}

pub enum GameState {
    Running,
    Ended(ObjectId),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CardsMode {
    Increasing,
    Decreasing,
}

#[derive(Debug)]
pub enum GameError {
    NotEnoughPlayers,
    TooManyPlayers,
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
    use super::*;

    #[test]
    fn test_game() {
        let player1 = ObjectId::new();
        let player2 = ObjectId::new();

        let mut game = Game::new(vec![player1, player2]).unwrap();

        assert!(game.turn_cards.is_empty());
        assert!(game.current_player_index == 0);
        assert!(game.hand_index == 0);

        let first_played_card = game.decks[&player1].deck[0];
        let first_turn = Turn {
            player_id: player1,
            card: first_played_card,
        };

        game.bid(player1, 1).expect("Valid bid");
        game.bid(player2, 2).expect("Valid bid");

        game.advance(first_turn).expect("Valid turn");

        assert!(game.turn_cards.len() == 1);
        assert!(game.current_player_index == 1);
        assert!(game.turn_cards.peek().map(|t| t.card) == Some(first_played_card));

        let second_played_card = game.decks[&player2].deck[0];
        let second_turn = Turn {
            player_id: player2,
            card: second_played_card,
        };

        game.advance(second_turn).expect("Valid turn");

        assert!(game.turn_cards.len() == 2);
        assert!(game.current_player_index == 1);

        assert!(game.hand_index == 1);
    }

    #[test]
    fn test_card_mode() {
        assert_eq!(
            Game::get_new_cards_mode(CardsMode::Increasing, 1, 4),
            (CardsMode::Increasing, 2)
        );

        assert_eq!(
            Game::get_new_cards_mode(CardsMode::Decreasing, 1, 4),
            (CardsMode::Increasing, 2)
        );

        assert_eq!(
            Game::get_new_cards_mode(CardsMode::Increasing, 2, 4),
            (CardsMode::Increasing, 3)
        );

        assert_eq!(
            Game::get_new_cards_mode(CardsMode::Increasing, 7, 5),
            (CardsMode::Decreasing, 6)
        );
    }
}
