use std::collections::BinaryHeap;

use indexmap::IndexMap;

use crate::models::GameError;

use super::{BiddingError, BiddingRound, Card, DealingMode, GameEvent, Player, Turn, TurnError};

#[derive(Debug)]
pub struct Game {
    decks: IndexMap<String, Player>,
    hand_index: usize,
    current_player_index: usize,
    round_cards: BinaryHeap<Turn>,
    dealing_mode: DealingMode,
    current_cards_count: usize,
}

const MAX_AVAILABLE_CARDS: usize = 40 - 1;
const MAX_PLAYER_COUNT: usize = 10;

impl Game {
    pub fn new(players: Vec<String>) -> Result<Self, GameError> {
        const INITIAL_HAND_INDEX: usize = 0;

        validate_game(&players)?;

        let decks = Self::get_decks(&players, INITIAL_HAND_INDEX + 1);

        Ok(Self {
            decks,
            round_cards: BinaryHeap::new(),
            current_player_index: INITIAL_HAND_INDEX,
            hand_index: INITIAL_HAND_INDEX,
            dealing_mode: DealingMode::Increasing,
            current_cards_count: 1,
        })
    }

    pub fn clone_decks(&self) -> IndexMap<String, Vec<Card>> {
        self.decks
            .iter()
            .map(|(id, p)| (id.clone(), p.deck.clone()))
            .collect()
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

        if self.decks.len() == 1 {
            let first = self.decks.first().expect("Should contain one");
            return Ok(GameEvent::GameEnded(first.0.to_string()));
        }

        self.current_player_index += 1;
        self.round_cards.push(turn);

        if self.round_cards.len() == self.decks.len() {
            self.remove_lifes();
            self.remove_losers();
            self.start_new_round();
            // todo needs to adjust the self.current_player_index and hand_index when a player is
            //removed
        }

        Ok(GameEvent::TurnPlayed)
    }

    pub fn bid(&mut self, player_id: &str, bid: usize) -> Result<BiddingRound, BiddingError> {
        let player = self
            .decks
            .get_mut(player_id)
            .ok_or(BiddingError::InvalidPlayer)?;

        if player.bid.is_some() {
            return Err(BiddingError::AlreadyBidded);
        }

        player.bid = Some(bid);

        Ok(todo!("Need to implement a system to handle the bidding loop and to get the next player to bid"))
    }

    pub fn next_bidder(&mut self) -> Option<String> {
        todo!()
    }

    fn get_current_player_id(&mut self) -> String {
        if self.current_player_index == self.decks.len() {
            self.current_player_index = 0;
        }

        let player = self
            .decks
            .get_index(self.current_player_index)
            .expect("Should have this index");

        player.0.to_string()
    }

    fn start_new_round(&mut self) {
        self.hand_index += 1;

        if self.hand_index == self.decks.len() {
            self.hand_index = 0;
        }

        self.current_player_index = self.hand_index;

        let (mode, count) =
            Self::get_new_cards_mode(self.dealing_mode, self.hand_index, self.decks.len());

        self.dealing_mode = mode;

        let players: Vec<_> = self.decks.keys().cloned().collect();

        self.decks = Self::get_decks(&players, count);
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

    fn get_decks(players: &[String], cards: usize) -> IndexMap<String, Player> {
        let mut deck = Card::shuffled_deck();

        players
            .iter()
            .map(|p| (p.to_string(), Player::new(deck.drain(..cards).collect())))
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
        self.decks.retain(|_, p| p.lifes != 0)
    }
}

fn validate_game(players: &[String]) -> Result<(), GameError> {
    if players.len() < 2 {
        return Err(GameError::NotEnoughPlayers);
    }

    if players.len() > MAX_PLAYER_COUNT {
        return Err(GameError::TooManyPlayers);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game() {
        let player1 = "P1".to_string();
        let player2 = "P2".to_string();

        let mut game = Game::new(vec![player1.clone(), player2.clone()]).unwrap();

        assert!(game.round_cards.is_empty());
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

        assert!(game.round_cards.len() == 1);
        assert!(game.current_player_index == 1);
        assert!(game.round_cards.peek().map(|t| t.card) == Some(first_played_card));

        let second_played_card = game.decks[&player2].deck[0];
        let second_turn = Turn {
            player_id: player2.clone(),
            card: second_played_card,
        };

        game.advance(second_turn).unwrap();

        assert!(game.round_cards.len() == 2);
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
