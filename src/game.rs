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
    AlreadyExists,
    AlreadyPlaying,
    CapacityReached,
    DoesNotExist,
    InsufficientFunds,
}

#[derive(Eq, PartialEq)]
pub enum UserState {
    Spectating,
    Playing,
    Waitlisted,
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
    StateTransition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd)]
pub enum GameState {
    SeatPlayers,
    MoveButton,
    CollectBlinds,
    Deal,
    TakeAction,
    Flop,
    Turn,
    River,
    EvalPot,
    AwardPot,
    RemovePlayers,
    DivideDonations,
    UpdateBlinds,
    BootPlayers,
}

pub struct ActionOptions {
    next_state: GameState,
    next_action_idx: Option<usize>,
    board: Vec<poker::Card>,
}

/// A poker game.
pub struct Game {
    next_state: GameState,
    deck: [poker::Card; 52],
    donations: u16,
    small_blind: u16,
    big_blind: u16,
    users: HashMap<String, User>,
    spectators: HashSet<String>,
    waitlist: VecDeque<String>,
    players_to_spectate: BTreeSet<String>,
    players_to_remove: BTreeSet<String>,
    table: [Option<Player>; MAX_PLAYERS],
    pots: Vec<Pot>,
    num_players: usize,
    deck_idx: usize,
    small_blind_idx: usize,
    big_blind_idx: usize,
    next_action_idx: usize,
}

/// Methods for managing players.
impl Game {
    pub fn boot_players(&mut self) -> Result<GameState, GameError> {
        if self.next_state != GameState::BootPlayers {
            return Err(GameError::StateTransition);
        }
        for seat in self.table.iter_mut() {
            if seat.is_some() {
                let player = seat.as_mut().unwrap();
                let user = self.users.get(&player.name).unwrap();
                if user.money < self.big_blind {
                    self.players_to_spectate.insert(player.name.clone());
                } else {
                    player.cards.clear();
                }
            }
        }
        while !self.players_to_spectate.is_empty() {
            let username = self.players_to_spectate.pop_first().unwrap();
            self.spectate_user(&username).ok();
        }
        self.next_state = GameState::SeatPlayers;
        Ok(self.next_state)
    }

    pub fn remove_players(&mut self) -> Result<GameState, GameError> {
        if self.next_state != GameState::RemovePlayers {
            return Err(GameError::StateTransition);
        }
        while !self.players_to_remove.is_empty() {
            let username = self.players_to_remove.pop_first().unwrap();
            self.remove_user(&username).ok();
        }
        self.next_state = GameState::DivideDonations;
        Ok(self.next_state)
    }

