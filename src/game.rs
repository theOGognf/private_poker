use crate::poker;

use rand::seq::SliceRandom;
use rand::thread_rng;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use thiserror::Error;

// Don't want too many people waiting to play the game.
const MAX_PLAYERS: usize = 12;
const MAX_USERS: usize = MAX_PLAYERS + 6;
// In the wild case that players have monotonically increasing
// stacks and they all go all-in.
const MAX_POTS: usize = MAX_PLAYERS / 3;
// Technically a hand can only consist of 7 cards, but we treat aces
// as two separate cards (1u8 and 14u8).
const MAX_CARDS: usize = 11;
// A player will be cleaned if they fold 20 rounds with the big blind.
const STARTING_STACK: u16 = 200;
const MIN_BIG_BLIND: u16 = STARTING_STACK / 20;
const MIN_SMALL_BLIND: u16 = MIN_BIG_BLIND / 2;

#[derive(Debug, Eq, PartialEq)]
enum UserState {
    Spectating,
    Playing,
    Waiting,
}

struct User {
    money: u16,
    state: UserState,
}

impl User {
    fn new() -> User {
        User {
            money: STARTING_STACK,
            state: UserState::Spectating,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Action {
    AllIn,
    Call,
    Check,
    Fold,
    Raise,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Action::AllIn => write!(f, "all-in"),
            Action::Call => write!(f, "call"),
            Action::Check => write!(f, "check"),
            Action::Fold => write!(f, "fold"),
            Action::Raise => write!(f, "raise"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Bet {
    action: Action,
    amount: u16,
}

#[derive(Debug, Eq, Error, PartialEq)]
enum UserError {
    #[error("User {username} already exists.")]
    AlreadyExists { username: String },
    #[error("Game is full.")]
    CapacityReached,
    #[error("User {username} does not exist.")]
    DoesNotExist { username: String },
    #[error("User {username} does not have the funds to satisfy the ${big_blind} big blind.")]
    InsufficientFunds { username: String, big_blind: u16 },
    #[error(
        "Seat {seat_idx} tried to {} but their ${} bet didn't satisfy that action.", .bet.action, .bet.amount
    )]
    InvalidBet { seat_idx: usize, bet: Bet },
}

#[derive(Clone, Copy, PartialEq)]
enum SidePotState {
    AllIn,
    Raise,
    CallOrReraise,
}

/// For users that're in a pot.
#[derive(PartialEq)]
enum PlayerState {
    // Player is in the pot but is waiting for their move.
    Wait,
    // Player put in their whole stack.
    AllIn,
    // Player forfeited their stack for the pot.
    Fold,
}

struct Player {
    name: String,
    state: PlayerState,
    cards: Vec<poker::Card>,
}

impl Player {
    fn new(name: &str) -> Player {
        Player {
            name: name.to_string(),
            state: PlayerState::Wait,
            cards: Vec::with_capacity(MAX_CARDS),
        }
    }

    fn reset(&mut self) {
        self.state = PlayerState::Wait;
        self.cards.clear();
    }
}

struct Pot {
    // The total investment for each player to remain in the hand.
    call: u16,
    // Size is just the sum of all investments in the pot.
    size: u16,
    // Map seat indices (players) to their investment in the pot.
    investments: HashMap<usize, u16>,
    // Used to check whether to spawn a side pot from this pot.
    // Should be `None` if no side pot conditions are met.
    side_pot_state: Option<SidePotState>,
}

impl Pot {
    fn bet(&mut self, seat_idx: usize, bet: &Bet) -> Result<Option<Pot>, UserError> {
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
                    return Err(UserError::InvalidBet {
                        seat_idx,
                        bet: *bet,
                    });
                }
            }
            Action::Raise => {
                if new_investment < (2 * self.call) {
                    return Err(UserError::InvalidBet {
                        seat_idx,
                        bet: *bet,
                    });
                }
                new_call = new_investment;
            }
            Action::AllIn => {
                if new_investment > self.call {
                    new_call = new_investment;
                }
            }
            // A bet must call, raise, or all-in.
            _ => {
                return Err(UserError::InvalidBet {
                    seat_idx,
                    bet: *bet,
                })
            }
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
                        // The original bet for the new pot is always considered
                        // a raise.
                        &Bet {
                            action: Action::Raise,
                            // The call excess starts the call for the new pot.
                            amount: new_investment - self.call,
                        },
                    )?;
                    // The call for the pot hasn't change.
                    new_call = self.call;
                    // The pot increase is just the pot's call remaining for the player.
                    pot_increase = self.call - *investment;
                    // The player has now matched the call for the pot.
                    new_investment = self.call;
                }
            }
            _ => (),
        }
        // Finally, update the call, the pot, and the player's investment
        // in the current pot.
        self.call = new_call;
        self.size += pot_increase;
        *investment = new_investment;
        Ok(side_pot)
    }

    /// Return the amount the player must bet to remain in the hand, and
    /// the minimum the player must raise by for it to be considered
    /// a valid raise.
    fn get_next_call(&mut self, seat_idx: usize) -> u16 {
        self.call - *self.investments.entry(seat_idx).or_default()
    }

    fn new() -> Pot {
        Pot {
            call: 0,
            size: 0,
            investments: HashMap::with_capacity(MAX_PLAYERS),
            side_pot_state: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd)]
