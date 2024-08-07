use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::cmp::{max, Ordering};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use thiserror::Error;

use super::{
    constants::{MAX_PLAYERS, MAX_POTS, MAX_USERS},
    entities::{
        Action, Bet, BetAction, Card, Player, PlayerState, Pot, SubHand, Usd, Usdf, User,
        UserState, MIN_BIG_BLIND, MIN_SMALL_BLIND, STARTING_STACK,
    },
    functional,
};

#[derive(Debug, Deserialize, Eq, Error, PartialEq, Serialize)]
pub enum UserError {
    #[error("Cannot show hand now.")]
    CannotShowHand,
    #[error("Game is full.")]
    CapacityReached,
    #[error("Game already in progress.")]
    GameAlreadyInProgress,
    #[error("Insufficient funds to satisfy the ${big_blind} big blind.")]
    InsufficientFunds { big_blind: Usd },
    #[error("Tried an illegal {action}.")]
    InvalidAction { action: Action },
    #[error("Tried an illegal {bet}.")]
    InvalidBet { bet: Bet },
    #[error("Need at least 2 players to start the game.")]
    NotEnoughPlayers,
    #[error("Tried acting out of turn.")]
    OutOfTurnAction,
    #[error("Username already exists.")]
    UserAlreadyExists,
    #[error("Username does not exist.")]
    UserDoesNotExist,
    #[error("User is not playing.")]
    UserNotPlaying,
    #[error("User already showing hand.")]
    UserAlreadyShowingHand,
}

#[derive(Debug)]
pub struct GameData {
    /// Deck of cards. This is instantiated once and reshuffled
    /// each deal.
    deck: [Card; 52],
    /// Money from users that've left the game. This money is
    /// split equally amongst all users at a particular game state.
    /// This helps keep the amount of money in the game constant,
    /// encouraging additional gameplay.
    pub donations: Usdf,
    pub small_blind: Usd,
    pub big_blind: Usd,
    pub users: HashMap<String, User>,
    pub spectators: HashSet<String>,
    pub waitlist: VecDeque<String>,
    pub seats: [Option<Player>; MAX_PLAYERS],
    /// Community cards shared amongst all players.
    pub board: Vec<Card>,
    /// Count of the number of players seated within `seats`.
    /// Helps refrain from overfilling the seats when players
    /// are seated.
    num_players: usize,
    /// Count of the number of players active in a hand.
    /// All-in and folding are considered INACTIVE since they
    /// have no more moves to make. Once `num_players_called`
    /// is equal to this value, the round of betting is concluded.
    num_players_active: usize,
    /// Count of the number of players that have matched the minimum
    /// call. Coupled with `num_players_active`, this helps track
    /// whether a round of betting has ended. This value is reset
    /// at the beginning of each betting round and whenever a player
    /// raises (since they've increased the minimum call).
    num_players_called: usize,
    /// All pots used in the current hand. A side pot is created
    /// and pushed to this vector anytime a player raises an all-in.
    /// The call a player must make is the sum of all calls from all
    /// pots within this vector.
    pub pots: Vec<Pot>,
    /// Queue of users that're playing the game but have opted
    /// to spectate. We can't safely remove them from the game mid gameplay,
    /// so we instead queue them for removal.
    players_to_spectate: BTreeSet<String>,
    /// Queue of users that're playing the game but have opted
    /// to leave. We can't safely remove them from the game mid gameplay,
    /// so we instead queue them for removal.
    players_to_remove: BTreeSet<String>,
    deck_idx: usize,
    pub small_blind_idx: usize,
    pub big_blind_idx: usize,
    starting_action_idx: usize,
    pub next_action_idx: Option<usize>,
}

impl GameData {
    fn new() -> Self {
        Self {
            deck: functional::new_deck(),
            donations: 0.0,
            small_blind: MIN_SMALL_BLIND,
            big_blind: MIN_BIG_BLIND,
            users: HashMap::with_capacity(MAX_USERS),
            spectators: HashSet::with_capacity(MAX_USERS),
            waitlist: VecDeque::with_capacity(MAX_USERS),
            seats: [const { None }; MAX_PLAYERS],
            board: Vec::with_capacity(5),
            num_players: 0,
            num_players_active: 0,
            num_players_called: 0,
            pots: Vec::with_capacity(MAX_POTS),
            players_to_remove: BTreeSet::new(),
            players_to_spectate: BTreeSet::new(),
            deck_idx: 0,
            small_blind_idx: 0,
            big_blind_idx: 1,
            starting_action_idx: 2,
            next_action_idx: None,
        }
    }
}

#[derive(Debug)]
pub struct Lobby {
    start_game: bool,
}

impl Lobby {
    pub fn new() -> Self {
        Self { start_game: false }
    }
}

#[derive(Debug)]
pub struct SeatPlayers {}

#[derive(Debug)]
pub struct MoveButton {}

#[derive(Debug)]
pub struct CollectBlinds {}

#[derive(Debug)]
pub struct Deal {}

#[derive(Debug)]
pub struct TakeAction {
    pub action_options: Option<HashSet<Action>>,
}

#[derive(Debug)]
pub struct Flop {}

#[derive(Debug)]
pub struct Turn {}

#[derive(Debug)]
pub struct River {}

#[derive(Debug)]
pub struct ShowHands {
    /// Temporarily maps player seats to poker hand evaluations so a player's
    /// hand doesn't have to be evaluated multiple times per game.
    hand_eval_cache: HashMap<usize, Vec<SubHand>>,
}

impl ShowHands {
    pub fn new() -> Self {
        ShowHands {
            hand_eval_cache: HashMap::with_capacity(MAX_PLAYERS),
        }
    }
}

#[derive(Debug)]
pub struct DistributePot {
    /// Temporarily maps player seats to poker hand evaluations so a player's
    /// hand doesn't have to be evaluated multiple times per game.
    hand_eval_cache: HashMap<usize, Vec<SubHand>>,
}

