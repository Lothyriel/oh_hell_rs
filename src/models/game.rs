use std::collections::{BinaryHeap, HashMap};

use crate::models::GameError;

use super::{BiddingError, Card, DealingMode, GameEvent, GameState, Player, Turn, TurnError};

#[derive(Debug)]
pub struct Game {
    decks: HashMap<String, Player>,
    players: Vec<String>,
    hand_index: usize,
    current_player_index: usize,
    turn_cards: BinaryHeap<Turn>,
    dealing_mode: DealingMode,
    current_cards_count: usize,
}

const MAX_AVAILABLE_CARDS: usize = 40 - 1;
const MAX_PLAYER_COUNT: usize = 10;

impl Game {
    pub fn new(players: Vec<String>) -> Result<Self, GameError> {
        const INITIAL_HAND_INDEX: usize = 0;

        if players.len() < 2 {
            return Err(GameError::NotEnoughPlayers);
        }

        if players.len() > MAX_PLAYER_COUNT {
            return Err(GameError::TooManyPlayers);
        }

        let decks = Self::get_decks(&players, INITIAL_HAND_INDEX + 1);

        Ok(Self {
            decks,
            players,
            turn_cards: BinaryHeap::new(),
            current_player_index: INITIAL_HAND_INDEX,
            hand_index: INITIAL_HAND_INDEX,
            dealing_mode: DealingMode::Increasing,
            current_cards_count: 1,
        })
    }

    pub fn advance(&mut self, turn: Turn) -> Result<GameEvent, TurnError> {
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
            return Ok(GameEvent::GameEnded(self.players[0].to_string()));
        }

        self.current_player_index += 1;
        self.turn_cards.push(turn);

        if self.turn_cards.len() == self.players.len() {
            self.remove_lifes();
            self.remove_losers();
            self.start_new_round();
            // todo needs to adjust the self.current_player_index and hand_index when a player is
            //removed
        }

        Ok(GameEvent::TurnPlayed)
    }

    pub fn bid(&mut self, player_id: &str, bid: usize) -> Result<(), BiddingError> {
        let player = self
            .decks
            .get_mut(player_id)
            .ok_or(BiddingError::InvalidPlayer)?;

        if player.bid.is_some() {
            return Err(BiddingError::AlreadyBidded);
        }

        player.bid = Some(bid);

        Ok(())
    }

    fn get_current_player_id(&mut self) -> String {
        if self.current_player_index == self.players.len() {
            self.current_player_index = 0;
        }

        self.players[self.current_player_index].to_string()
    }

    fn start_new_round(&mut self) {
        self.hand_index += 1;

        if self.hand_index == self.players.len() {
            self.hand_index = 0;
        }

        self.current_player_index = self.hand_index;

        let (mode, count) =
            Self::get_new_cards_mode(self.dealing_mode, self.hand_index, self.players.len());

        self.dealing_mode = mode;

        self.decks = Self::get_decks(&self.players, count);
    }

    fn get_new_cards_mode(
        mode: DealingMode,
        count: usize,
        player_count: usize,
    ) -> (DealingMode, usize) {
        match mode {
            DealingMode::Increasing => {
                if count + 1 < MAX_AVAILABLE_CARDS / player_count {
                    (DealingMode::Increasing, count + 1)
                } else {
                    (DealingMode::Decreasing, count - 1)
                }
            }
            DealingMode::Decreasing => {
                if count - 1 == 0 {
                    (DealingMode::Increasing, count + 1)
                } else {
                    (DealingMode::Decreasing, count - 1)
                }
            }
        }
    }

    fn get_decks(players: &[String], cards: usize) -> HashMap<String, Player> {
        let mut deck = Card::shuffled_deck();

        // TODO try to return a slice

        players
            .into_iter()
            .map(|p| {
                (
                    p.to_string(),
                    Player::new(p.to_string(), deck.drain(..cards).collect()),
                )
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game() {
        let player1 = "P1".to_string();
        let player2 = "P2".to_string();

        let mut game = Game::new(vec![player1.clone(), player2.clone()]).unwrap();

        assert!(game.turn_cards.is_empty());
        assert!(game.current_player_index == 0);
        assert!(game.hand_index == 0);

        let first_played_card = game.decks[&player1].deck[0];
        let first_turn = Turn {
            player_id: player1.clone(),
            card: first_played_card,
        };

        game.bid(&player1, 1).unwrap();
        game.bid(&player2, 2).unwrap();

        game.advance(first_turn).unwrap();

        assert!(game.turn_cards.len() == 1);
        assert!(game.current_player_index == 1);
        assert!(game.turn_cards.peek().map(|t| t.card) == Some(first_played_card));

        let second_played_card = game.decks[&player2].deck[0];
        let second_turn = Turn {
            player_id: player2.clone(),
            card: second_played_card,
        };

        game.advance(second_turn).unwrap();

        assert!(game.turn_cards.len() == 2);
        assert!(game.current_player_index == 1);

        assert!(game.hand_index == 1);
    }

    #[test]
    fn test_card_mode() {
        assert_eq!(
            Game::get_new_cards_mode(DealingMode::Increasing, 1, 4),
            (DealingMode::Increasing, 2)
        );

        assert_eq!(
            Game::get_new_cards_mode(DealingMode::Decreasing, 1, 4),
            (DealingMode::Increasing, 2)
        );

        assert_eq!(
            Game::get_new_cards_mode(DealingMode::Increasing, 2, 4),
            (DealingMode::Increasing, 3)
        );

        assert_eq!(
            Game::get_new_cards_mode(DealingMode::Increasing, 7, 5),
            (DealingMode::Decreasing, 6)
        );
    }
}
