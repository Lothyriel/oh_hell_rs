use std::collections::{BinaryHeap, HashMap};

use indexmap::IndexMap;

use crate::models::GameError;

use super::{
    iter::CyclicIterator, BiddingError, Card, DealingMode, GameEvent, Player, RoundInfo,
    RoundState, Turn, TurnError,
};

#[derive(Debug)]
pub struct Game {
    decks: IndexMap<String, Player>,
    round_cards: BinaryHeap<Turn>,
    dealing_mode: DealingMode,
    cyclic: CyclicIterator<String>,
    cards_count: usize,
}

#[derive(PartialEq, Debug)]
enum CycleStage {
    Dealing,
    Bidding,
}

const MAX_AVAILABLE_CARDS: usize = 40 - 1;
const MAX_PLAYER_COUNT: usize = 10;

impl Game {
    pub fn new(players: Vec<String>) -> Result<Self, GameError> {
        validate_game(&players)?;

        let initial_cards_count = 1;

        let decks = Self::get_decks(&players, initial_cards_count);

        Ok(Self {
            decks,
            round_cards: BinaryHeap::new(),
            dealing_mode: DealingMode::Increasing,
            cards_count: initial_cards_count,
            cyclic: CyclicIterator::new(players),
        })
    }

    pub fn clone_decks(&self) -> IndexMap<String, Vec<Card>> {
        self.decks
            .iter()
            .map(|(id, p)| (id.clone(), p.deck.clone()))
            .collect()
    }

    pub fn deal(&mut self, turn: Turn) -> Result<(RoundInfo, Vec<GameEvent>), TurnError> {
        if self.get_cycle_stage() == CycleStage::Bidding {
            return Err(TurnError::BiddingStageActive);
        }

        let player = self
            .decks
            .get_mut(&turn.player_id)
            .ok_or(TurnError::InvalidPlayer)?;

        let current_dealer = self.cyclic.peek();

        if current_dealer != Some(&turn.player_id) {
            return Err(TurnError::NotYourTurn);
        }

        if !player.deck.contains(&turn.card) {
            return Err(TurnError::NotYourCard);
        }

        player.deck.retain(|&c| c != turn.card);

        //add card to the heap
        self.round_cards.push(turn);
        let mut events = vec![];

        // finish round
        if self.round_cards.len() == self.decks.len() {
            let winner = self
                .round_cards
                .iter()
                .next()
                .expect("Should contain a turn");

            events.push(GameEvent::RoundEnded(self.get_points()));
            self.award_points(winner.clone());
        }

        //finish set
        if self.decks.iter().all(|(_, p)| p.deck.is_empty()) {
            // todo send message when player loses a life
            self.remove_lifes();
            self.remove_losers();

            events.push(GameEvent::SetEnded(self.get_lifes()));

            self.start_new_set();
        }

        if self.decks.len() == 1 {
            let first = self.decks.first().expect("Should contain one");
            events.push(GameEvent::Ended {
                winner: first.0.to_string(),
            })
        }

        self.cyclic.next();

        let info = match self.cyclic.peek() {
            Some(n) => RoundInfo::new(n.clone(), RoundState::Active),
            None => {
                let next = self.cyclic.advance();

                RoundInfo::new(next, RoundState::Ended)
            }
        };

        Ok((info, events))
    }

    pub fn bid(&mut self, player_id: &String, bid: usize) -> Result<RoundInfo, BiddingError> {
        if self.get_cycle_stage() == CycleStage::Dealing {
            return Err(BiddingError::DealingStageActive);
        }

        if bid > self.cards_count {
            return Err(BiddingError::BidOutOfRange);
        }

        let player = self
            .decks
            .get_mut(player_id)
            .ok_or(BiddingError::InvalidPlayer)?;

        let current_bidder = self.cyclic.peek();

        if Some(player_id) != current_bidder {
            return Err(BiddingError::NotYourTurn);
        }

        if player.bid.is_some() {
            return Err(BiddingError::AlreadyBidded);
        }

        player.bid = Some(bid);

        self.cyclic.next();

        let info = match self.cyclic.peek() {
            Some(n) => RoundInfo::new(n.clone(), RoundState::Active),
            None => {
                let next = self.cyclic.reset();

                RoundInfo::new(next, RoundState::Ended)
            }
        };

        Ok(info)
    }

    fn get_cycle_stage(&mut self) -> CycleStage {
        match self.decks.values().any(|p| p.bid.is_none()) {
            true => CycleStage::Bidding,
            false => CycleStage::Dealing,
        }
    }

    fn start_new_set(&mut self) {
        let (mode, count) =
            Self::get_new_cards_mode(self.dealing_mode, self.cards_count, self.decks.len());

        self.dealing_mode = mode;
        self.cards_count = count;

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

    fn award_points(&mut self, turn: Turn) {
        let player = self
            .decks
            .get_mut(&turn.player_id)
            .expect("This player should exist here");

        player.rounds += 1;
    }

    fn get_points(&self) -> HashMap<String, usize> {
        self.decks
            .iter()
            .map(|(id, player)| (id.clone(), player.rounds))
            .collect()
    }

    fn get_lifes(&self) -> HashMap<String, usize> {
        self.decks
            .iter()
            .map(|(id, player)| (id.clone(), player.lifes))
            .collect()
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

        let first_played_card = game.decks[&player1].deck[0];
        let first_turn = Turn {
            player_id: player1.clone(),
            card: first_played_card,
        };

        let info = game.bid(&player1, 1).unwrap();
        assert_eq!(info.next, player2);
        assert_eq!(info.state, RoundState::Active);

        let info = game.bid(&player2, 1).unwrap();
        assert_eq!(info.next, player1);
        assert_eq!(info.state, RoundState::Ended);

        game.deal(first_turn).unwrap();

        assert!(game.round_cards.len() == 1);
        assert!(game.round_cards.peek().map(|t| t.card) == Some(first_played_card));

        let second_played_card = game.decks[&player2].deck[0];
        let second_turn = Turn {
            player_id: player2.clone(),
            card: second_played_card,
        };

        game.deal(second_turn).unwrap();

        assert!(game.round_cards.len() == 2);
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