#[derive(Debug)]
pub struct RemovePlayers {}

#[derive(Debug)]
pub struct DivideDonations {}

#[derive(Debug)]
pub struct UpdateBlinds {}

#[derive(Debug)]
pub struct BootPlayers {}

/// A poker game.
#[derive(Debug)]
pub struct Game<T> {
    pub data: GameData,
    pub state: T,
}

/// General game methods.
impl<T> Game<T> {
    /// Return the index of the player who has the next action.
    fn get_next_action_idx(&self, new_phase: bool) -> Option<usize> {
        if self.is_end_of_round() {
            return None;
        }
        match self.data.next_action_idx {
            Some(action_idx) => self
                .data
                .seats
                .iter()
                .enumerate()
                .cycle()
                .skip(action_idx + !new_phase as usize)
                .find(|(_, s)| s.as_ref().is_some_and(|p| p.state == PlayerState::Wait))
                .map(|(next_action_idx, _)| next_action_idx),
            None => None,
        }
    }

    /// Return the set of possible actions the next player can
    /// make.
    fn get_next_action_options(&self) -> Option<HashSet<Action>> {
        if self.is_ready_for_next_phase() {
            return None;
        }
        match self.data.next_action_idx {
            Some(action_idx) => {
                let mut action_options = HashSet::from([Action::AllIn, Action::Fold]);
                let player = self.data.seats[action_idx].as_ref().unwrap();
                let user = self.data.users.get(&player.name).unwrap();
                let raise = self.get_total_min_raise_by_seat(action_idx);
                let call = self.get_total_call_by_seat(action_idx);
                if call > 0 && call < user.money {
                    action_options.insert(Action::Call(call));
                } else if call == 0 {
                    action_options.insert(Action::Check);
                }
                if user.money > raise {
                    action_options.insert(Action::Raise(raise));
                }
                Some(action_options)
            }
            None => None,
        }
    }

    pub fn get_next_action_username(&self) -> Option<String> {
        match self.data.next_action_idx {
            Some(action_idx) => self.data.seats[action_idx]
                .as_ref()
                .map(|player| player.name.clone()),
            None => None,
        }
    }

    /// Return the number of cards that've been dealt.
    pub fn get_num_community_cards(&self) -> usize {
        self.data.board.len()
    }

    /// Return the number of players.
    pub fn get_num_players(&self) -> usize {
        self.data.num_players
    }

    /// Return the number of players.
    pub fn get_num_potential_players(&self) -> usize {
        self.data.num_players + self.data.waitlist.len()
    }

    /// Return the sum of all calls for all pots. A player's total investment
    /// must match this amount in order to stay in the pot(s).
    fn get_total_call(&self) -> Usd {
        self.data.pots.iter().map(|p| p.call).sum()
    }

    /// Return the remaining amount a player has to bet in order to stay
    /// in the pot(s).
    fn get_total_call_by_seat(&self, seat_idx: usize) -> Usd {
        self.data
            .pots
            .iter()
            .map(|p| p.get_call_by_seat(seat_idx))
            .sum()
    }

    /// Return the total amount a player has invested in the pot(s).
    fn get_total_investment_by_seat(&self, seat_idx: usize) -> Usd {
        self.data
            .pots
            .iter()
            .map(|p| p.get_investment_by_seat(seat_idx))
            .sum()
    }

    /// Return the minimum amount a player has to bet in order for their
    /// raise to be considered a valid raise.
    fn get_total_min_raise_by_seat(&self, seat_idx: usize) -> Usd {
        2 * self.get_total_call() - self.get_total_investment_by_seat(seat_idx)
    }

    /// Return whether the game is ready to move onto the next phase
    /// now that the betting round is over.
    fn is_end_of_round(&self) -> bool {
        self.data.num_players_active == self.data.num_players_called
    }

    pub fn is_pot_empty(&self) -> bool {
        self.data.pots.is_empty()
    }

    /// Return whether the betting round is over and the game can continue
    /// to the next phase. Used to help signal state transitions.
    pub fn is_ready_for_next_phase(&self) -> bool {
        self.is_end_of_round() || self.is_ready_for_showdown()
    }

    /// Return whether the game is ready to evaluate all the hands
    /// remaining in the pot. Used to help signal state transitions.
    pub fn is_ready_for_showdown(&self) -> bool {
        match self.data.next_action_idx {
            Some(action_idx) => {
                self.data.num_players_active <= 1 && self.get_total_call_by_seat(action_idx) == 0
            }
            None => self.data.num_players_active <= 1,
        }
    }

    pub fn is_turn(&self, username: &str) -> bool {
        match self.data.next_action_idx {
            Some(action_idx) => self.data.seats[action_idx].as_ref().unwrap().name == username,
            None => false,
        }
    }

    pub fn new() -> Game<SeatPlayers> {
        Game {
            data: GameData::new(),
            state: SeatPlayers {},
        }
    }

    pub fn new_user(&mut self, username: &str) -> Result<bool, UserError> {
        if self.data.users.len() == MAX_USERS {
            return Err(UserError::CapacityReached);
        } else if self.data.users.contains_key(username) {
            // Check if player already exists but is queued for removal.
            // This probably means the user disconnected and is trying
            // to reconnect.
            if !self.data.players_to_remove.remove(username) {
                return Err(UserError::UserAlreadyExists);
            } else {
                return Ok(false);
            }
        }
        self.data.users.insert(username.to_string(), User::new());
        self.data.spectators.insert(username.to_string());
        Ok(true)
    }

    /// Reset the next action index and return the possible actions
    /// for that player. This should be called prior to each game phase
    /// in preparation for a new round of betting.
    fn prepare_for_next_phase(&mut self) -> Option<HashSet<Action>> {
        self.data.num_players_called = 0;
        self.data.next_action_idx = Some(self.data.starting_action_idx);
        self.data.next_action_idx = self.get_next_action_idx(true);
        self.get_next_action_options()
    }

