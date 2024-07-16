use crate::poker;

use rand::seq::SliceRandom;
use rand::thread_rng;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

// Don't want too many people waiting to play the game.
pub const MAX_PLAYERS: usize = 12;
pub const MAX_USERS: usize = MAX_PLAYERS + 6;
// In the wild case that players have monotonically increasing
// stacks and they all go all-in.
pub const MAX_POTS: usize = MAX_PLAYERS / 3;
// Technically a hand can only consist of 7 cards, but we treat aces
// as two separate cards (1u8 and 14u8).
pub const MAX_CARDS: usize = 11;
// A player will be cleaned if they fold 20 rounds with the big blind.
pub const STARTING_STACK: u16 = 200;
pub const MIN_BIG_BLIND: u16 = STARTING_STACK / 20;
pub const MIN_SMALL_BLIND: u16 = MIN_BIG_BLIND / 2;

#[derive(Debug)]
pub enum GameError {
    // A bet is considered a game error because it should never be
    // possible for a player to place an invalid bet.
    InvalidBet,
    NotEnoughPlayers,
    StateTransition,
}

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

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum Action {
    AllIn,
    Call,
    Check,
    Fold,
    Raise,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SidePotState {
    AllIn,
    Raise,
    CallOrReraise,
}

/// For users that're in a pot.
#[derive(PartialEq)]
pub enum PlayerState {
    // Player is in the pot but is waiting for their move.
    Wait,
    // Player put in their whole stack.
    AllIn,
    // Player forfeited their stack for the pot.
    Fold,
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

    pub fn reset(&mut self) {
        self.state = PlayerState::Wait;
        self.cards.clear();
    }
}

pub struct Bet {
    pub action: Action,
    pub amount: u16,
}

pub struct Pot {
    // The total investment for each player to remain in the hand.
    pub call: u16,
    // Size is just the sum of all investments in the pot.
    pub size: u16,
    // Map seat indices (players) to their investment in the pot.
    pub investments: HashMap<usize, u16>,
    // Used to check whether to spawn a side pot from this pot.
    // Should be `None` if no side pot conditions are met.
    pub side_pot_state: Option<SidePotState>,
}

impl Pot {
    pub fn bet(&mut self, seat_idx: usize, bet: &Bet) -> Result<Option<Pot>, GameError> {
        let investment = self.investments.entry(seat_idx).or_default();
        let mut new_call = self.call;
        let mut new_investment = *investment + bet.amount;
        let mut pot_increase = bet.amount;
        // A call must match the call amount. A raise must match the min raise
        // amount. There's an exception for all-ins; an all-in is treated as
        // a call without affecting the call if the all-in is less than the
        // previous call. An all-in is treated as a raise if the all-in is
        // greater than the call.
        match bet.action {
            Action::Call => {
                if new_investment != self.call {
                    return Err(GameError::InvalidBet);
                }
            }
            Action::Raise => {
                if new_investment < (2 * self.call) {
                    return Err(GameError::InvalidBet);
                }
                new_call = new_investment;
            }
            Action::AllIn => {
                if bet.amount > self.call {
                    new_call = new_investment;
                }
            }
            // A bet must call, raise, or all-in.
            _ => return Err(GameError::InvalidBet),
        }
        // Need to check whether a side pot is created. A side pot is created
        // when a player all-ins and then a subsequent player raises (an all-in
        // that is more than the previous all-in is considered a raise).
        // In this case, the call for the current pot remains unchanged, and
        // the pot is only increased by the original call. The excess
        // is used to start a new pot.
        let mut side_pot = None;
        match (bet.action, self.side_pot_state) {
            (Action::AllIn, None) => self.side_pot_state = Some(SidePotState::AllIn),
            (Action::AllIn, Some(SidePotState::AllIn))
            | (Action::Raise, Some(SidePotState::AllIn)) => {
                if new_investment > self.call {
                    self.side_pot_state = Some(SidePotState::Raise);
                    side_pot = Some(Pot::new());
                    side_pot.as_mut().unwrap().bet(
                        seat_idx,
                        &Bet {
                            action: bet.action,
                            amount: new_investment - self.call,
                        },
                    );
                    new_call = self.call;
                    pot_increase = self.call;
                    new_investment = self.call;
                }
            }
            _ => (),
        }
        // Finally, update the call, pot, and the player's investment
        // in the current pot.
        self.call = new_call;
        self.size += pot_increase;
        *investment = new_investment;
        Ok(side_pot)
    }

    /// Return the amount the player must bet to remain in the hand, and
    /// the minimum the player must raise by for it to be considered
    /// a valid raise.
    pub fn get_next_call(&mut self, seat_idx: usize) -> u16 {
        self.call - *self.investments.entry(seat_idx).or_default()
    }

    pub fn new() -> Pot {
        Pot {
            call: 0,
            size: 0,
            investments: HashMap::with_capacity(MAX_PLAYERS),
            side_pot_state: None,
        }
    }
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
    Showdown,
    RemovePlayers,
    DivideDonations,
    UpdateBlinds,
    BootPlayers,
}

pub struct ActionData {
    next_state: GameState,
    next_action_idx: Option<usize>,
    board: Vec<poker::Card>,
    options: HashSet<Action>,
}

pub struct ShowdownData {
    next_state: GameState,
    // Map seat indices to winnings.
    money_per_player: HashMap<usize, u16>,
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
    table: [Option<Player>; MAX_PLAYERS],
    board: Vec<poker::Card>,
    num_players: usize,
    pots: Vec<Pot>,
    players_to_spectate: BTreeSet<String>,
    players_to_remove: BTreeSet<String>,
    deck_idx: usize,
    small_blind_idx: usize,
    big_blind_idx: usize,
    next_action_idx: Option<usize>,
    prev_raise_idx: usize,
}

/// General game methods.
impl Game {
    pub fn new() -> Game {
        Game {
            next_state: GameState::SeatPlayers,
            deck: poker::new_deck(),
            donations: 0,
            small_blind: MIN_SMALL_BLIND,
            big_blind: MIN_BIG_BLIND,
            users: HashMap::with_capacity(MAX_USERS),
            spectators: HashSet::with_capacity(MAX_USERS),
            waitlist: VecDeque::with_capacity(MAX_USERS),
            table: [const { None }; MAX_PLAYERS],
            board: Vec::with_capacity(5),
            num_players: 0,
            pots: Vec::with_capacity(MAX_POTS),
            players_to_remove: BTreeSet::new(),
            players_to_spectate: BTreeSet::new(),
            deck_idx: 0,
            small_blind_idx: 0,
            big_blind_idx: 1,
            next_action_idx: Some(2),
            prev_raise_idx: 1,
        }
    }
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
                    player.reset();
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
            // Check if player already exists but is queued for removal.
            // This probably means the user disconnected and are trying
            // to reconnect.
            if !self.players_to_remove.remove(username) {
                return Err(UserError::AlreadyExists);
            } else {
                return Ok(self.users.len());
            }
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
        // The player has already been queued for removal. Just wait for
        // the next removal phase.
        if self.players_to_remove.contains(username) {
            return Ok(false);
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
                // Need to remove the player from other queues just in
                // case they changed their mind.
                self.players_to_spectate.remove(username);
                if self.next_state == GameState::SeatPlayers
                    || self.next_state >= GameState::RemovePlayers
                {
                    self.spectate_user(username).ok();
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
                let waitlist_idx = self.waitlist.iter().position(|u| u == username).unwrap();
                self.waitlist.remove(waitlist_idx);
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
        // The player has already been queued for spectate. Just wait for
        // the next spectate phase.
        if self.players_to_spectate.contains(username) {
            return Ok(false);
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
                // Need to remove the player from other queues just in
                // case they changed their mind.
                self.players_to_remove.remove(username);
                if self.next_state == GameState::SeatPlayers
                    || self.next_state >= GameState::RemovePlayers
                {
                    let seat_idx = self
                        .table
                        .iter()
                        .position(|o| o.as_ref().is_some_and(|p| p.name == username))
                        .unwrap();
                    self.table[seat_idx] = None;
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
                let waitlist_idx = self.waitlist.iter().position(|u| u == username).unwrap();
                self.waitlist.remove(waitlist_idx);
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
            pot.bet(
                seat_idx,
                &Bet {
                    action: Action::Raise,
                    amount: blind,
                },
            );
            user.money -= blind;
        }
        self.next_state = GameState::Deal;
        Ok(self.next_state)
    }

    pub fn deal(&mut self) -> Result<ActionData, GameError> {
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
            player.cards.push(self.deck[self.deck_idx]);
            self.deck_idx += 1;
        }
        self.next_state = GameState::TakeAction;
        Ok(ActionData {
            next_state: self.next_state,
            next_action_idx: self.next_action_idx,
            board: self.board.clone(),
            options: HashSet::from_iter([Action::AllIn, Action::Call, Action::Fold, Action::Raise]),
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

    pub fn flop(&mut self) -> Result<ActionData, GameError> {
        if self.next_state != GameState::Flop {
            return Err(GameError::StateTransition);
        }
        for _ in 0..3 {
            self.board.push(self.deck[self.deck_idx]);
            self.deck_idx += 1;
        }
        self.next_state = GameState::TakeAction;
        Ok(ActionData {
            next_state: self.next_state,
            next_action_idx: self.next_action_idx,
            board: self.board.clone(),
        })
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
        self.prev_raise_idx = self.big_blind_idx;
        self.next_action_idx = Some(table.position(|p| p.is_some()).unwrap());
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

    pub fn river(&mut self) -> Result<ActionData, GameError> {
        if self.next_state != GameState::River {
            return Err(GameError::StateTransition);
        }
        self.board.push(self.deck[self.deck_idx]);
        self.deck_idx += 1;
        self.next_state = GameState::TakeAction;
        Ok(ActionData {
            next_state: self.next_state,
            next_action_idx: self.next_action_idx,
            board: self.board.clone(),
        })
    }

    /// Get all players in the pot that haven't folded and compare their
    /// hands to one another. Get the winning indices and distribute
    /// the pot accordingly. If there's a tie, winners are given their
    /// original investments and then split the remainder. If there's
    /// a sole winner, then everyone else can only lose as much as the winner
    /// has invested. This prevents folks that went all-in, but have
    /// much higher money than the winner, from losing the extra
    /// money.
    pub fn showdown(&mut self) -> Result<ShowdownData, GameError> {
        if self.next_state != GameState::Showdown {
            return Err(GameError::StateTransition);
        }
        let mut pot = self.pots.pop().unwrap();
        let mut seats_in_pot = Vec::with_capacity(MAX_PLAYERS);
        let mut hands_in_pot = Vec::with_capacity(MAX_PLAYERS);
        for (seat_idx, _) in pot.investments.iter() {
            let player = self.table[*seat_idx].as_mut().unwrap();
            if player.state != PlayerState::Fold {
                seats_in_pot.push(*seat_idx);
                player.cards.sort_unstable();
                hands_in_pot.push(poker::eval(&player.cards));
            }
        }

        // Only up to 4 players can split the pot (only four suits per card value).
        let mut money_per_player: HashMap<usize, u16> = HashMap::with_capacity(4);
        let mut winner_indices = poker::argmax(&hands_in_pot);
        let num_winners = winner_indices.len();
        match num_winners {
            0 => unreachable!("There is always at least one player in the pot."),
            // Give the whole pot to the winner.
            1 => {
                let winner_idx = winner_indices.pop().unwrap();
                let winner_seat_idx = seats_in_pot[winner_idx];
                let (_, winner_investment) =
                    pot.investments.remove_entry(&winner_seat_idx).unwrap();
                for (seat_idx, investment) in pot.investments.iter() {
                    if *investment > winner_investment {
                        let remainder = investment - winner_investment;
                        money_per_player.insert(*seat_idx, remainder);
                        pot.size -= remainder;
                    }
                }
                money_per_player.insert(winner_seat_idx, pot.size);
            }
            // Split pot amongst multiple winners.
            _ => {
                // Need to first give each winner's original investment back
                // to them so the pot can be split fairly. The max winner
                // investment is tracked to handle the edge case of some
                // whale going all-in with no one else to call them.
                let mut money_per_winner: HashMap<usize, u16> = HashMap::with_capacity(4);
                let mut max_winner_investment = u16::MIN;
                for winner_idx in winner_indices {
                    let winner_seat_idx = seats_in_pot[winner_idx];
                    let (_, winner_investment) =
                        pot.investments.remove_entry(&winner_seat_idx).unwrap();
                    if winner_investment > max_winner_investment {
                        max_winner_investment = winner_investment;
                    }
                    money_per_winner.insert(winner_seat_idx, winner_investment);
                    pot.size -= winner_investment;
                }
                for (seat_idx, investment) in pot.investments.iter() {
                    if *investment > max_winner_investment {
                        let remainder = investment - max_winner_investment;
                        money_per_player.insert(*seat_idx, remainder);
                        pot.size -= remainder;
                    }
                }
                // Finally, split the remaining pot amongst all the winners.
                let split = pot.size / num_winners as u16;
                for (winner_seat_idx, money) in money_per_winner.drain() {
                    money_per_player.insert(winner_seat_idx, money + split);
                }
            }
        }

        // We have to keep doing showdowns so long as there's another pot
        // where the winners need to be determined.
        if !self.pots.is_empty() {
            self.next_state = GameState::Showdown;
        } else {
            self.next_state = GameState::RemovePlayers;
        }
        Ok(ShowdownData {
            next_state: self.next_state,
            money_per_player,
        })
    }

    pub fn turn(&mut self) -> Result<ActionData, GameError> {
        if self.next_state != GameState::Turn {
            return Err(GameError::StateTransition);
        }
        self.board.push(self.deck[self.deck_idx]);
        self.deck_idx += 1;
        self.next_state = GameState::TakeAction;
        Ok(ActionData {
            next_state: self.next_state,
            next_action_idx: self.next_action_idx,
            board: self.board.clone(),
        })
    }

    pub fn update_blinds(&mut self) -> Result<GameState, GameError> {
        if self.next_state != GameState::UpdateBlinds {
            return Err(GameError::StateTransition);
        }
        let mut min_money = u16::MAX;
        for (_, user) in self.users.iter() {
            if user.money < min_money {
                min_money = user.money;
            }
        }
        if min_money < u16::MAX && min_money > (2 * self.big_blind) {
            self.small_blind *= 2;
            self.big_blind *= 2;
        }
        self.next_state = GameState::BootPlayers;
        Ok(self.next_state)
    }
}
