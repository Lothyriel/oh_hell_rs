use std::collections::{BinaryHeap, HashMap};

use indexmap::IndexMap;

use crate::models::GameError;

use super::{
    iter::CyclicIterator, BiddingError, Card, DealingMode, GameEvent, Player, RoundInfo,
    RoundState, Turn, TurnError,
};

#[derive(Debug)]
pub struct Game {
    players: IndexMap<String, Player>,
    round_cards: BinaryHeap<Turn>,
    dealing_mode: DealingMode,
    cyclic: CyclicIterator<String>,
    cards_count: usize,
    trump: Card,
}

#[derive(PartialEq, Debug)]
enum CycleStage {
    Dealing,
    Bidding,
}

const MAX_AVAILABLE_CARDS: usize = 40 - 1;
const MAX_PLAYER_COUNT: usize = 10;

impl Game {
    pub fn new(player_names: Vec<String>) -> Result<Self, GameError> {
        validate_game(&player_names)?;

        let initial_cards_count = 1;

        let (players, trump) = Self::init_players(&player_names, initial_cards_count);

        Ok(Self {
            players,
            round_cards: BinaryHeap::new(),
            dealing_mode: DealingMode::Increasing,
            cards_count: initial_cards_count,
            // TODO we need to reset this guy when we remove someone from
            // the game (player lost all lifes)
            cyclic: CyclicIterator::new(player_names),
            trump,
        })
    }

    pub fn clone_decks(&self) -> (IndexMap<String, Vec<Card>>, Card) {
        let decks = self
            .players
            .iter()
            .map(|(id, p)| (id.clone(), p.deck.clone()))
            .collect();

        (decks, self.trump)
    }

    pub fn deal(&mut self, turn: Turn) -> Result<(RoundInfo, Option<GameEvent>), TurnError> {
        if self.get_cycle_stage() == CycleStage::Bidding {
            return Err(TurnError::BiddingStageActive);
        }

        let player = self
            .players
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

        //finish game
        if self.players.len() == 1 {
            self.award_points();
            self.remove_lifes();
            self.remove_losers();

            let first = self.players.first().expect("Should contain one");

            let evt = GameEvent::Ended {
                winner: first.0.to_string(),
                lifes: self.get_lifes(),
            };

            return self.deal_round_data(Some(evt));
        }

        //finish set
        if self.players.iter().all(|(_, p)| p.deck.is_empty()) {
            self.award_points();
            self.remove_lifes();
            self.remove_losers();

            self.start_new_set();

            let (decks, trump) = self.clone_decks();

            let first = self.current_player();

            let evt = GameEvent::SetEnded {
                lifes: self.get_lifes(),
                possible: self.get_possible_bids(),
                first,
                trump,
                decks,
            };

            return self.deal_round_data(Some(evt));
        }

        // finish round
        if self.round_cards.len() == self.players.len() {
            self.award_points();

            let evt = GameEvent::RoundEnded(self.get_points());
            return self.deal_round_data(Some(evt));
        }

        self.deal_round_data(None)
    }

    fn deal_round_data(
        &mut self,
        event: Option<GameEvent>,
    ) -> Result<(RoundInfo, Option<GameEvent>), TurnError> {
        self.cyclic.next();

        let possible = self.get_possible_bids();

        let info = match self.cyclic.peek() {
            Some(n) => RoundInfo::new(n.clone(), RoundState::Active, possible),
            None => {
                let next = self.cyclic.advance();
                RoundInfo::new(next, RoundState::Ended, possible)
            }
        };

        Ok((info, event))
    }

    pub fn bid(&mut self, player_id: &String, bid: usize) -> Result<RoundInfo, BiddingError> {
        if self.get_cycle_stage() == CycleStage::Dealing {
            return Err(BiddingError::DealingStageActive);
        }

        if bid > self.cards_count || !self.valid_bid(bid) {
            return Err(BiddingError::BidOutOfRange);
        }

        let player = self
            .players
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

        let possible = self.get_possible_bids();

        let info = match self.cyclic.peek() {
            Some(n) => RoundInfo::new(n.clone(), RoundState::Active, possible),
            None => {
                let next = self.cyclic.reset();

                RoundInfo::new(next, RoundState::Ended, possible)
            }
        };

        Ok(info)
    }