    pub fn waitlist_user(&mut self, username: &str) -> Result<bool, UserError> {
        match self.data.users.get_mut(username) {
            Some(user) => {
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
            }
            None => Err(UserError::UserDoesNotExist),
        }
    }
}

macro_rules! impl_user_managers {
    ($($t:ty),+) => {
        $(impl $t {
            pub fn remove_user(&mut self, username: &str) -> Result<bool, UserError> {
                match self.data.users.remove(username) {
                    Some(mut user) => {
                        match user.state {
                            UserState::Playing => {
                                // Need to remove the player from other queues just in
                                // case they changed their mind.
                                self.data.players_to_spectate.remove(username);
                                let seat_idx = self
                                    .data.seats
                                    .iter()
                                    .position(|o| o.as_ref().is_some_and(|p| p.name == username))
                                    .unwrap();
                                self.data.seats[seat_idx] = None;
                                self.data.num_players -= 1;
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
                        self.data.donations += user.money as Usdf;
                        user.money = 0;
                        Ok(true)
                    },
                    None => Err(UserError::UserDoesNotExist),
                }
            }

            pub fn spectate_user(&mut self, username: &str) -> Result<bool, UserError> {
                match self.data.users.get_mut(username) {
                    Some(user) => {
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
                    },
                    None => Err(UserError::UserDoesNotExist),
                }
            }
        })*
    }
}

macro_rules! impl_user_managers_with_queue {
    ($($t:ty),+) => {
        $(impl $t {
            pub fn remove_user(&mut self, username: &str) -> Result<bool, UserError> {
                match self.data.users.get_mut(username) {
                    Some(user) =>  {
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
                        self.data.donations += user.money as Usdf;
                        user.money = 0;
                        self.data.users.remove(username);
                        Ok(true)
                    },
                    None => Err(UserError::UserDoesNotExist)
                }
            }

            pub fn spectate_user(&mut self, username: &str) -> Result<bool, UserError> {
                match self.data.users.get_mut(username) {
                    Some(user) => {
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
                    },
                    None => Err(UserError::UserDoesNotExist)
                }
            }
        })*
    }
}

impl_user_managers!(
    Game<Lobby>,
    Game<SeatPlayers>,
    // There's an edge case where a player can queue for removal
    // when the game is in the `RemovePlayers` state, but before
    // the transition to the `DivideDonations` state. That's why
    // the `RemovePlayers` state manages users with the queue-driven
    // methods.
    Game<RemovePlayers>,
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
    Game<ShowHands>,
    Game<DistributePot>
);

impl Game<Lobby> {
    pub fn init_game_start(&mut self) -> Result<(), UserError> {
        if self.get_num_potential_players() >= 2 {
            self.state.start_game = true;
            Ok(())
        } else {
            Err(UserError::NotEnoughPlayers)
        }
    }

    pub fn is_ready_for_game_start(&self) -> bool {
        self.state.start_game
    }
}

impl From<Game<Lobby>> for Game<SeatPlayers> {
    fn from(value: Game<Lobby>) -> Self {
        Self {
            data: value.data,
            state: SeatPlayers {},
        }
    }
}

impl From<Game<SeatPlayers>> for Game<Lobby> {
    fn from(value: Game<SeatPlayers>) -> Self {
        Self {
            data: value.data,
            state: Lobby::new(),
        }
    }
}

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
            if value.data.seats[i].is_some() {
                i += 1;
            }
        }
        value.data.num_players_active = value.data.num_players;
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
        let mut seats = value
            .data
            .seats
            .iter()
            .enumerate()
            .cycle()
            .skip(value.data.big_blind_idx + 1);
        (value.data.big_blind_idx, _) = seats.find(|(_, s)| s.is_some()).unwrap();
        (value.data.starting_action_idx, _) = seats.find(|(_, s)| s.is_some()).unwrap();
        value.data.next_action_idx = Some(value.data.starting_action_idx);
        // Reverse the table search to find the small blind position relative
        // to the big blind position since the small blind must always trail the big
        // blind.
        let mut seats = value
            .data
            .seats
            .iter()
            .enumerate()
            .rev()
            .cycle()
            .skip(MAX_PLAYERS - value.data.big_blind_idx);
        (value.data.small_blind_idx, _) = seats.find(|(_, s)| s.is_some()).unwrap();
        Self {
            data: value.data,
            state: CollectBlinds {},
        }
    }
}

/// Collect blinds, initializing the main pot.
impl From<Game<CollectBlinds>> for Game<Deal> {
    fn from(mut value: Game<CollectBlinds>) -> Self {
        value.data.pots.push(Pot::new());
        let pot = &mut value.data.pots[0];
        for (seat_idx, blind) in [
            (value.data.small_blind_idx, value.data.small_blind),
            (value.data.big_blind_idx, value.data.big_blind),
        ] {
            let player = value.data.seats[seat_idx].as_mut().unwrap();
            let user = value.data.users.get_mut(&player.name).unwrap();
            let bet = match user.money.cmp(&blind) {
                Ordering::Equal => {
                    player.state = PlayerState::AllIn;
                    Bet {
                        action: BetAction::AllIn,
                        amount: user.money,
                    }
                }
                Ordering::Greater => {
                    player.state = PlayerState::Wait;
                    Bet {
                        action: BetAction::Raise,
                        amount: blind,
                    }
                }
                _ => unreachable!(
                    "A player can't be in a game if they don't have enough for the big blind."
                ),
            };
            // Impossible for a side pot to be created from the blinds, so
            // we don't even need to check.
            pot.bet(seat_idx, &bet);
            user.money -= blind;
        }
        value.data.num_players_called = 0;
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

        let mut seats = (0..MAX_PLAYERS).cycle().skip(value.data.small_blind_idx);
        // Deal 2 cards per player, looping over players and dealing them 1 card
        // at a time.
        while value.data.deck_idx < (2 * value.data.num_players) {
            let deal_idx = seats.find(|&idx| value.data.seats[idx].is_some()).unwrap();
            let player = value.data.seats[deal_idx].as_mut().unwrap();
            let card = value.data.deck[value.data.deck_idx];
            player.cards.push(card);
            value.data.deck_idx += 1;
        }
        let action_options = value.prepare_for_next_phase();
        Self {
            data: value.data,
            state: TakeAction { action_options },
        }
    }
}