    pub fn seat_players(&mut self) -> Result<GameState, GameError> {
        if self.next_state != GameState::SeatPlayers {
            return Err(GameError::StateTransition);
        }
        let mut i: usize = 0;
        while self.num_players < MAX_PLAYERS && !self.waitlist.is_empty() {
            if self.table[i].is_none() {
                let username = self.waitlist.pop_front().unwrap();
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
        self.next_state = GameState::MoveButton;
        Ok(self.next_state)
    }
}

/// Methods for managing users.
impl Game {
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

    pub fn remove_user(&mut self, username: &str) -> Result<bool, UserError> {
        let maybe_user = self.users.get_mut(username);
        if maybe_user.is_none() {
            return Err(UserError::DoesNotExist);
        }
        let user = maybe_user.unwrap();
        match user.state {
            // If the user is already playing, we can only remove them
            // during two game phases. We can remove them right before the seat
            // players state because the game hasn't started and there's
            // still time to seat another player to fill their seat. We can
            // also remove them right before or anytime after the remove players state
            // because 1) this is the state that's actually reserved for removing
            // players that've been queued for removal, and 2) we can alter the
            // game state just fine before the next seat players state.
            UserState::Playing => {
                // Need to remove the player from the spectate set just in
                // case they wanted to spectate, but then changed their
                // mind and just want to leave.
                //
                // We don't need to remove the player from the removal set because
                // the only time this method can be used and the removal set is
                // affected is when the user gets queued for removal.
                self.players_to_spectate.remove(username);
                if self.next_state == GameState::SeatPlayers
                    || self.next_state >= GameState::RemovePlayers
                {
                    self.spectate_user(&username).ok();
                    self.spectators.remove(username);
                } else {
                    // The player is still at the table while the game is ongoing.
                    // We don't want to disrupt gameplay, so we just queue the
                    // player for removal and remove them later.
                    self.players_to_remove.insert(username.to_string());
                    return Ok(false);
                }
            }
            UserState::Spectating => {
                self.spectators.remove(username);
            }
            UserState::Waitlisted => {
                // We can remove the user from the waitlist anytime we want.
                let player_idx = self.waitlist.iter().position(|u| u == username).unwrap();
                self.waitlist.remove(player_idx);
            }
        }
        let user = self.users.get_mut(username).unwrap();
        self.donations += user.money;
        user.money = 0;
        self.users.remove(username);
        Ok(true)
    }

    pub fn spectate_user(&mut self, username: &str) -> Result<bool, UserError> {
        let maybe_user = self.users.get_mut(username);
        if maybe_user.is_none() {
            return Err(UserError::DoesNotExist);
        }
        let user = maybe_user.unwrap();
        match user.state {
            // If the user is already playing, we can only spectate them
            // during two game phases. We can spectate them right before the seat
            // players state because the game hasn't started and there's
            // still time to seat another player to fill their seat. We can
            // also spectate them right before or anytime after the remove players state
            // because 1) this is the state that's actually reserved for removing
            // players that've been queued for removal, and 2) we can alter the
            // game state just fine before the next seat players state.
            UserState::Playing => {
                // Need to remove the player from the removal set just in
                // case they wanted to leave, but then changed their
                // mind and just want to spectate.
                //
                // We don't need to remove the player from the spectate set because
                // the only time this method can be used and the spectate set is
                // affected is when the user gets queued for spectating.
                self.players_to_remove.remove(username);
                if self.next_state == GameState::SeatPlayers
                    || self.next_state >= GameState::RemovePlayers
                {
                    let player_idx = self
                        .table
                        .iter()
                        .position(|o| o.as_ref().is_some_and(|p| p.name == username))
                        .unwrap();
                    self.table[player_idx] = None;
                    self.num_players -= 1;
                } else {
                    self.players_to_spectate.insert(username.to_string());
                    return Ok(false);
                }
            }
            // The user is already spectating, so we can just quietly
            // say that they're spectating.
            UserState::Spectating => return Ok(true),
            UserState::Waitlisted => {
                let player_idx = self.waitlist.iter().position(|u| u == username).unwrap();
                self.waitlist.remove(player_idx);
            }
        }
        self.spectators.insert(username.to_string());
        user.state = UserState::Spectating;
        Ok(true)
    }

    pub fn waitlist_user(&mut self, username: &str) -> Result<bool, UserError> {
        let maybe_user = self.users.get_mut(username);
        if maybe_user.is_none() {
            return Err(UserError::DoesNotExist);
        }
        let user = maybe_user.unwrap();
        // Need to remove the player from the removal and spectate sets just in
        // case they wanted to do one of those, but then changed their mind and
        // want to play again.
        self.players_to_spectate.remove(username);
        self.players_to_remove.remove(username);
        match user.state {
            // The user is already playing, so we don't need to do anything,
            // but we should acknowledge that the user still isn't
            // technically waitlisted.
            UserState::Playing => Ok(false),
            UserState::Spectating => {
                if user.money < self.big_blind {
                    return Err(UserError::InsufficientFunds);
                }
                self.spectators.remove(username);
                self.waitlist.push_back(username.to_string());
                user.state = UserState::Waitlisted;
                Ok(true)
            }
            // The user is already waitlisted, so we can just quietly
            // say that they're waitlisted.
            UserState::Waitlisted => Ok(true),
        }
    }
}

/// Methods for managing poker gameplay.
impl Game {
    pub fn collect_blinds(&mut self) -> Result<GameState, GameError> {
        if self.next_state != GameState::CollectBlinds {
            return Err(GameError::StateTransition);
        }
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
        self.next_state = GameState::Deal;
        Ok(self.next_state)
    }

    pub fn deal(&mut self) -> Result<ActionOptions, GameError> {
        if self.next_state != GameState::Deal {
            return Err(GameError::StateTransition);
        }
        self.deck.shuffle(&mut thread_rng());
        self.deck_idx = 0;

        let mut table = (0..MAX_PLAYERS).cycle().skip(self.small_blind_idx);
        // Deal 2 cards per player, looping over players and dealing them 1 card
        // at a time.
        while self.deck_idx < (2 * self.num_players) {
            let deal_idx = table.find(|&idx| self.table[idx].is_some()).unwrap();
            let player = self.table[deal_idx].as_mut().unwrap();
            player.cards.push(self.deck[deal_idx]);
            self.deck_idx += 1;
        }
        self.next_state = GameState::TakeAction;
        Ok(ActionOptions {
            next_state: self.next_state,
            next_action_idx: Some(self.next_action_idx),
            board: Vec::with_capacity(5),
        })
    }

    pub fn divide_donations(&mut self) -> Result<GameState, GameError> {
        if self.next_state != GameState::DivideDonations {
            return Err(GameError::StateTransition);
        }
        let num_users = self.users.len();
        if num_users > 0 {
            let donation_per_user = self.donations / num_users as u16;
            for (_, user) in self.users.iter_mut() {
                user.money += donation_per_user;
            }
            self.donations = 0;
        }
        self.next_state = GameState::UpdateBlinds;
        Ok(self.next_state)
    }

    pub fn move_button(&mut self) -> Result<GameState, GameError> {
        if self.next_state != GameState::MoveButton {
            return Err(GameError::StateTransition);
        }
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
        self.next_state = GameState::CollectBlinds;
        Ok(self.next_state)
    }

    pub fn new() -> Game {
        Game {
            next_state: GameState::SeatPlayers,
            deck: poker::new_deck(),
            donations: 0,
            small_blind: MIN_SMALL_BLIND,
            big_blind: MIN_BIG_BLIND,
            users: HashMap::with_capacity(MAX_USERS),
            spectators: HashSet::with_capacity(MAX_USERS),
            players_to_remove: BTreeSet::new(),
            waitlist: VecDeque::with_capacity(MAX_USERS),
            players_to_spectate: BTreeSet::new(),
            table: [const { None }; MAX_PLAYERS],
            pots: Vec::with_capacity(MAX_POTS),
            num_players: 0,
            deck_idx: 0,
            small_blind_idx: 0,
            big_blind_idx: 1,
            next_action_idx: 2,
        }
    }

    pub fn update_blinds(&mut self) -> Result<GameState, GameError> {
        if self.next_state != GameState::UpdateBlinds {
            return Err(GameError::StateTransition);
        }
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
        self.next_state = GameState::BootPlayers;
        Ok(self.next_state)
    }
}