enum GameState {
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

struct ActionData {
    next_state: GameState,
    next_action_idx: Option<usize>,
    board: Vec<poker::Card>,
    options: HashSet<Action>,
}

struct ShowdownData {
    next_state: GameState,
    // Map seat indices to winnings.
    money_per_player: HashMap<usize, u16>,
}

struct GameData {
    deck: [poker::Card; 52],
    donations: u16,
    small_blind: u16,
    big_blind: u16,
    users: HashMap<String, User>,
    spectators: HashSet<String>,
    waitlist: VecDeque<String>,
    seats: [Option<Player>; MAX_PLAYERS],
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

impl GameData {
    fn new() -> Self {
        Self {
            deck: poker::new_deck(),
            donations: 0,
            small_blind: MIN_SMALL_BLIND,
            big_blind: MIN_BIG_BLIND,
            users: HashMap::with_capacity(MAX_USERS),
            spectators: HashSet::with_capacity(MAX_USERS),
            waitlist: VecDeque::with_capacity(MAX_USERS),
            seats: [const { None }; MAX_PLAYERS],
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

struct SeatPlayers {}
struct MoveButton {}
struct CollectBlinds {}
struct Deal {}
struct TakeAction {
    action_options: Option<HashSet<Action>>,
}
struct Flop {}
struct Turn {}
struct River {}
struct Showdown {}
struct RemovePlayers {}
struct DivideDonations {}
struct UpdateBlinds {}
struct BootPlayers {}

/// A poker game.
struct Game<T> {
    data: GameData,
    state: T,
}

/// General game methods.
impl<T> Game<T> {
    fn is_ready_for_showdown(&self) -> bool {
        let mut num_players_remaining: usize = 0;
        let mut num_all_in: usize = 0;
        for seat in self.data.seats.iter() {
            if seat.is_some() {
                let player = seat.as_ref().unwrap();
                match player.state {
                    PlayerState::AllIn => {
                        num_players_remaining += 1;
                        num_all_in += 1;
                    }
                    PlayerState::Wait => num_players_remaining += 1,
                    _ => (),
                }
            }
        }
        // If no one else is left to make a move, then proceed to the showdown.
        num_players_remaining == 1 || num_all_in >= num_players_remaining - 1
    }

    fn new() -> Game<SeatPlayers> {
        Game {
            data: GameData::new(),
            state: SeatPlayers {},
        }
    }

    fn new_user(&mut self, username: &str) -> Result<usize, UserError> {
        if self.data.users.len() == MAX_USERS {
            return Err(UserError::CapacityReached);
        } else if self.data.users.contains_key(username) {
            // Check if player already exists but is queued for removal.
            // This probably means the user disconnected and are trying
            // to reconnect.
            if !self.data.players_to_remove.remove(username) {
                return Err(UserError::AlreadyExists {
                    username: username.to_string(),
                });
            } else {
                return Ok(self.data.users.len());
            }
        }
        self.data.users.insert(username.to_string(), User::new());
        self.data.spectators.insert(username.to_string());
        Ok(self.data.users.len())
    }

    fn waitlist_user(&mut self, username: &str) -> Result<bool, UserError> {
        if let Some(user) = self.data.users.get_mut(username) {
            // Need to remove the player from the removal and spectate sets just in
            // case they wanted to do one of those, but then changed their mind and
            // want to play again.
            self.data.players_to_spectate.remove(username);
            self.data.players_to_remove.remove(username);
            match user.state {
                // The user is already playing, so we don't need to do anything,
                // but we should acknowledge that the user still isn't
                // technically waitlisted.
                UserState::Playing => Ok(false),
                UserState::Spectating => {
                    if user.money < self.data.big_blind {
                        return Err(UserError::InsufficientFunds {
                            username: username.to_string(),
                            big_blind: self.data.big_blind,
                        });
                    }
                    self.data.spectators.remove(username);
                    self.data.waitlist.push_back(username.to_string());
                    user.state = UserState::Waiting;
                    Ok(true)
                }
                // The user is already waitlisted, so we can just quietly
                // say that they're waitlisted.
                UserState::Waiting => Ok(true),
            }
        } else {
            Err(UserError::DoesNotExist {
                username: username.to_string(),
            })
        }
    }
}

macro_rules! impl_user_managers {
    ($($t:ty),+) => {
        $(impl $t {
            fn remove_user(&mut self, username: &str) -> Result<bool, UserError> {
                if let Some(mut user) = self.data.users.remove(username) {
                    match user.state {
                        UserState::Playing => {
                            // Need to remove the player from other queues just in
                            // case they changed their mind.
                            self.data.players_to_spectate.remove(username);
                            self.spectate_user(username).unwrap();
                            self.data.spectators.remove(username);
                        }
                        UserState::Spectating => {
                            self.data.spectators.remove(username);
                        }
                        UserState::Waiting => {
                            // We can remove the user from the waitlist anytime we want.
                            let waitlist_idx = self.data.waitlist.iter().position(|u| u == username).unwrap();
                            self.data.waitlist.remove(waitlist_idx);
                        }
                    }
                    self.data.donations += user.money;
                    user.money = 0;
                    self.data.users.remove(username);
                    Ok(true)
                } else {
                    Err(UserError::DoesNotExist{username: username.to_string()})
                }
            }

            fn spectate_user(&mut self, username: &str) -> Result<bool, UserError> {
                if let Some(user) = self.data.users.get_mut(username) {
                    // The player has already been queued for spectate. Just wait for
                    // the next spectate phase.
                    if self.data.players_to_spectate.contains(username) {
                        return Ok(false);
                    }
                    match user.state {
                        UserState::Playing => {
                            // Need to remove the player from other queues just in
                            // case they changed their mind.
                            self.data.players_to_remove.remove(username);
                            let seat_idx = self
                                .data.seats
                                .iter()
                                .position(|o| o.as_ref().is_some_and(|p| p.name == username))
                                .unwrap();
                            self.data.seats[seat_idx] = None;
                            self.data.num_players -= 1;
                        }
                        // The user is already spectating, so we can just quietly
                        // say that they're spectating.
                        UserState::Spectating => return Ok(true),
                        UserState::Waiting => {
                            let waitlist_idx = self.data.waitlist.iter().position(|u| u == username).unwrap();
                            self.data.waitlist.remove(waitlist_idx);
                        }
                    }
                    self.data.spectators.insert(username.to_string());
                    user.state = UserState::Spectating;
                    Ok(true)
                } else {
                    Err(UserError::DoesNotExist{username: username.to_string()})
                }
            }
        })*
    }
}

macro_rules! impl_user_managers_with_queue {
    ($($t:ty),+) => {
        $(impl $t {
            fn remove_user(&mut self, username: &str) -> Result<bool, UserError> {
                if let Some(user) = self.data.users.get_mut(username) {
                    // The player has already been queued for removal. Just wait for
                    // the next removal phase.
                    if self.data.players_to_remove.contains(username) {
                        return Ok(false);
                    }
                    match user.state {
                        UserState::Playing => {
                            // Need to remove the player from other queues just in
                            // case they changed their mind.
                            self.data.players_to_spectate.remove(username);
                            // The player is still at the table while the game is ongoing.
                            // We don't want to disrupt gameplay, so we just queue the
                            // player for removal and remove them later.
                            self.data.players_to_remove.insert(username.to_string());
                            return Ok(false);
                        }
                        UserState::Spectating => {
                            self.data.spectators.remove(username);
                        }
                        UserState::Waiting => {
                            // We can remove the user from the waitlist anytime we want.
                            let waitlist_idx = self.data.waitlist.iter().position(|u| u == username).unwrap();
                            self.data.waitlist.remove(waitlist_idx);
                        }
                    }
                    self.data.donations += user.money;
                    user.money = 0;
                    self.data.users.remove(username);
                    Ok(true)
                } else {
                    Err(UserError::DoesNotExist{username: username.to_string()})
                }
            }

            fn spectate_user(&mut self, username: &str) -> Result<bool, UserError> {
                if let Some(user) = self.data.users.get_mut(username) {
                    // The player has already been queued for spectate. Just wait for
                    // the next spectate phase.
                    if self.data.players_to_spectate.contains(username) {
                        return Ok(false);
                    }
                    match user.state {
                        UserState::Playing => {
                            // Need to remove the player from other queues just in
                            // case they changed their mind.
                            self.data.players_to_remove.remove(username);
                            self.data.players_to_spectate.insert(username.to_string());
                            return Ok(false);
                        }
                        // The user is already spectating, so we can just quietly
                        // say that they're spectating.
                        UserState::Spectating => return Ok(true),
                        UserState::Waiting => {
                            let waitlist_idx = self.data.waitlist.iter().position(|u| u == username).unwrap();
                            self.data.waitlist.remove(waitlist_idx);
                        }
                    }
                    self.data.spectators.insert(username.to_string());
                    user.state = UserState::Spectating;
                    Ok(true)
                } else {
                    Err(UserError::DoesNotExist{username: username.to_string()})
                }
            }
        })*
    }
}

impl_user_managers!(
    Game<SeatPlayers>,
    Game<DivideDonations>,
    Game<UpdateBlinds>,
    Game<BootPlayers>
);

impl_user_managers_with_queue!(
    Game<MoveButton>,
    Game<CollectBlinds>,
    Game<Deal>,
    Game<TakeAction>,
    Game<Flop>,
    Game<Turn>,
    Game<River>,
    Game<Showdown>,
    // There's an edge case where a player can queue for removal
    // when the game is in the `RemovePlayers` state, but before
    // the transition to the `DivideDonations` state.
    Game<RemovePlayers>
);

impl From<Game<SeatPlayers>> for Game<MoveButton> {
    fn from(mut value: Game<SeatPlayers>) -> Self {
        let mut i: usize = 0;
        while value.data.num_players < MAX_PLAYERS && !value.data.waitlist.is_empty() {
            if value.data.seats[i].is_none() {
                let username = value.data.waitlist.pop_front().unwrap();
                let user = value.data.users.get_mut(&username).unwrap();
                if user.money < value.data.big_blind {
                    value.spectate_user(&username).unwrap();
                } else {
                    value.data.seats[i] = Some(Player::new(&username));
                    user.state = UserState::Playing;
                    value.data.num_players += 1;
                }
            }
            i += 1;
        }
        Self {
            data: value.data,
            state: MoveButton {},
        }
    }
}

/// Move the blinds and next action indices, preparing the next game
/// by determining who will be paying blinds and who will be making
/// the first action.
impl From<Game<MoveButton>> for Game<CollectBlinds> {
    fn from(mut value: Game<MoveButton>) -> Self {
        // Search for the big blind and starting positions.
        let mut table = value.data.seats.iter().cycle();
        table.nth(value.data.big_blind_idx + 1);
        value.data.big_blind_idx = table.position(|p| p.is_some()).unwrap();
        value.data.prev_raise_idx = value.data.big_blind_idx;
        value.data.next_action_idx = Some(table.position(|p| p.is_some()).unwrap());
        // Reverse the table search to find the small blind position relative
        // to the big blind position since the small blind must always trail the big
        // blind.
        let mut table = value.data.seats.iter();
        table.nth(value.data.big_blind_idx);
        let mut reverse_table = table.rev().cycle();
        value.data.small_blind_idx = reverse_table.position(|p| p.is_some()).unwrap();
        Self {
            data: value.data,
            state: CollectBlinds {},
        }
    }
}

/// Collect blinds, initializing the main pot.
impl From<Game<CollectBlinds>> for Game<Deal> {
    fn from(mut value: Game<CollectBlinds>) -> Self {
        value.data.pots.clear();
        value.data.pots.push(Pot::new());
        let pot = &mut value.data.pots[0];
        for (seat_idx, blind) in [
            (value.data.small_blind_idx, value.data.small_blind),
            (value.data.big_blind_idx, value.data.big_blind),
        ] {
            let player = value.data.seats[seat_idx].as_ref().unwrap();
            let user = value.data.users.get_mut(&player.name).unwrap();
            let action: Action;
            if user.money > blind {
                action = Action::Raise;
            } else {
                action = Action::AllIn;
            }
            pot.bet(
                seat_idx,
                &Bet {
                    action: action,
                    amount: blind,
                },
            )
            .unwrap();
            user.money -= blind;
        }
        Self {
            data: value.data,
            state: Deal {},
        }
    }
}

/// Shuffle the game's deck and deal 2 cards to each player.
impl From<Game<Deal>> for Game<TakeAction> {
    fn from(mut value: Game<Deal>) -> Self {
        value.data.deck.shuffle(&mut thread_rng());
        value.data.deck_idx = 0;

        let mut table = (0..MAX_PLAYERS).cycle().skip(value.data.small_blind_idx);
        // Deal 2 cards per player, looping over players and dealing them 1 card
        // at a time.
        while value.data.deck_idx < (2 * value.data.num_players) {
            let deal_idx = table.find(|&idx| value.data.seats[idx].is_some()).unwrap();
            let player = value.data.seats[deal_idx].as_mut().unwrap();
            player.cards.push(value.data.deck[value.data.deck_idx]);
            value.data.deck_idx += 1;
        }
        // The only option unavailable after dealing is checking.
        Self {
            data: value.data,
            state: TakeAction {
                action_options: Some(HashSet::from([
                    Action::AllIn,
                    Action::Call,
                    Action::Fold,
                    Action::Raise,
                ])),
            },
        }
    }
}

impl Game<Flop> {
    fn step(&mut self) {
        for _ in 0..3 {
            self.data.board.push(self.data.deck[self.data.deck_idx]);
            self.data.deck_idx += 1;
        }
    }
}

/// Put the first 3 cards on the board.
impl From<Game<Flop>> for Game<TakeAction> {
    fn from(mut value: Game<Flop>) -> Self {
        value.step();
        // Assumes the next player has already been determined and they haven't
        // gone all-in or folded yet.
        Self {
            data: value.data,
            state: TakeAction {
                action_options: Some(HashSet::from([Action::Check, Action::Fold, Action::Raise])),
            },
        }
    }
}

/// Put the first 3 cards on the board assuming the game is ready for
/// showdown.
impl From<Game<Flop>> for Game<Turn> {
    fn from(mut value: Game<Flop>) -> Self {
        value.step();
        Self {
            data: value.data,
            state: Turn {},
        }
    }
}

impl Game<Turn> {
    fn step(&mut self) {
        self.data.board.push(self.data.deck[self.data.deck_idx]);
        self.data.deck_idx += 1;
    }
}

/// Put the 4th card on the board.
impl From<Game<Turn>> for Game<TakeAction> {
    fn from(mut value: Game<Turn>) -> Self {
        value.step();
        // Assumes the next player has already been determined and they haven't
        // gone all-in or folded yet.
        Self {
            data: value.data,
            state: TakeAction {
                action_options: Some(HashSet::from([Action::Check, Action::Fold, Action::Raise])),
            },
        }
    }
}

/// Put the 4th card on the board assuming the game is ready for
/// showdown.
impl From<Game<Turn>> for Game<River> {
    fn from(mut value: Game<Turn>) -> Self {
        value.step();
        Self {
            data: value.data,
            state: River {},
        }
    }
}

impl Game<River> {
    fn step(&mut self) {
        self.data.board.push(self.data.deck[self.data.deck_idx]);
        self.data.deck_idx += 1;
    }
}

/// Put the 5th card on the board.
impl From<Game<River>> for Game<TakeAction> {
    fn from(mut value: Game<River>) -> Self {
        value.step();
        // Assumes the next player has already been determined and they haven't
        // gone all-in or folded yet.
        Self {
            data: value.data,
            state: TakeAction {
                action_options: Some(HashSet::from([Action::Check, Action::Fold, Action::Raise])),
            },
        }
    }
}

/// Put the 5th card on the board assuming the game is ready for
/// showdown.
impl From<Game<River>> for Game<Showdown> {
    fn from(mut value: Game<River>) -> Self {
        value.step();
        Self {
            data: value.data,
            state: Showdown {},
        }
    }
}

impl Game<Showdown> {
    /// Get all players in the pot that haven't folded and compare their
    /// hands to one another. Get the winning indices and distribute
    /// the pot accordingly. If there's a tie, winners are given their
    /// original investments and then split the remainder. Everyone
    /// can only lose as much as they had originally invested or as much
    /// as a winner had invested, whichever is lower. This prevents folks
    /// that went all-in, but have much more money than the winner, from
    /// losing the extra money.
    fn distribute(&mut self) -> bool {
        if let Some(mut pot) = self.data.pots.pop() {
            let mut seats_in_pot = Vec::with_capacity(MAX_PLAYERS);
            let mut hands_in_pot = Vec::with_capacity(MAX_PLAYERS);
            for (seat_idx, _) in pot.investments.iter() {
                let player = self.data.seats[*seat_idx].as_mut().unwrap();
                if player.state != PlayerState::Fold {
                    seats_in_pot.push(*seat_idx);
                    player.cards.sort_unstable();
                    hands_in_pot.push(poker::eval(&player.cards));
                }
            }

            // Only up to 4 players can split the pot (only four suits per card value).
            let mut distributions_per_player: HashMap<usize, u16> = HashMap::with_capacity(4);
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
                    for (seat_idx, investment) in pot.investments {
                        if investment > winner_investment {
                            let remainder = investment - winner_investment;
                            distributions_per_player.insert(seat_idx, remainder);
                            pot.size -= remainder;
                        }
                    }
                    distributions_per_player.insert(winner_seat_idx, pot.size);
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
                    for (seat_idx, investment) in pot.investments {
                        if investment > max_winner_investment {
                            let remainder = investment - max_winner_investment;
                            distributions_per_player.insert(seat_idx, remainder);
                            pot.size -= remainder;
                        }
                    }
                    // Finally, split the remaining pot amongst all the winners.
                    let split = pot.size / num_winners as u16;
                    for (winner_seat_idx, money) in money_per_winner {
                        distributions_per_player.insert(winner_seat_idx, money + split);
                    }
                }
            }

            // Give money back to players.
            for (seat_idx, distribution) in distributions_per_player {
                let player = self.data.seats[seat_idx].as_ref().unwrap();
                let user = self.data.users.get_mut(&player.name).unwrap();
                user.money += distribution;
            }

            // We have no data to return, but we still want to signal that
            // something did happen.
            true
        } else {
            false
        }
    }
}

impl From<Game<Showdown>> for Game<RemovePlayers> {
    fn from(value: Game<Showdown>) -> Self {
        Self {
            data: value.data,
            state: RemovePlayers {},
        }
    }
}

impl From<Game<RemovePlayers>> for Game<DivideDonations> {
    fn from(mut value: Game<RemovePlayers>) -> Self {
        while let Some(username) = value.data.players_to_remove.pop_first() {
            value.remove_user(&username).unwrap();
        }
        Self {
            data: value.data,
            state: DivideDonations {},
        }
    }
}

/// Empty the community donations pot and split it equally amongst
/// all users. The community donations pot is filled with money from
/// users that left the game. Redistributing the money back to remaining
/// users helps keep games going. It especially helps to continue
/// gameplay if a user aggregates most of the money and then leaves.
/// Rather than taking their money with them, their money is distributed
/// to all the poor folks so they can keep playing and don't have to
/// create a new game.
impl From<Game<DivideDonations>> for Game<UpdateBlinds> {
    fn from(mut value: Game<DivideDonations>) -> Self {
        let num_users = value.data.users.len();
        if num_users > 0 {
            let donation_per_user = value.data.donations / num_users as u16;
            for (_, user) in value.data.users.iter_mut() {
                user.money += donation_per_user;
            }
            value.data.donations = 0;
        }
        Self {
            data: value.data,
            state: UpdateBlinds {},
        }
    }
}

/// Update the blinds, checking if the minimum stack size for all users
/// is larger than twice the blind. If it is, blinds are doubled. This
/// helps progress the game, increasing the investment each player must
/// make in each hand. This prevents longer games where a handful of
/// players have large stacks and can afford to fold many times without
/// any action.
impl From<Game<UpdateBlinds>> for Game<BootPlayers> {
    fn from(mut value: Game<UpdateBlinds>) -> Self {
        let mut min_money = u16::MAX;
        for (_, user) in value.data.users.iter() {
            if user.money < min_money {
                min_money = user.money;
            }
        }
        if min_money < u16::MAX && min_money > (2 * value.data.big_blind) {
            value.data.small_blind *= 2;
            value.data.big_blind *= 2;
        }
        Self {
            data: value.data,
            state: BootPlayers {},
        }
    }
}

impl From<Game<BootPlayers>> for Game<SeatPlayers> {
    fn from(mut value: Game<BootPlayers>) -> Self {
        for seat in value.data.seats.iter_mut() {
            if seat.is_some() {
                let player = seat.as_mut().unwrap();
                let user = value.data.users.get(&player.name).unwrap();
                if user.money < value.data.big_blind {
                    value.data.players_to_spectate.insert(player.name.clone());
                } else {
                    player.reset();
                }
            }
        }
        while let Some(username) = value.data.players_to_spectate.pop_first() {
            value.spectate_user(&username).unwrap();
        }
        Self {
            data: value.data,
            state: SeatPlayers {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Game, SeatPlayers, UserError, UserState, MAX_USERS};

    #[test]
    fn manipulate_user_in_lobby() {
        let mut game = Game::<SeatPlayers>::new();
        let username = "ognf";

        // Add new user, make sure they exist and are spectating.
        game.new_user(username).unwrap();
        assert!(game.data.users.contains_key(username));
        assert!(game.data.spectators.contains(username));
        assert_eq!(
            game.data.users.get(username).unwrap().state,
            UserState::Spectating
        );

        // Make sure we can't add another user of the same name.
        assert_eq!(
            game.new_user(username),
            Err(UserError::AlreadyExists {
                username: username.to_string()
            })
        );

        // Try some user state transitions.
        // Waitlisting.
        game.waitlist_user(username).unwrap();
        assert!(game.data.waitlist.contains(&username.to_string()));
        assert_eq!(
            game.data.users.get(username).unwrap().state,
            UserState::Waiting
        );

        // Back to spectating.
        game.spectate_user(username).unwrap();
        assert!(game.data.spectators.contains(username));
        assert_eq!(
            game.data.users.get(username).unwrap().state,
            UserState::Spectating
        );

        // Remove them.
        game.remove_user(username).unwrap();
        assert!(!game.data.users.contains_key(username));
        assert!(!game.data.spectators.contains(username));

        // Try to do stuff when they don't exist.
        assert_eq!(
            game.remove_user(username),
            Err(UserError::DoesNotExist {
                username: username.to_string()
            })
        );
        assert_eq!(
            game.waitlist_user(username),
            Err(UserError::DoesNotExist {
                username: username.to_string()
            })
        );
        assert_eq!(
            game.spectate_user(username),
            Err(UserError::DoesNotExist {
                username: username.to_string()
            })
        );

        // Add them again.
        game.new_user(username).unwrap();
        assert!(game.data.users.contains_key(username));
        assert!(game.data.spectators.contains(username));

        // Waitlist them again.
        game.waitlist_user(username).unwrap();
        assert!(game.data.waitlist.contains(&username.to_string()));
        assert_eq!(
            game.data.users.get(username).unwrap().state,
            UserState::Waiting
        );

        // Remove them again.
        game.remove_user(username).unwrap();
        assert!(!game.data.users.contains_key(username));
        assert!(!game.data.waitlist.contains(&username.to_string()));

        // Finally, add a bunch of users until capacity is reached.
        for i in 0..MAX_USERS {
            game.new_user(&i.to_string()).unwrap();
        }
        // The game should now be full.
        assert_eq!(game.new_user(username), Err(UserError::CapacityReached));
    }
}