impl Game<TakeAction> {
    pub fn act(&mut self, action: Action) -> Result<(), UserError> {
        self.affect(action)?;
        self.data.next_action_idx = self.get_next_action_idx(false);
        self.state.action_options = self.get_next_action_options();
        Ok(())
    }

    fn affect(&mut self, action: Action) -> Result<(), UserError> {
        let seat_idx = self.data.next_action_idx.unwrap();
        if !self
            .state
            .action_options
            .as_ref()
            .unwrap()
            .contains(&action)
        {
            return Err(UserError::InvalidAction { action });
        }
        let player = self.data.seats[seat_idx].as_mut().unwrap();
        let user = self.data.users.get(&player.name).unwrap();
        // Convert the action to a valid bet. Sanitize the bet amount according
        // to the player's intended action.
        let mut bet = match action {
            Action::AllIn => Bet {
                action: BetAction::AllIn,
                amount: user.money,
            },
            Action::Call(amount) => Bet {
                action: BetAction::Call,
                amount,
            },
            Action::Check => {
                self.data.num_players_called += 1;
                return Ok(());
            }
            Action::Fold => {
                player.state = PlayerState::Fold;
                self.data.num_players_active -= 1;
                return Ok(());
            }
            Action::Raise(amount) => Bet {
                action: BetAction::Raise,
                amount,
            },
        };
        if bet.amount >= user.money {
            bet.action = BetAction::AllIn;
            bet.amount = user.money;
            player.state = PlayerState::AllIn;
        }
        // Do some additional bet validation based on the bet's amount.
        let total_call = self.get_total_call();
        let total_investment = self.get_total_investment_by_seat(seat_idx);
        let new_total_investment = total_investment + bet.amount;
        match bet.action {
            BetAction::AllIn => {
                self.data.num_players_active -= 1;
                if new_total_investment > total_call {
                    self.data.num_players_called = 0;
                }
            }
            BetAction::Call => {
                if new_total_investment != total_call {
                    return Err(UserError::InvalidBet { bet });
                }
                self.data.num_players_called += 1;
            }
            BetAction::Raise => {
                if new_total_investment < (2 * total_call) {
                    return Err(UserError::InvalidBet { bet });
                }
                self.data.num_players_called = 1;
            }
        }
        // The player's bet is OK. Remove the bet amount from the player's
        // stack and start distributing it appropriately amongst all the pots.
        let player = self.data.seats[seat_idx].as_ref().unwrap();
        let user = self.data.users.get_mut(&player.name).unwrap();
        user.money -= bet.amount;
        // Place bets for all pots except for the last. If the player's bet
        // is too small, it's considered an all-in (though this really should've
        // been caught earlier during bet sanitization).
        let num_pots = self.data.pots.len();
        for pot in self.data.pots.iter_mut().take(num_pots - 1) {
            let call = pot.get_call_by_seat(seat_idx);
            let pot_bet = if bet.amount <= call {
                Bet {
                    action: BetAction::AllIn,
                    amount: bet.amount,
                }
            } else {
                Bet {
                    action: BetAction::Call,
                    amount: call,
                }
            };
            pot.bet(seat_idx, &pot_bet);
            bet.amount -= pot_bet.amount;
        }
        // Can only continue betting for the final pot if the player
        // still has money to bet with.
        if bet.amount > 0 {
            let pot = self.data.pots.iter_mut().last().unwrap();
            // Make sure we catch the side pot if one was created.
            if let Some(side_pot) = pot.bet(seat_idx, &bet) {
                self.data.pots.push(side_pot);
            }
        }
        Ok(())
    }
}

impl From<Game<TakeAction>> for Game<Flop> {
    fn from(value: Game<TakeAction>) -> Self {
        Self {
            data: value.data,
            state: Flop {},
        }
    }
}

impl From<Game<TakeAction>> for Game<Turn> {
    fn from(value: Game<TakeAction>) -> Self {
        Self {
            data: value.data,
            state: Turn {},
        }
    }
}

impl From<Game<TakeAction>> for Game<River> {
    fn from(value: Game<TakeAction>) -> Self {
        Self {
            data: value.data,
            state: River {},
        }
    }
}

impl From<Game<TakeAction>> for Game<ShowHands> {
    fn from(value: Game<TakeAction>) -> Self {
        Self {
            data: value.data,
            state: ShowHands::new(),
        }
    }
}

impl Game<Flop> {
    fn step(&mut self) {
        for _ in 0..3 {
            let card = self.data.deck[self.data.deck_idx];
            self.data.board.push(card);
            self.data.deck_idx += 1;
        }
    }
}

