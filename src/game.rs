use crate::poker;

use std::collections::{HashMap, HashSet, VecDeque};

// Don't want too many people waiting to play the game.
pub const MAX_PLAYERS: usize = 10;
pub const MAX_USERS: usize = MAX_PLAYERS + 6;
// In the wild case that players have monotonically increasing
// stacks and they all go all-in.
pub const MAX_POTS: usize = MAX_PLAYERS - 1;
// Technically a hand can only consist of 7 cards, but we treat aces
// as two separate cards (1u8 and 14u8).
pub const MAX_CARDS: usize = 11;
// A player will be cleaned if they fold 20 rounds with the big blind.
pub const STARTING_STACK: u16 = 200;
pub const MIN_BIG_BLIND: u16 = STARTING_STACK / 20;
pub const MIN_SMALL_BLIND: u16 = MIN_BIG_BLIND / 2;

#[derive(Eq, PartialEq)]
pub enum UserState {
    Spectating,
    Queued,
    Playing,
}

pub struct User {
    money: u16,
    state: UserState,
}

impl User {
    pub fn new() -> User {
        User {
            money: STARTING_STACK,
            state: UserState::Spectating,
        }
    }
}

/// For users that're in a pot.
pub enum PlayerState {
    // Player is in the pot but is waiting for their first move
    // or for another pot to conclude.
    Wait,
    // Player reveals their cards after the pot is over.
    Show,
    // Player stakes all their stack.
    AllIn,
    // Player ups their stake.
    Raise,
    // Player matches the last bet.
    Call,
    // Player forfeits their stake.
    Fold,
    // Player wants to see the next card.
    Check,
}

pub struct Player {
    pub name: String,
    pub state: PlayerState,
    pub cards: Vec<poker::Card>,
}

impl Player {
    pub fn clear(&mut self) {
        self.name = "".to_string();
        self.state = PlayerState::Wait;
        self.cards.clear();
    }

    pub fn new(name: &str) -> Player {
        Player {
            name: name.to_string(),
            state: PlayerState::Wait,
            cards: Vec::with_capacity(MAX_CARDS),
        }
    }
}

#[derive(Debug)]
pub enum UserError {
    DoesNotExist,
    AlreadyExists,
    CapacityReached,
    Enqueue,
    Spectate,
}

pub struct Game {
    deck: [poker::Card; 52],
    small_blind: u16,
    big_blind: u16,
    users: HashMap<String, User>,
    spectators: HashSet<String>,
    queued_players: VecDeque<String>,
    players: [Player; MAX_PLAYERS],
    pots: Vec<(u16, Vec<usize>)>,
    num_players: usize,
    dealer_idx: usize,
    small_blind_idx: usize,
    big_blind_idx: usize,
}

impl Game {
    pub fn new() -> Game {
        Game {
            deck: poker::new_deck(),
            small_blind: MIN_SMALL_BLIND,
            big_blind: MIN_BIG_BLIND,
            users: HashMap::with_capacity(MAX_USERS),
            spectators: HashSet::with_capacity(MAX_USERS),
            queued_players: VecDeque::with_capacity(MAX_USERS),
            players: core::array::from_fn(|_| Player::new("")),
            pots: Vec::with_capacity(MAX_POTS),
            num_players: 0,
            dealer_idx: 0,
            small_blind_idx: 1,
            big_blind_idx: 2,
        }
    }

    pub fn new_user(&mut self, username: &str) -> Result<usize, UserError> {
        if self.users.len() == self.users.capacity() {
            return Err(UserError::CapacityReached);
        } else if self.users.contains_key(username) {
            return Err(UserError::AlreadyExists);
        }
        self.users.insert(username.to_string(), User::new());
        self.spectators.insert(username.to_string());
        Ok(self.users.len())
    }

    pub fn queue_user(&mut self, username: &str) -> Result<usize, UserError> {
        let maybe_user = self.users.get_mut(username);
        if maybe_user.is_none() {
            return Err(UserError::DoesNotExist);
        }
        let user = maybe_user.unwrap();
        if user.state != UserState::Spectating {
            return Err(UserError::Enqueue);
        }
        self.queued_players.push_back(username.to_string());
        user.state = UserState::Queued;
        Ok(self.queued_players.len())
    }

    pub fn spectate_user(&mut self, username: &str) -> Result<usize, UserError> {
        let maybe_user = self.users.get_mut(username);
        if maybe_user.is_none() {
            return Err(UserError::DoesNotExist);
        }
        let user = maybe_user.unwrap();
        match user.state {
            UserState::Queued => {
                let player_idx = self
                    .queued_players
                    .iter()
                    .position(|x| x == username)
                    .unwrap();
                self.queued_players.remove(player_idx);
            }
            UserState::Playing => {
                let player_idx = self
                    .players
                    .iter()
                    .position(|x| x.name == username)
                    .unwrap();
                self.players[player_idx].clear();
            }
            UserState::Spectating => {
                return Err(UserError::Spectate);
            }
        }
        self.spectators.insert(username.to_string());
        user.state = UserState::Spectating;
        Ok(self.spectators.len())
    }
}
