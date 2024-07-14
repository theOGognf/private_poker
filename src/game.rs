use crate::poker;

use rand::seq::SliceRandom;
use rand::thread_rng;
use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    u16::MAX,
};

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

pub struct Pot {
    pub size: u16,
    pub seat_indices: Vec<usize>,
}

impl Pot {
    pub fn new() -> Pot {
        Pot {
            size: 0,
            seat_indices: Vec::with_capacity(MAX_PLAYERS),
        }
    }
}

#[derive(Debug)]
pub enum GameError {
    NotEnoughPlayers,
}

/// A poker game.
///
pub struct Game {
    deck: [poker::Card; 52],
    donations: u16,
    small_blind: u16,
    big_blind: u16,
    users: HashMap<String, User>,
    spectators: HashSet<String>,
    players_to_seat: VecDeque<String>,
    players_to_spectate: BTreeSet<String>,
    users_to_remove: BTreeSet<String>,
    table: [Option<Player>; MAX_PLAYERS],
    pots: Vec<Pot>,
    num_players: usize,
    deck_idx: usize,
    small_blind_idx: usize,
    big_blind_idx: usize,
    next_action_idx: usize,
}

impl Game {
    pub fn boot_players(&mut self) -> usize {
        for seat in self.table.iter() {
            if seat.is_some() {
                let player = seat.as_ref().unwrap();
                let user = self.users.get(&player.name).unwrap();
                if user.money < self.big_blind {
                    self.players_to_spectate.insert(player.name.clone());
                }
            }
        }
        while !self.players_to_spectate.is_empty() {
            let username = self.players_to_spectate.pop_first().unwrap();
            // Users are removed prior to `boot_players` being called, so
            // a user that's being put in spectators should always exist.
            self.spectate_user(&username).ok();
        }
        self.num_players
    }

    pub fn collect_blinds(&mut self) {
        self.pots.clear();
        self.pots.push(Pot::new());
        let pot = &mut self.pots[0];
        for (seat_idx, blind) in [
            (self.small_blind_idx, self.small_blind),
            (self.big_blind_idx, self.big_blind),
        ] {
            let player = self.table[seat_idx].as_ref().unwrap();
            let user = self.users.get_mut(&player.name).unwrap();
            pot.size += blind;
            user.money -= blind;
        }
    }

    pub fn deal(&mut self) -> usize {
        self.deck.shuffle(&mut thread_rng());
        self.deck_idx = 0;

        let mut table = (0..MAX_PLAYERS).cycle().skip(self.small_blind_idx);
        // Deal 2 cards per player, looping over players and dealing them 1 card
        // at a time.
        while self.deck_idx < (2 * self.num_players) {
            let deal_idx = table.find(|&idx| self.table[idx].is_some()).unwrap();
            let player = self.table[deal_idx].as_mut().unwrap();
            player.cards.clear();
            player.cards.push(self.deck[deal_idx]);
            self.deck_idx += 1;
        }
        self.next_action_idx
    }

    pub fn divide_donations(&mut self) {
        let num_users = self.users.len();
        if num_users > 0 {
            let donation_per_user = self.donations / num_users as u16;
            for (_, user) in self.users.iter_mut() {
                user.money += donation_per_user;
            }
            self.donations = 0;
        }
    }

    pub fn move_button(&mut self) -> Result<usize, GameError> {
        if self.num_players <= 1 {
            return Err(GameError::NotEnoughPlayers);
        }
        // Search for the big blind and starting positions.
        let mut table = self.table.iter().cycle();
        table.nth(self.big_blind_idx + 1);
        self.big_blind_idx = table.position(|p| p.is_some()).unwrap();
        self.next_action_idx = table.position(|p| p.is_some()).unwrap();
        // Reverse the table search to find the small blind position relative
        // to the big blind position since the small blind must always trail the big
        // blind.
        let mut table = self.table.iter();
        table.nth(self.big_blind_idx);
        let mut reverse_table = table.rev().cycle();
        self.small_blind_idx = reverse_table.position(|p| p.is_some()).unwrap();
        Ok(self.next_action_idx)
    }

    pub fn new() -> Game {
        Game {
            deck: poker::new_deck(),
            donations: 0,
            small_blind: MIN_SMALL_BLIND,
            big_blind: MIN_BIG_BLIND,
            users: HashMap::with_capacity(MAX_USERS),
            spectators: HashSet::with_capacity(MAX_USERS),
            players_to_seat: VecDeque::with_capacity(MAX_USERS),
            players_to_spectate: BTreeSet::new(),
            users_to_remove: BTreeSet::new(),
            table: [const { None }; MAX_PLAYERS],
            pots: Vec::with_capacity(MAX_POTS),
            num_players: 0,
            deck_idx: 0,
            small_blind_idx: 0,
            big_blind_idx: 1,
            next_action_idx: 2,
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

    pub fn queue_user_for_play(&mut self, username: &str) -> Result<usize, UserError> {
        let maybe_user = self.users.get_mut(username);
        if maybe_user.is_none() {
            return Err(UserError::DoesNotExist);
        }
        let user = maybe_user.unwrap();
        if user.state == UserState::Spectating {
            if user.money < self.big_blind {
                return Err(UserError::InsufficientFunds);
            }
            self.spectators.remove(username);
            self.players_to_seat.push_back(username.to_string());
            user.state = UserState::Queued;
        }
        Ok(self.players_to_seat.len())
    }

    pub fn queue_user_for_removal(&mut self, username: &str) -> Result<usize, UserError> {
        let maybe_user = self.users.get_mut(username);
        if maybe_user.is_none() {
            return Err(UserError::DoesNotExist);
        }
        self.users_to_remove.insert(username.to_string());
        Ok(self.users_to_remove.len())
    }

    pub fn remove_users(&mut self) -> usize {
        while !self.users_to_remove.is_empty() {
            let username = self.users_to_remove.pop_first().unwrap();
            let user = self.users.get_mut(&username).unwrap();
            self.donations += user.money;
            user.money = 0;
            self.spectate_user(&username).ok();
            self.spectators.remove(&username);
            self.users.remove(&username);
        }
        self.users.len()
    }

    pub fn seat_players(&mut self) -> usize {
        let mut i: usize = 0;
        while self.num_players < MAX_PLAYERS && !self.players_to_seat.is_empty() {
            if self.table[i].is_none() {
                let username = self.players_to_seat.pop_front().unwrap();
                let user = self.users.get_mut(&username).unwrap();
                if user.money < self.big_blind {
                    self.spectate_user(&username).ok();
                } else {
                    self.table[i] = Some(Player::new(&username));
                    user.state = UserState::Playing;
                    self.num_players += 1;
                }
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
                self.num_players -= 1;
            }
            UserState::Queued => {
                let player_idx = self
                    .players_to_seat
                    .iter()
                    .position(|u| u == username)
                    .unwrap();
                self.players_to_seat.remove(player_idx);
            }
            UserState::Spectating => return Ok(self.spectators.len()),
        }
        self.spectators.insert(username.to_string());
        user.state = UserState::Spectating;
        Ok(self.spectators.len())
    }

    pub fn update_blinds(&mut self) {
        let mut min_money = MAX;
        for (_, user) in self.users.iter() {
            if user.money < min_money {
                min_money = user.money;
            }
        }
        if min_money < MAX && min_money > (2 * self.big_blind) {
            self.small_blind *= 2;
            self.big_blind *= 2;
        }
    }
}