/// Put the first 3 cards on the board.
impl From<Game<Flop>> for Game<TakeAction> {
    fn from(mut value: Game<Flop>) -> Self {
        value.step();
        let action_options = value.prepare_for_next_phase();
        Self {
            data: value.data,
            state: TakeAction { action_options },
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
        let card = self.data.deck[self.data.deck_idx];
        self.data.board.push(card);
        self.data.deck_idx += 1;
    }
}

/// Put the 4th card on the board.
impl From<Game<Turn>> for Game<TakeAction> {
    fn from(mut value: Game<Turn>) -> Self {
        value.step();
        let action_options = value.prepare_for_next_phase();
        Self {
            data: value.data,
            state: TakeAction { action_options },
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
        let card = self.data.deck[self.data.deck_idx];
        self.data.board.push(card);
        self.data.deck_idx += 1;
    }
}

/// Put the 5th card on the board.
impl From<Game<River>> for Game<TakeAction> {
    fn from(mut value: Game<River>) -> Self {
        value.step();
        let action_options = value.prepare_for_next_phase();
        Self {
            data: value.data,
            state: TakeAction { action_options },
        }
    }
}

/// Put the 5th card on the board assuming the game is ready for
/// showdown.
impl From<Game<River>> for Game<ShowHands> {
    fn from(mut value: Game<River>) -> Self {
        value.step();
        Self {
            data: value.data,
            state: ShowHands::new(),
        }
    }
}

impl Game<ShowHands> {
    pub fn show_hand(&mut self, username: &str) -> Result<(), UserError> {
        match self
            .data
            .seats
            .iter_mut()
            .flatten()
            .find(|s| s.name == username)
        {
            Some(player) => {
                if player.state != PlayerState::Show {
                    player.state = PlayerState::Show;
                    Ok(())
                } else {
                    Err(UserError::UserAlreadyShowingHand)
                }
            }
            None => Err(UserError::UserNotPlaying),
        }
    }
}

impl From<Game<ShowHands>> for Game<DistributePot> {
    fn from(mut value: Game<ShowHands>) -> Self {
        if let Some(pot) = value.data.pots.last() {
            for (seat_idx, _) in pot.investments.iter() {
                let player = value.data.seats[*seat_idx].as_mut().unwrap();
                if player.state != PlayerState::Show {
                    player.state = PlayerState::Show;
                }
            }
        }
        Self {
            data: value.data,
            state: DistributePot {
                hand_eval_cache: value.state.hand_eval_cache,
            },
        }
    }
}

/// Get all players in the pot that haven't folded and compare their
/// hands to one another. Get the winning indices and distribute
/// the pot accordingly. If there's a tie, winners are given their
/// original investments and then split the remainder. Everyone
/// can only lose as much as they had originally invested or as much
/// as a winner had invested, whichever is lower. This prevents folks
/// that went all-in, but have much more money than the winner, from
/// losing the extra money.
impl From<Game<DistributePot>> for Game<ShowHands> {
    fn from(mut value: Game<DistributePot>) -> Self {
        if let Some(mut pot) = value.data.pots.pop() {
            let mut seats_in_pot = Vec::with_capacity(MAX_PLAYERS);
            let mut hands_in_pot = Vec::with_capacity(MAX_PLAYERS);
            for (seat_idx, _) in pot.investments.iter() {
                let player = value.data.seats[*seat_idx].as_mut().unwrap();
                if player.state != PlayerState::Fold {
                    seats_in_pot.push(*seat_idx);
                    let hand_eval = || {
                        let mut cards = player.cards.clone();
                        cards.extend(value.data.board.clone());
                        cards.sort_unstable();
                        // Add ace highs to the hand for evaluation.
                        for card_idx in 0..4 {
                            if let (1u8, suit) = cards[card_idx] {
                                cards.push((14u8, suit));
                            }
                        }
                        functional::eval(&cards)
                    };
                    let hand = value
                        .state
                        .hand_eval_cache
                        .entry(*seat_idx)
                        .or_insert_with(hand_eval);
                    hands_in_pot.push(hand.clone());
                }
            }
            // Only up to 4 players can split the pot (only four suits per card value).
            let mut distributions_per_player: HashMap<usize, Usd> = HashMap::with_capacity(4);
            let mut winner_indices = functional::argmax(&hands_in_pot);
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
                    let mut money_per_winner: HashMap<usize, Usd> = HashMap::with_capacity(4);
                    let mut max_winner_investment = Usd::MIN;
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
                    // There's a possibility for the pot to not split perfectly
                    // amongst all players; in this case, the remainder is
                    // put in the donations and will eventually be redistributed
                    // amongst remaining users. This also encourages users to
                    // stay in the game so they can be donated these breadcrumbs
                    // and continue playing with them.
                    let pot_split = pot.size / num_winners as Usd;
                    let mut pot_remainder = pot.size as Usdf;
                    for (winner_seat_idx, money) in money_per_winner {
                        distributions_per_player.insert(winner_seat_idx, money + pot_split);
                        pot_remainder -= pot_split as Usdf;
                    }
                    value.data.donations += pot_remainder;
                }
            }

            // Give money back to players.
            for (seat_idx, distribution) in distributions_per_player {
                let player = value.data.seats[seat_idx].as_ref().unwrap();
                let user = value.data.users.get_mut(&player.name).unwrap();
                user.money += distribution;
            }
        }
        Self {
            data: value.data,
            state: ShowHands {
                hand_eval_cache: value.state.hand_eval_cache,
            },
        }
    }
}

impl From<Game<ShowHands>> for Game<RemovePlayers> {
    fn from(value: Game<ShowHands>) -> Self {
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
            let donation_per_user = value.data.donations as Usd / num_users as Usd;
            for (_, user) in value.data.users.iter_mut() {
                user.money += donation_per_user;
                value.data.donations -= donation_per_user as Usdf;
            }
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
        let min_money = value
            .data
            .users
            .values()
            .map(|user| user.money)
            .filter(|money| *money >= value.data.big_blind)
            .min()
            .unwrap_or(Usd::MAX);
        if min_money < Usd::MAX {
            let multiple = max(1, min_money / STARTING_STACK);
            value.data.small_blind = multiple * MIN_SMALL_BLIND;
            value.data.big_blind = multiple * MIN_BIG_BLIND;
        }
        Self {
            data: value.data,
            state: BootPlayers {},
        }
    }
}