    fn valid_bid(&self, bid: usize) -> bool {
        let current_bidding: usize = self
            .players
            .iter()
            .map(|(_, p)| p.bid.unwrap_or_default())
            .sum();

        self.cyclic.peek_next().is_some() || bid + current_bidding != self.cards_count
    }

    pub fn current_player(&self) -> String {
        self.cyclic.peek().expect("Should have a player").clone()
    }

    pub fn get_possible_bids(&self) -> Vec<usize> {
        let last = self.cyclic.peek_next().is_none();
        let n = self.players.len();

        if last {
            (0..n).filter(|&i| self.valid_bid(i)).collect()
        } else {
            (0..n).collect()
        }
    }

    pub fn get_pile(&self) -> Vec<Turn> {
        self.round_cards.iter().cloned().collect()
    }

    fn get_cycle_stage(&mut self) -> CycleStage {
        match self.players.values().any(|p| p.bid.is_none()) {
            true => CycleStage::Bidding,
            false => CycleStage::Dealing,
        }
    }

    fn start_new_set(&mut self) {
        let (mode, count) =
            Self::get_new_cards_mode(self.dealing_mode, self.cards_count, self.players.len());

        self.dealing_mode = mode;
        self.cards_count = count;

        let mut deck = Card::shuffled_deck();

        for (_, player) in self.players.iter_mut() {
            player.deck = deck.drain(..self.cards_count).collect();
            player.bid = None;
        }

        self.trump = deck[0];
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

    fn init_players(players: &[String], cards: usize) -> (IndexMap<String, Player>, Card) {
        let mut deck = Card::shuffled_deck();

        let decks = players
            .iter()
            .map(|p| (p.to_string(), Player::new(deck.drain(..cards).collect())))
            .collect();

        (decks, deck[0])
    }

    fn remove_lifes(&mut self) {
        let lost = self
            .players
            .iter_mut()
            .filter(|(_, p)| p.bid != Some(p.rounds));

        for (_, player) in lost {
            player.lifes -= 1;
        }
    }

    fn remove_losers(&mut self) {
        self.players.retain(|_, p| p.lifes != 0)
    }

    fn award_points(&mut self) {
        let winner = self
            .round_cards
            .iter()
            .next()
            .expect("Should contain a turn");

        let player = self
            .players
            .get_mut(&winner.player_id)
            .expect("This player should exist here");

        player.rounds += 1;
    }

    fn get_points(&self) -> HashMap<String, usize> {
        self.players
            .iter()
            .map(|(id, player)| (id.clone(), player.rounds))
            .collect()
    }

    fn get_lifes(&self) -> HashMap<String, usize> {
        self.players
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

        let info = game.bid(&player1, 1).unwrap();
        assert_eq!(info.next, player2);
        assert_eq!(info.state, RoundState::Active);

        let info = game.bid(&player2, 1).unwrap();
        assert_eq!(info.next, player1);
        assert_eq!(info.state, RoundState::Ended);

        let first_played_card = game.players[&player1].deck[0];
        let first_turn = Turn {
            player_id: player1,
            card: first_played_card,
        };

        game.deal(first_turn).unwrap();

        assert!(game.round_cards.len() == 1);
        assert!(game.round_cards.peek().map(|t| t.card) == Some(first_played_card));

        let second_played_card = game.players[&player2].deck[0];
        let second_turn = Turn {
            player_id: player2.clone(),
            card: second_played_card,
        };

        let info = game.deal(second_turn).unwrap();

        assert!(matches!(
            info.1,
            Some(GameEvent::SetEnded {
                lifes: _,
                trump: _,
                decks: _,
                first: _,
                possible: _
            })
        ));

        assert!(game.round_cards.len() == 2);

        let players = game.players.iter().filter(|(_, p)| p.lifes == 5).count();

        assert!(players == 1);
    }

    #[test]
    fn test_invalid_bid() {
        let player1 = "P1".to_string();
        let player2 = "P2".to_string();

        let mut game = Game::new(vec![player1.clone(), player2.clone()]).unwrap();

        let info = game.bid(&player1, 1).unwrap();
        assert_eq!(info.next, player2);
        assert_eq!(info.state, RoundState::Active);

        let result = game.bid(&player2, 0);
        assert_eq!(result, Err(BiddingError::BidOutOfRange));

        let possible = game.get_possible_bids();
        assert_eq!(possible, vec![1]);
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
