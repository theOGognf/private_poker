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

#[derive(Debug)]
pub enum UserError {
    DoesNotExist,
    AlreadyExists,
    CapacityReached,
    AlreadyPlaying,
    InsufficientFunds,
}

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
    pub fn new(name: &str) -> Player {
        Player {
            name: name.to_string(),
            state: PlayerState::Wait,
            cards: Vec::with_capacity(MAX_CARDS),
        }
    }
}

#[derive(Debug)]
pub enum GameError {
    NotEnoughPlayers,
}

pub struct Game {
    deck: [poker::Card; 52],
    small_blind: u16,
    big_blind: u16,
    users: HashMap<String, User>,
    spectators: HashSet<String>,
    queued_players: VecDeque<String>,
    table: [Option<Player>; MAX_PLAYERS],
    pots: Vec<(u16, Vec<usize>)>,
    num_players: usize,
    starting_player_idx: usize,
    small_blind_idx: usize,
    big_blind_idx: usize,
}

impl Game {
    pub fn end_hand(&mut self) {
        
    }

    pub fn move_button(&mut self) -> Result<usize, GameError> {
        if self.num_players == 1 {
            return Err(GameError::NotEnoughPlayers);
        }
        // Move the big blind position and starting position.
        let mut table = self.table.iter().cycle();
        table.nth(self.big_blind_idx + 1);
        self.big_blind_idx = table.position(|p| p.is_some()).unwrap();
        self.starting_player_idx = table.position(|p| p.is_some()).unwrap();
        // Reverse the table search to find the small blind position relative
        // to the big blind position.
        let mut table = self.table.iter();
        table.nth(self.big_blind_idx);
        let mut reverse_table = table.rev().cycle();
        self.small_blind_idx = reverse_table.position(|p| p.is_some()).unwrap();
        Ok(self.starting_player_idx)
    }

    pub fn new() -> Game {
        Game {
            deck: poker::new_deck(),
            small_blind: MIN_SMALL_BLIND,
            big_blind: MIN_BIG_BLIND,
            users: HashMap::with_capacity(MAX_USERS),
            spectators: HashSet::with_capacity(MAX_USERS),
            queued_players: VecDeque::with_capacity(MAX_USERS),
            table: [const { None }; MAX_PLAYERS],
            pots: Vec::with_capacity(MAX_POTS),
            num_players: 0,
            small_blind_idx: 0,
            big_blind_idx: 1,
            starting_player_idx: 2,
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
        match user.state {
            UserState::Spectating => {
                if user.money < self.big_blind {
                    return Err(UserError::InsufficientFunds);
                }
                self.spectators.remove(username);
                self.queued_players.push_back(username.to_string());
                user.state = UserState::Queued;
            },
            _ => ()
        }
        Ok(self.queued_players.len())
    }

    pub fn seat_players(&mut self) -> usize {
        let mut i: usize = 0;
        while self.num_players < MAX_PLAYERS && !self.queued_players.is_empty() {
            if self.table[i].is_none() {
                let username = self.queued_players.pop_front().unwrap();
                let user = self.users.get(&username).unwrap();
                if user.money < self.big_blind {
                    self.spectate_user(&username).ok();
                    continue;
                }
                self.table[i] = Some(Player::new(&username));
                self.num_players += 1;
            }
            i += 1;
        }
        self.num_players
    }

    pub fn spectate_user(&mut self, username: &str) -> Result<usize, UserError> {
        let maybe_user = self.users.get_mut(username);
        if maybe_user.is_none() {
            return Err(UserError::DoesNotExist);
        }
        let user = maybe_user.unwrap();
        match user.state {
            UserState::Playing => {
                let player_idx = self
                    .table
                    .iter()
                    .position(|o| o.as_ref().is_some_and(|p| p.name == username))
                    .unwrap();
                self.table[player_idx] = None;
            }
            UserState::Queued => {
                let player_idx = self
                    .queued_players
                    .iter()
                    .position(|u| u == username)
                    .unwrap();
                self.queued_players.remove(player_idx);
            }
            UserState::Spectating => return Ok(self.spectators.len()),
        }
        self.spectators.insert(username.to_string());
        user.state = UserState::Spectating;
        Ok(self.spectators.len())
    }
}