/// Remove players from seats that don't have enough money to satisfy
/// the big blind, and reset player states for players that do have
/// enough money to play.
impl From<Game<BootPlayers>> for Game<Lobby> {
    fn from(mut value: Game<BootPlayers>) -> Self {
        value.data.board.clear();
        for player in value.data.seats.iter_mut().flatten() {
            let user = value.data.users.get(&player.name).unwrap();
            if user.money < value.data.big_blind {
                value.data.players_to_spectate.insert(player.name.clone());
            } else {
                player.reset();
            }
        }
        while let Some(username) = value.data.players_to_spectate.pop_first() {
            value.spectate_user(&username).unwrap();
        }
        Self {
            data: value.data,
            state: Lobby::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::poker::entities::{Action, Suit, STARTING_STACK};
    use crate::poker::game::{
        DistributePot, DivideDonations, Lobby, RemovePlayers, TakeAction, UpdateBlinds,
        MIN_BIG_BLIND, MIN_SMALL_BLIND,
    };

    use super::{
        BootPlayers, CollectBlinds, Deal, Flop, Game, MoveButton, River, SeatPlayers, ShowHands,
        Turn, UserError, UserState, MAX_PLAYERS, MAX_USERS,
    };

    fn init_game() -> Game<SeatPlayers> {
        let mut game = Game::<SeatPlayers>::new();
        for i in 0..3 {
            let username = i.to_string();
            game.new_user(&username).unwrap();
            game.waitlist_user(&username).unwrap();
        }
        game
    }

    fn init_game_at_collect_blinds() -> Game<Deal> {
        let game = init_game();
        let game: Game<MoveButton> = game.into();
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        game
    }

    fn init_game_at_deal() -> Game<TakeAction> {
        let game = init_game();
        let game: Game<MoveButton> = game.into();
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let game: Game<TakeAction> = game.into();
        game
    }

    fn init_game_at_move_button() -> Game<CollectBlinds> {
        let game = init_game();
        let game: Game<MoveButton> = game.into();
        let game: Game<CollectBlinds> = game.into();
        game
    }

    fn init_game_at_seat_players() -> Game<MoveButton> {
        let game = init_game();
        let game: Game<MoveButton> = game.into();
        game
    }

    fn init_game_at_showdown_with_2_all_ins() -> Game<ShowHands> {
        let mut game = init_game_at_deal();
        game.act(Action::Fold).unwrap();
        game.act(Action::AllIn).unwrap();
        game.act(Action::AllIn).unwrap();
        let game: Game<Flop> = game.into();
        let game: Game<Turn> = game.into();
        let game: Game<River> = game.into();
        let game: Game<ShowHands> = game.into();
        game
    }

    fn init_game_at_showdown_with_3_all_ins() -> Game<ShowHands> {
        let mut game = init_game_at_deal();
        game.act(Action::AllIn).unwrap();
        game.act(Action::AllIn).unwrap();
        game.act(Action::AllIn).unwrap();
        let game: Game<Flop> = game.into();
        let game: Game<Turn> = game.into();
        let game: Game<River> = game.into();
        let game: Game<ShowHands> = game.into();
        game
    }

    #[test]
    fn collect_blinds() {
        let game = init_game_at_collect_blinds();
        for (i, blind) in [0, MIN_SMALL_BLIND, MIN_BIG_BLIND].iter().enumerate() {
            let username = i.to_string();
            assert_eq!(
                game.data.users.get(&username).unwrap().money,
                STARTING_STACK - blind
            );
        }
    }

    #[test]
    fn deal() {
        let game = init_game_at_deal();
        assert_eq!(game.get_num_community_cards(), 0);
        assert_eq!(game.data.deck_idx, 2 * game.data.users.len());
        for player in game.data.seats.iter().flatten() {
            assert_eq!(player.cards.len(), 2);
        }
    }

    #[test]
    fn early_showdown() {
        let mut game = init_game_at_deal();
        game.act(Action::Fold).unwrap();
        game.act(Action::AllIn).unwrap();
        game.act(Action::AllIn).unwrap();
        let game: Game<Flop> = game.into();
        let game: Game<Turn> = game.into();
        assert_eq!(game.get_num_community_cards(), 3);
        let game: Game<River> = game.into();
        assert_eq!(game.get_num_community_cards(), 4);
        let game: Game<ShowHands> = game.into();
        assert_eq!(game.get_num_community_cards(), 5);
    }

    #[test]
    fn early_showdown_1_winner() {
        let mut game = init_game_at_showdown_with_2_all_ins();
        // Gotta replace all the cards to make the showdown result
        // deterministic. Also test out a tricky scenario: the ace
        // (as 1u8) counts as a high ace as well, so seat 1 wins
        // the showdown with a higher flush.
        game.data.board = vec![
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ];
        game.data.seats[1].as_mut().unwrap().cards = vec![(1u8, Suit::Diamond), (7u8, Suit::Heart)];
        game.data.seats[2].as_mut().unwrap().cards = vec![(2u8, Suit::Diamond), (5u8, Suit::Heart)];
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for (i, money) in [STARTING_STACK, 2 * STARTING_STACK, 0].iter().enumerate() {
            let username = i.to_string();
            assert_eq!(game.data.users.get(&username).unwrap().money, *money);
        }
    }

    #[test]
    fn early_showdown_2_winners() {
        let mut game = init_game_at_showdown_with_2_all_ins();
        game.data.board = vec![
            (2u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ];
        game.data.seats[1].as_mut().unwrap().cards = vec![(1u8, Suit::Heart), (7u8, Suit::Heart)];
        game.data.seats[2].as_mut().unwrap().cards = vec![(2u8, Suit::Heart), (5u8, Suit::Heart)];
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for i in (0..3).into_iter() {
            let username = i.to_string();
            assert_eq!(
                game.data.users.get(&username).unwrap().money,
                STARTING_STACK
            );
        }
    }

    #[test]
    fn early_showdown_3_decreasing_all_ins() {
        let game = init_game();
        let mut game: Game<MoveButton> = game.into();
        for i in (0..3).into_iter() {
            let username = i.to_string();
            game.data.users.get_mut(&username).unwrap().money = STARTING_STACK * (3 - i);
        }
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let mut game: Game<TakeAction> = game.into();
        game.act(Action::AllIn).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([Action::AllIn, Action::Fold,]))
        );
        game.act(Action::AllIn).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([Action::AllIn, Action::Fold,]))
        );
        game.act(Action::AllIn).unwrap();
        let game: Game<Flop> = game.into();
        let game: Game<Turn> = game.into();
        let game: Game<River> = game.into();
        let mut game: Game<ShowHands> = game.into();
        game.data.board = vec![
            (1u8, Suit::Spade),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ];
        game.data.seats[0].as_mut().unwrap().cards =
            vec![(3u8, Suit::Heart), (11u8, Suit::Diamond)];
        game.data.seats[1].as_mut().unwrap().cards =
            vec![(1u8, Suit::Heart), (10u8, Suit::Diamond)];
        game.data.seats[2].as_mut().unwrap().cards = vec![(2u8, Suit::Heart), (9u8, Suit::Diamond)];
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for (i, money) in [6 * STARTING_STACK, 0, 0].iter().enumerate() {
            let username = i.to_string();
            assert_eq!(game.data.users.get(&username).unwrap().money, *money);
        }
    }

    #[test]
    fn early_showdown_3_increasing_all_ins() {
        let game = init_game();
        let mut game: Game<MoveButton> = game.into();
        for i in (0..3).into_iter() {
            let username = i.to_string();
            game.data.users.get_mut(&username).unwrap().money = STARTING_STACK * (i + 1);
        }
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let mut game: Game<TakeAction> = game.into();
        game.act(Action::AllIn).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(195),
                Action::Fold,
            ]))
        );
        game.act(Action::AllIn).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(395),
                Action::Fold,
            ]))
        );
        game.act(Action::AllIn).unwrap();
        let game: Game<Flop> = game.into();
        let game: Game<Turn> = game.into();
        let game: Game<River> = game.into();
        let mut game: Game<ShowHands> = game.into();
        game.data.board = vec![
            (1u8, Suit::Spade),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ];
        game.data.seats[0].as_mut().unwrap().cards =
            vec![(3u8, Suit::Heart), (11u8, Suit::Diamond)];
        game.data.seats[1].as_mut().unwrap().cards =
            vec![(1u8, Suit::Heart), (10u8, Suit::Diamond)];
        game.data.seats[2].as_mut().unwrap().cards = vec![(2u8, Suit::Heart), (9u8, Suit::Diamond)];
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for (i, money) in [3 * STARTING_STACK, 2 * STARTING_STACK, STARTING_STACK]
            .iter()
            .enumerate()
        {
            let username = i.to_string();
            assert_eq!(game.data.users.get(&username).unwrap().money, *money);
        }
    }

    #[test]
    fn manipulating_user_in_lobby() {
        let mut game = Game::<SeatPlayers>::new();
        let username = "ognf";

        game.new_user(username).unwrap();
        assert!(game.data.users.contains_key(username));
        assert!(game.data.spectators.contains(username));
        assert_eq!(
            game.data.users.get(username).unwrap().state,
            UserState::Spectating
        );

        assert_eq!(game.new_user(username), Err(UserError::UserAlreadyExists));

        game.waitlist_user(username).unwrap();
        assert!(game.data.waitlist.contains(&username.to_string()));
        assert_eq!(
            game.data.users.get(username).unwrap().state,
            UserState::Waiting
        );

        game.spectate_user(username).unwrap();
        assert!(game.data.spectators.contains(username));
        assert_eq!(
            game.data.users.get(username).unwrap().state,
            UserState::Spectating
        );

        game.remove_user(username).unwrap();
        assert!(!game.data.users.contains_key(username));
        assert!(!game.data.spectators.contains(username));

        assert_eq!(game.remove_user(username), Err(UserError::UserDoesNotExist));
        assert_eq!(
            game.waitlist_user(username),
            Err(UserError::UserDoesNotExist)
        );
        assert_eq!(
            game.spectate_user(username),
            Err(UserError::UserDoesNotExist)
        );

        game.new_user(username).unwrap();
        assert!(game.data.users.contains_key(username));
        assert!(game.data.spectators.contains(username));

        game.waitlist_user(username).unwrap();
        assert!(game.data.waitlist.contains(&username.to_string()));
        assert_eq!(
            game.data.users.get(username).unwrap().state,
            UserState::Waiting
        );

        game.remove_user(username).unwrap();
        assert!(!game.data.users.contains_key(username));
        assert!(!game.data.waitlist.contains(&username.to_string()));

        for i in 0..MAX_USERS {
            game.new_user(&i.to_string()).unwrap();
        }
        assert_eq!(game.new_user(username), Err(UserError::CapacityReached));
    }

    #[test]
    fn move_button() {
        let game = init_game_at_move_button();
        assert_eq!(game.data.small_blind_idx, 1);
        assert_eq!(game.data.big_blind_idx, 2);
        assert_eq!(game.data.starting_action_idx, 0);
        assert_eq!(
            game.data.next_action_idx,
            Some(game.data.starting_action_idx)
        );
    }

    // Fill a game to capacity and then move the action index around.
    // Every player should get their turn.
    #[test]
    fn move_next_action_idx() {
        let mut game = Game::<SeatPlayers>::new();
        for i in 0..MAX_USERS {
            let username = i.to_string();
            game.new_user(&username).unwrap();
            game.waitlist_user(&username).unwrap();
        }
        let game: Game<MoveButton> = game.into();
        let mut game: Game<CollectBlinds> = game.into();
        for i in 3..MAX_PLAYERS {
            assert_eq!(game.data.next_action_idx, Some(i));
            game.data.next_action_idx = game.get_next_action_idx(false);
        }
        for i in 0..3 {
            assert_eq!(game.data.next_action_idx, Some(i));
            game.data.next_action_idx = game.get_next_action_idx(false);
        }
    }

    #[test]
    fn prepare_for_next_game() {
        let mut game = init_game_at_showdown_with_3_all_ins();
        game.data.board = vec![
            (1u8, Suit::Spade),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ];
        game.data.seats[0].as_mut().unwrap().cards = vec![(3u8, Suit::Heart), (8u8, Suit::Diamond)];
        game.data.seats[1].as_mut().unwrap().cards = vec![(1u8, Suit::Heart), (7u8, Suit::Heart)];
        game.data.seats[2].as_mut().unwrap().cards = vec![(2u8, Suit::Heart), (5u8, Suit::Heart)];
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<RemovePlayers> = game.into();
        let game: Game<DivideDonations> = game.into();
        let game: Game<UpdateBlinds> = game.into();
        let game: Game<BootPlayers> = game.into();
        assert_eq!(game.data.big_blind, 3 * MIN_BIG_BLIND);
        let game: Game<Lobby> = game.into();
        assert_eq!(game.data.num_players, 1);
    }

    #[test]
    fn remove_player() {
        let mut game = init_game_at_showdown_with_2_all_ins();
        game.data.board = vec![
            (2u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ];
        game.data.seats[1].as_mut().unwrap().cards = vec![(1u8, Suit::Heart), (7u8, Suit::Heart)];
        game.data.seats[2].as_mut().unwrap().cards = vec![(2u8, Suit::Heart), (5u8, Suit::Heart)];
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<RemovePlayers> = game.into();
        let mut game: Game<DivideDonations> = game.into();
        game.remove_user("0").unwrap();
        assert!(!game.data.users.contains_key("0"));
        assert!(game.data.users.contains_key("1"));
        assert!(game.data.users.contains_key("2"));
        let game: Game<UpdateBlinds> = game.into();
        for i in (1..3).into_iter() {
            let username = i.to_string();
            assert_eq!(
                game.data.users.get(&username).unwrap().money,
                STARTING_STACK + STARTING_STACK / 2
            );
        }
    }

    #[test]
    fn remove_player_with_queue() {
        let mut game = init_game_at_showdown_with_2_all_ins();
        game.data.board = vec![
            (2u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ];
        game.data.seats[1].as_mut().unwrap().cards = vec![(1u8, Suit::Heart), (7u8, Suit::Heart)];
        game.data.seats[2].as_mut().unwrap().cards = vec![(2u8, Suit::Heart), (5u8, Suit::Heart)];
        game.remove_user("0").unwrap();
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<RemovePlayers> = game.into();
        let game: Game<DivideDonations> = game.into();
        assert!(!game.data.users.contains_key("0"));
        assert!(game.data.users.contains_key("1"));
        assert!(game.data.users.contains_key("2"));
        let game: Game<UpdateBlinds> = game.into();
        for i in (1..3).into_iter() {
            let username = i.to_string();
            assert_eq!(
                game.data.users.get(&username).unwrap().money,
                STARTING_STACK + STARTING_STACK / 2
            );
        }
    }

    #[test]
    fn seat_players() {
        let game = init_game_at_seat_players();
        assert_eq!(game.data.num_players, game.data.users.len());
        assert_eq!(game.data.num_players_active, game.data.users.len());
        for (_, user) in game.data.users {
            assert_eq!(user.state, UserState::Playing);
        }
    }

    #[test]
    fn take_action_2_all_ins() {
        let mut game = init_game_at_deal();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(10),
                Action::Fold,
                Action::Raise(20)
            ]))
        );
        game.act(Action::AllIn).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([Action::AllIn, Action::Fold]))
        );
        game.act(Action::AllIn).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([Action::AllIn, Action::Fold]))
        );
        game.act(Action::Fold).unwrap();
        assert_eq!(game.get_next_action_options(), None);
    }

    #[test]
    fn take_action_2_calls_1_check() {
        let mut game = init_game_at_deal();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(10),
                Action::Fold,
                Action::Raise(20)
            ]))
        );
        game.act(Action::Call(10)).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(5),
                Action::Fold,
                Action::Raise(15)
            ]))
        );
        game.act(Action::Call(5)).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Check,
                Action::Fold,
                Action::Raise(20)
            ]))
        );
        game.act(Action::Check).unwrap();
        assert_eq!(game.get_next_action_options(), None);
    }

    #[test]
    fn take_action_2_folds() {
        let mut game = init_game_at_deal();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(10),
                Action::Fold,
                Action::Raise(20)
            ]))
        );
        game.act(Action::Fold).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(5),
                Action::Fold,
                Action::Raise(15)
            ]))
        );
        game.act(Action::Fold).unwrap();
        assert_eq!(game.get_next_action_options(), None);
    }

    #[test]
    fn take_action_2_reraises() {
        let mut game = init_game_at_deal();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(10),
                Action::Fold,
                Action::Raise(20)
            ]))
        );
        game.act(Action::Fold).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(5),
                Action::Fold,
                Action::Raise(15)
            ]))
        );
        // Total call is 20
        game.act(Action::Raise(15)).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(10),
                Action::Fold,
                Action::Raise(30)
            ]))
        );
        // Total call is 40
        game.act(Action::Raise(30)).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(20),
                Action::Fold,
                Action::Raise(60)
            ]))
        );
        // Total call is 80
        game.act(Action::Raise(60)).unwrap();
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(40),
                Action::Fold,
                Action::Raise(120)
            ]))
        );
        game.act(Action::Fold).unwrap();
        assert_eq!(game.get_next_action_options(), None);
    }
}
