use core::fmt;
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::cmp::{max, min, Ordering};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use thiserror::Error;

use super::{
    constants::{DEFAULT_MAX_USERS, MAX_PLAYERS, MAX_POTS},
    entities::{
        Action, Bet, BetAction, Card, Player, PlayerState, Pot, SubHand, Usd, Usdf, User,
        DEFAULT_MIN_BIG_BLIND, DEFAULT_MIN_SMALL_BLIND, DEFAULT_STARTING_STACK,
    },
    functional,
};

#[derive(Debug, Deserialize, Eq, Error, PartialEq, Serialize)]
pub enum UserError {
    #[error("Cannot show hand now.")]
    CannotShowHand,
    #[error("Cannot start game unless you're waitlisted or playing.")]
    CannotStartGame,
    #[error("Game is full.")]
    CapacityReached,
    #[error("Game already in progress.")]
    GameAlreadyInProgress,
    #[error("Game already starting.")]
    GameAlreadyStarting,
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
    #[error("User already exists.")]
    UserAlreadyExists,
    #[error("User does not exist.")]
    UserDoesNotExist,
    #[error("User is not playing.")]
    UserNotPlaying,
    #[error("User already showing hand.")]
    UserAlreadyShowingHand,
}

#[derive(Debug)]
pub struct GameConfig {
    pub starting_stack: Usd,
    pub min_big_blind: Usd,
    pub min_small_blind: Usd,
    pub max_players: usize,
    pub max_users: usize,
    pub max_pots: usize,
}

impl GameConfig {
    pub fn new(max_players: usize, max_users: usize, starting_stack: Usd) -> Self {
        let min_big_blind = starting_stack / 20;
        let min_small_blind = min_big_blind / 2;
        let max_pots = max_players / 2 + 1;
        Self {
            starting_stack,
            min_big_blind,
            min_small_blind,
            max_players,
            max_users,
            max_pots,
        }
    }
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            starting_stack: DEFAULT_STARTING_STACK,
            min_big_blind: DEFAULT_MIN_BIG_BLIND,
            min_small_blind: DEFAULT_MIN_SMALL_BLIND,
            max_players: MAX_PLAYERS,
            max_users: DEFAULT_MAX_USERS,
            max_pots: MAX_POTS,
        }
    }
}

type UserView = User;

#[derive(Debug, Deserialize, Serialize)]
pub struct PlayerView {
    user: UserView,
    state: PlayerState,
    cards: Option<Vec<Card>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PotView {
    call: Usd,
    size: Usd,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GameView {
    donations: Usdf,
    small_blind: Usd,
    big_blind: Usd,
    spectators: HashMap<String, UserView>,
    waitlist: VecDeque<UserView>,
    open_seats: VecDeque<usize>,
    players: Vec<PlayerView>,
    board: Vec<Card>,
    pots: Vec<PotView>,
    small_blind_idx: usize,
    big_blind_idx: usize,
    next_action_idx: Option<usize>,
}

impl fmt::Display for GameView {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f)?;
        writeln!(f, "Small blind: ${}", self.small_blind)?;
        writeln!(f, "Big blind: ${}", self.small_blind)?;

        // Display users just spectating the game.
        writeln!(f)?;
        writeln!(f, "Spectators:")?;
        match self.spectators.len() {
            0 => writeln!(f, "N/A")?,
            _ => {
                for user in self.spectators.values() {
                    writeln!(f, "{}", user)?;
                }
            }
        };

        // Display users in queue to play.
        writeln!(f)?;
        writeln!(f, "Waitlisters:")?;
        match self.waitlist.len() {
            0 => writeln!(f, "N/A")?,
            _ => {
                for waitlister in self.waitlist.iter() {
                    writeln!(f, "{}", waitlister)?;
                }
            }
        }

        // Display number of open seats.
        writeln!(f)?;
        writeln!(f, "Number of open seats: {}", self.open_seats.len())?;

        // Display all players.
        writeln!(f)?;
        writeln!(f, "Players:")?;
        match self.players.len() {
            0 => {
                writeln!(f, "N/A")?;
                writeln!(f)?;
            }
            _ => {
                for player in self.players.iter() {
                    write!(f, "{} | {} | ", player.user, player.state)?;
                    match &player.cards {
                        Some(cards) => {
                            for card in cards.iter() {
                                write!(f, "{} ", card)?;
                            }
                        }
                        None => write!(f, "?? ?? ")?,
                    }
                    writeln!(f)?;
                }
            }
        }

        // Display all pots.
        writeln!(f, "Pots:")?;
        match self.pots.len() {
            0 => writeln!(f, "N/A")?,
            _ => {
                for (mut i, pot) in self.pots.iter().enumerate() {
                    i += 1;
                    writeln!(f, "Pot {}: ${}", i, pot.size)?;
                }
            }
        }

        // Display community cards (cards on the board).
        writeln!(f)?;
        write!(f, "Board: ")?;
        for card in self.board.iter() {
            write!(f, "{} ", card)?;
        }

        // Display whose turn it is if it's someone's turn.
        if let Some(next_action_idx) = self.next_action_idx {
            let player = &self.players[next_action_idx];
            writeln!(f)?;
            writeln!(f, "{}'s turn", player.user.name)?;
        }

        writeln!(f)?;
        Ok(())
    }
}

pub type GameViews = HashMap<String, GameView>;

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
    pub spectators: HashMap<String, User>,
    pub waitlist: VecDeque<User>,
    pub open_seats: VecDeque<usize>,
    pub players: Vec<Player>,
    /// Community cards shared amongst all players.
    pub board: Vec<Card>,
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
    settings: GameConfig,
}

impl GameData {
    fn new() -> Self {
        let settings = GameConfig::default();
        Self {
            deck: functional::new_deck(),
            donations: 0.0,
            small_blind: settings.min_small_blind,
            big_blind: settings.min_big_blind,
            spectators: HashMap::with_capacity(settings.max_users),
            waitlist: VecDeque::with_capacity(settings.max_users),
            open_seats: VecDeque::from_iter(0..settings.max_players),
            players: Vec::with_capacity(settings.max_players),
            board: Vec::with_capacity(5),
            num_players_active: 0,
            num_players_called: 0,
            pots: Vec::with_capacity(settings.max_pots),
            players_to_remove: BTreeSet::new(),
            players_to_spectate: BTreeSet::new(),
            deck_idx: 0,
            small_blind_idx: 0,
            big_blind_idx: 1,
            starting_action_idx: 2,
            next_action_idx: None,
            settings,
        }
    }
}

impl From<GameConfig> for GameData {
    fn from(value: GameConfig) -> Self {
        Self {
            deck: functional::new_deck(),
            donations: 0.0,
            small_blind: value.min_small_blind,
            big_blind: value.min_big_blind,
            spectators: HashMap::with_capacity(value.max_users),
            waitlist: VecDeque::with_capacity(value.max_users),
            open_seats: VecDeque::from_iter(0..value.max_players),
            players: Vec::with_capacity(value.max_players),
            board: Vec::with_capacity(5),
            num_players_active: 0,
            num_players_called: 0,
            pots: Vec::with_capacity(value.max_pots),
            players_to_remove: BTreeSet::new(),
            players_to_spectate: BTreeSet::new(),
            deck_idx: 0,
            small_blind_idx: 0,
            big_blind_idx: 1,
            starting_action_idx: 2,
            next_action_idx: None,
            settings: value,
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

#[derive(Clone, Debug)]
pub struct TakeAction {
    pub action_options: Option<HashSet<Action>>,
}

#[derive(Debug)]
pub struct Flop {}

#[derive(Debug)]
pub struct Turn {}

#[derive(Debug)]
pub struct River {}

#[derive(Clone, Debug)]
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
    fn as_view(&self, username: &str) -> GameView {
        let mut players = Vec::with_capacity(self.data.settings.max_players);
        for player in self.data.players.iter() {
            let cards = if player.user.name == username || player.state == PlayerState::Show {
                Some(player.cards.clone())
            } else {
                None
            };
            let player_view = PlayerView {
                user: player.user.clone(),
                state: player.state.clone(),
                cards,
            };
            players.push(player_view);
        }
        GameView {
            donations: self.data.donations,
            small_blind: self.data.small_blind,
            big_blind: self.data.big_blind,
            spectators: self.data.spectators.clone(),
            waitlist: self.data.waitlist.clone(),
            open_seats: self.data.open_seats.clone(),
            players,
            board: self.data.board.clone(),
            pots: self
                .data
                .pots
                .iter()
                .map(|pot| PotView {
                    call: pot.call,
                    size: pot.size,
                })
                .collect(),
            small_blind_idx: self.data.small_blind_idx,
            big_blind_idx: self.data.big_blind_idx,
            next_action_idx: self.data.next_action_idx,
        }
    }

    pub fn contains_player(&self, username: &str) -> bool {
        self.data.players.iter().any(|p| p.user.name == username)
    }

    fn contains_user(&self, username: &str) -> bool {
        self.data.spectators.contains_key(username)
            || self
                .data
                .waitlist
                .iter()
                .chain(self.data.players.iter().map(|p| &p.user))
                .any(|u| u.name == username)
    }

    fn contains_spectator(&self, username: &str) -> bool {
        self.data.spectators.contains_key(username)
    }

    pub fn contains_waitlister(&self, username: &str) -> bool {
        self.data.waitlist.iter().any(|u| u.name == username)
    }

    /// Return the index of the player who has the next action, or
    /// nothing if no one has the next turn.
    fn get_next_action_idx(&self, new_phase: bool) -> Option<usize> {
        if self.is_end_of_round() {
            return None;
        }
        match self.data.next_action_idx {
            Some(action_idx) => self
                .data
                .players
                .iter()
                .enumerate()
                .cycle()
                .skip(action_idx + !new_phase as usize)
                .find(|(_, p)| p.state == PlayerState::Wait)
                .map(|(next_action_idx, _)| next_action_idx),
            None => None,
        }
    }

    /// Return the set of possible actions the next player can
    /// make, or nothing if there are no actions possible for the current
    /// state.
    fn get_next_action_options(&self) -> Option<HashSet<Action>> {
        if self.is_ready_for_next_phase() {
            return None;
        }
        match self.data.next_action_idx {
            Some(action_idx) => {
                let mut action_options = HashSet::from([Action::AllIn, Action::Fold]);
                let user = &self.data.players[action_idx].user;
                let raise = self.get_total_min_raise_by_player_idx(action_idx);
                let call = self.get_total_call_by_player_idx(action_idx);
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

    /// Return the username of the user that has the next turn (or nothing
    /// if there is no turn next). Helps determine whether to notify the
    /// player that their turn has come.
    pub fn get_next_action_username(&self) -> Option<String> {
        self.data
            .next_action_idx
            .map(|action_idx| self.data.players[action_idx].user.name.clone())
    }

    /// Return the number of cards that've been dealt. This helps
    /// signal state transitions (i.e., determine whether to move on
    /// to the flop, turn, river, etc).
    pub fn get_num_community_cards(&self) -> usize {
        self.data.board.len()
    }

    fn get_num_players(&self) -> usize {
        self.data.players.len()
    }

    /// Return the number of players plus the number of players in
    /// the waitlist. This is equal to the number of players that
    /// could play the game if the game started. This helps determine
    /// whether the game can actually start.
    pub fn get_num_potential_players(&self) -> usize {
        min(
            self.data.players.len() + self.data.waitlist.len(),
            self.data.settings.max_players,
        )
    }

    fn get_num_users(&self) -> usize {
        self.data.spectators.len() + self.data.waitlist.len() + self.data.players.len()
    }

    /// Return the sum of all calls for all pots. A player's total investment
    /// must match this amount in order to stay in the pot(s).
    fn get_total_call(&self) -> Usd {
        self.data.pots.iter().map(|p| p.call).sum()
    }

    /// Return the remaining amount a player has to bet in order to stay
    /// in the pot(s).
    fn get_total_call_by_player_idx(&self, player_idx: usize) -> Usd {
        self.data
            .pots
            .iter()
            .map(|p| p.get_call_by_player_idx(player_idx))
            .sum()
    }

    /// Return the total amount a player has invested in the pot(s).
    fn get_total_investment_by_player_idx(&self, player_idx: usize) -> Usd {
        self.data
            .pots
            .iter()
            .map(|p| p.get_investment_by_player_idx(player_idx))
            .sum()
    }

    /// Return the minimum amount a player has to bet in order for their
    /// raise to be considered a valid raise.
    fn get_total_min_raise_by_player_idx(&self, player_idx: usize) -> Usd {
        2 * self.get_total_call() - self.get_total_investment_by_player_idx(player_idx)
    }

    /// Return independent views of the game for each user. For non-players,
    /// only the board is shown until the showdown. For players, only their
    /// hand and the board is shown until the showdown.
    pub fn get_views(&self) -> GameViews {
        let mut views = HashMap::with_capacity(self.data.settings.max_users);
        for username in self
            .data
            .spectators
            .keys()
            .chain(self.data.waitlist.iter().map(|u| &u.name))
            .chain(self.data.players.iter().map(|p| &p.user.name))
        {
            views.insert(username.to_string(), self.as_view(username));
        }
        views
    }

    /// Return whether the game is ready to move onto the next phase
    /// now that the betting round is over.
    fn is_end_of_round(&self) -> bool {
        self.data.num_players_active == self.data.num_players_called
    }

    /// Return whether the pot is empty, signaling whether to continue
    /// showing player hands and distributing the pots, or whether
    /// to move on to other post-game phases.
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
                self.data.num_players_active <= 1
                    && self.get_total_call_by_player_idx(action_idx) == 0
            }
            None => self.data.num_players_active <= 1,
        }
    }

    /// Return whether it's the user's turn. This helps determine whether
    /// a user trying to take an action can actually take an action, or
    /// if they're violating rules of play.
    pub fn is_turn(&self, username: &str) -> bool {
        match self.data.next_action_idx {
            Some(action_idx) => self.data.players[action_idx].user.name == username,
            None => false,
        }
    }

    pub fn new() -> Game<Lobby> {
        Game {
            data: GameData::new(),
            state: Lobby::new(),
        }
    }

    /// Add a new user to the game, making them a spectator.
    pub fn new_user(&mut self, username: &str) -> Result<bool, UserError> {
        if self.get_num_users() == self.data.settings.max_users {
            return Err(UserError::CapacityReached);
        } else if self.contains_user(username) {
            // Check if player already exists but is queued for removal.
            // This probably means the user disconnected and is trying
            // to reconnect.
            if !self.data.players_to_remove.remove(username) {
                return Err(UserError::UserAlreadyExists);
            } else {
                return Ok(false);
            }
        }
        self.data.spectators.insert(
            username.to_string(),
            User {
                name: username.to_string(),
                money: self.data.settings.starting_stack,
            },
        );
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

    /// Add a user to the waitlist, putting them in queue to play. The queue
    /// is eventually drained until the table is full and there are no more
    /// seats available for play.
    pub fn waitlist_user(&mut self, username: &str) -> Result<bool, UserError> {
        // Need to remove the player from the removal and spectate sets just in
        // case they wanted to do one of those, but then changed their mind and
        // want to play again.
        self.data.players_to_spectate.remove(username);
        self.data.players_to_remove.remove(username);
        if let Some(user) = self.data.spectators.remove(username) {
            if user.money < self.data.big_blind {
                self.data.spectators.insert(username.to_string(), user);
                return Err(UserError::InsufficientFunds {
                    big_blind: self.data.big_blind,
                });
            }
            self.data.waitlist.push_back(user);
            Ok(true)
        } else if self.contains_player(username) {
            // The user is already playing, so we don't need to do anything,
            // but we should acknowledge that the user still isn't
            // technically waitlisted.
            Ok(false)
        } else if self.contains_waitlister(username) {
            // The user is already waitlisted.
            Ok(true)
        } else {
            Err(UserError::UserDoesNotExist)
        }
    }
}

macro_rules! impl_user_managers {
    ($($t:ty),+) => {
        $(impl $t {
            pub fn remove_user(&mut self, username: &str) -> Result<bool, UserError> {
                let mut user = if let Some(user) = self.data.spectators.remove(username) {
                    user
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).unwrap()
                } else if let Some(player_idx) = self.data.players.iter().position(|p| p.user.name == username) {
                    self.data.players_to_spectate.remove(username);
                    let player = self.data.players.remove(player_idx);
                    self.data.open_seats.push_back(player.seat_idx);
                    player.user
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.donations += user.money as Usdf;
                user.money = 0;
                Ok(true)
            }

            pub fn spectate_user(&mut self, username: &str) -> Result<bool, UserError> {
                // The player has already been queued for spectate. Just wait for
                // the next spectate phase.
                if self.data.players_to_spectate.contains(username) {
                    return Ok(false);
                }
                let user = if self.data.spectators.contains_key(username) {
                    return Ok(true);
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).unwrap()
                } else if let Some(player_idx) = self.data.players.iter().position(|p| p.user.name == username) {
                    self.data.players_to_remove.remove(username);
                    let player = self.data.players.remove(player_idx);
                    self.data.open_seats.push_back(player.seat_idx);
                    player.user
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.spectators.insert(username.to_string(), user);
                Ok(true)
            }
        })*
    }
}

macro_rules! impl_user_managers_with_queue {
    ($($t:ty),+) => {
        $(impl $t {
            pub fn remove_user(&mut self, username: &str) -> Result<bool, UserError> {
                // The player has already been queued for removal. Just wait for
                // the next removal phase.
                if self.data.players_to_remove.contains(username) {
                    return Ok(false);
                }
                let mut user = if let Some(user) = self.data.spectators.remove(username) {
                    user
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).unwrap()
                } else if let Some(_) = self.data.players.iter().position(|p| p.user.name == username) {
                    // Need to remove the player from other queues just in
                    // case they changed their mind.
                    self.data.players_to_spectate.remove(username);
                    // The player is still at the table while the game is ongoing.
                    // We don't want to disrupt gameplay, so we just queue the
                    // player for removal and remove them later.
                    self.data.players_to_remove.insert(username.to_string());
                    return Ok(false);
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.donations += user.money as Usdf;
                user.money = 0;
                Ok(true)
            }

            pub fn spectate_user(&mut self, username: &str) -> Result<bool, UserError> {
                // The player has already been queued for spectate. Just wait for
                // the next spectate phase.
                if self.data.players_to_spectate.contains(username) {
                    return Ok(false);
                }
                let user = if self.data.spectators.contains_key(username) {
                    return Ok(true)
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).unwrap()
                } else if let Some(_) = self.data.players.iter().position(|p| p.user.name == username) {
                    // Need to remove the player from other queues just in
                    // case they changed their mind.
                    self.data.players_to_remove.remove(username);
                    self.data.players_to_spectate.insert(username.to_string());
                    return Ok(false);
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.spectators.insert(username.to_string(), user);
                Ok(true)
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
    pub fn init_start(&mut self) -> Result<(), UserError> {
        match (self.state.start_game, self.get_num_potential_players() >= 2) {
            (false, false) => Err(UserError::NotEnoughPlayers),
            (false, true) => {
                self.state.start_game = true;
                Ok(())
            }
            (true, _) => Err(UserError::GameAlreadyStarting),
        }
    }

    pub fn is_ready_to_start(&self) -> bool {
        self.state.start_game && self.get_num_potential_players() >= 2
    }
}

impl From<GameConfig> for Game<Lobby> {
    fn from(value: GameConfig) -> Self {
        let data: GameData = value.into();
        Self {
            data,
            state: Lobby::new(),
        }
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
        while !value.data.waitlist.is_empty() && !value.data.open_seats.is_empty() {
            let open_seat_idx = value.data.open_seats.pop_front().unwrap();
            let user = value.data.waitlist.pop_front().unwrap();
            if user.money < value.data.big_blind {
                value.data.spectators.insert(user.name.clone(), user);
            } else {
                let num_players = value.get_num_players();
                let player = Player::new(user, open_seat_idx);
                if num_players > 0 {
                    match (0..num_players - 1).position(|player_idx| {
                        value.data.players[player_idx].seat_idx < open_seat_idx
                            && value.data.players[player_idx + 1].seat_idx > open_seat_idx
                    }) {
                        Some(player_idx) => value.data.players.insert(player_idx + 1, player),
                        None => value.data.players.push(player),
                    }
                } else {
                    value.data.players.push(player);
                }
            }
        }
        value.data.num_players_active = value.get_num_players();
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
        let num_players = value.get_num_players();
        // Search for the big blind and starting positions.
        let mut seats = value
            .data
            .players
            .iter()
            .enumerate()
            .map(|(player_idx, _)| player_idx)
            .cycle()
            .skip(value.data.big_blind_idx + 1);
        value.data.big_blind_idx = seats.next().unwrap();
        value.data.starting_action_idx = seats.next().unwrap();
        value.data.next_action_idx = Some(value.data.starting_action_idx);
        // Reverse the table search to find the small blind position relative
        // to the big blind position since the small blind must always trail the big
        // blind.
        let mut seats = value
            .data
            .players
            .iter()
            .enumerate()
            .map(|(player_idx, _)| player_idx)
            .rev()
            .cycle()
            .skip(num_players - value.data.big_blind_idx);
        value.data.small_blind_idx = seats.next().unwrap();
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
        for (player_idx, blind) in [
            (value.data.small_blind_idx, value.data.small_blind),
            (value.data.big_blind_idx, value.data.big_blind),
        ] {
            let player = &mut value.data.players[player_idx];
            let bet = match player.user.money.cmp(&blind) {
                Ordering::Equal => {
                    player.state = PlayerState::AllIn;
                    Bet {
                        action: BetAction::AllIn,
                        amount: player.user.money,
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
            pot.bet(player_idx, &bet);
            player.user.money -= blind;
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

        let num_players = value.get_num_players();
        let mut seats = (0..num_players).cycle().skip(value.data.small_blind_idx);
        // Deal 2 cards per player, looping over players and dealing them 1 card
        // at a time.
        while value.data.deck_idx < (2 * num_players) {
            let deal_idx = seats.next().unwrap();
            let player = &mut value.data.players[deal_idx];
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
        match (self.data.next_action_idx, &self.state.action_options) {
            (Some(player_idx), Some(action_options)) => {
                if !action_options.contains(&action) {
                    return Err(UserError::InvalidAction { action });
                }
                let player = &mut self.data.players[player_idx];
                // Convert the action to a valid bet. Sanitize the bet amount according
                // to the player's intended action.
                let mut bet = match action {
                    Action::AllIn => Bet {
                        action: BetAction::AllIn,
                        amount: player.user.money,
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
                if bet.amount >= player.user.money {
                    bet.action = BetAction::AllIn;
                    bet.amount = player.user.money;
                    player.state = PlayerState::AllIn;
                }
                // Do some additional bet validation based on the bet's amount.
                let total_call = self.get_total_call();
                let total_investment = self.get_total_investment_by_player_idx(player_idx);
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
                let player = &mut self.data.players[player_idx];
                player.user.money -= bet.amount;
                // Place bets for all pots except for the last. If the player's bet
                // is too small, it's considered an all-in (though this really should've
                // been caught earlier during bet sanitization).
                let num_pots = self.data.pots.len();
                for pot in self.data.pots.iter_mut().take(num_pots - 1) {
                    let call = pot.get_call_by_player_idx(player_idx);
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
                    pot.bet(player_idx, &pot_bet);
                    bet.amount -= pot_bet.amount;
                }
                // Can only continue betting for the final pot if the player
                // still has money to bet with.
                if bet.amount > 0 {
                    let pot = &mut self.data.pots[num_pots - 1];
                    // Make sure we catch the side pot if one was created.
                    if let Some(side_pot) = pot.bet(player_idx, &bet) {
                        self.data.pots.push(side_pot);
                    }
                }
                Ok(())
            }
            _ => Err(UserError::OutOfTurnAction),
        }
    }

    pub fn get_action_options(&self) -> Option<HashSet<Action>> {
        self.state.action_options.clone()
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
            .players
            .iter_mut()
            .find(|p| p.user.name == username)
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
            for (player_idx, _) in pot.investments.iter() {
                let player = &mut value.data.players[*player_idx];
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
            let mut seats_in_pot = Vec::with_capacity(value.data.settings.max_players);
            let mut hands_in_pot = Vec::with_capacity(value.data.settings.max_players);
            for (player_idx, _) in pot.investments.iter() {
                let player = &mut value.data.players[*player_idx];
                if player.state != PlayerState::Fold {
                    seats_in_pot.push(*player_idx);
                    let hand_eval = || {
                        let mut cards = player.cards.clone();
                        cards.extend(value.data.board.clone());
                        cards.sort_unstable();
                        // Add ace highs to the hand for evaluation.
                        for card_idx in 0..4 {
                            if let Card(1, suit) = cards[card_idx] {
                                cards.push(Card(14, suit));
                            }
                        }
                        functional::eval(&cards)
                    };
                    let hand = value
                        .state
                        .hand_eval_cache
                        .entry(*player_idx)
                        .or_insert_with(hand_eval);
                    hands_in_pot.push(hand.clone());
                }
            }
            // Most likely that up to four players will tie. It's possible
            // for more players to tie, but very unlikely.
            let mut distributions_per_player: HashMap<usize, Usd> = HashMap::with_capacity(4);
            let winner_indices = functional::argmax(&hands_in_pot);
            let num_winners = winner_indices.len();
            // Need to first give each winner's original investment back
            // to them so the pot can be split fairly. The max winner
            // investment is tracked to handle the edge case of some
            // whale going all-in with no one else to call them.
            let mut money_per_winner: HashMap<usize, Usd> = HashMap::with_capacity(4);
            let mut max_winner_investment = Usd::MIN;
            for winner_idx in winner_indices {
                let winner_player_idx = seats_in_pot[winner_idx];
                let (_, winner_investment) =
                    pot.investments.remove_entry(&winner_player_idx).unwrap();
                if winner_investment > max_winner_investment {
                    max_winner_investment = winner_investment;
                }
                money_per_winner.insert(winner_player_idx, winner_investment);
                pot.size -= winner_investment;
            }
            for (player_idx, investment) in pot.investments {
                if investment > max_winner_investment {
                    let remainder = investment - max_winner_investment;
                    distributions_per_player.insert(player_idx, remainder);
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
            for (winner_player_idx, money) in money_per_winner {
                distributions_per_player.insert(winner_player_idx, money + pot_split);
                pot_remainder -= pot_split as Usdf;
            }
            value.data.donations += pot_remainder;

            // Give money back to players.
            for (player_idx, distribution) in distributions_per_player {
                let player = &mut value.data.players[player_idx];
                player.user.money += distribution;
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
    fn from(mut value: Game<ShowHands>) -> Self {
        value.data.num_players_active = 0;
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
impl From<Game<DivideDonations>> for Game<UpdateBlinds> {
    fn from(mut value: Game<DivideDonations>) -> Self {
        let num_users = value.get_num_users();
        if num_users > 0 {
            let donation_per_user = value.data.donations as Usd / num_users as Usd;
            for user in value
                .data
                .spectators
                .iter_mut()
                .map(|(_, u)| u)
                .chain(value.data.waitlist.iter_mut())
                .chain(value.data.players.iter_mut().map(|p| &mut p.user))
            {
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
/// is larger than a multiple of the blind. If it is, blinds are multiplied
/// by that multiple. This helps progress the game, increasing the investment
/// each player must make in each hand, preventing games where a handful of
/// players have large stacks and can afford to fold many times without
/// any other action.
impl From<Game<UpdateBlinds>> for Game<BootPlayers> {
    fn from(mut value: Game<UpdateBlinds>) -> Self {
        let min_money = value
            .data
            .spectators
            .values()
            .map(|u| u.money)
            .chain(value.data.waitlist.iter().map(|u| u.money))
            .chain(value.data.players.iter().map(|p| p.user.money))
            .filter(|money| *money >= value.data.big_blind)
            .min()
            .unwrap_or(Usd::MAX);
        if min_money < Usd::MAX {
            let multiple = max(1, min_money / value.data.settings.starting_stack);
            value.data.small_blind = multiple * value.data.settings.min_small_blind;
            value.data.big_blind = multiple * value.data.settings.min_big_blind;
        }
        Self {
            data: value.data,
            state: BootPlayers {},
        }
    }
}

/// Spectate players that don't have enough money to satisfy the big blind
/// from seats, and reset player states for players that do have enough
/// money to play.
impl From<Game<BootPlayers>> for Game<Lobby> {
    fn from(mut value: Game<BootPlayers>) -> Self {
        value.data.board.clear();
        for player in value.data.players.iter_mut() {
            if player.user.money < value.data.big_blind {
                value.data.open_seats.push_back(player.seat_idx);
                value
                    .data
                    .players_to_spectate
                    .insert(player.user.name.clone());
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

    use crate::poker::entities::{Action, Card, Suit};
    use crate::poker::game::{
        DistributePot, DivideDonations, Lobby, RemovePlayers, TakeAction, UpdateBlinds,
    };

    use super::{
        BootPlayers, CollectBlinds, Deal, Flop, Game, MoveButton, River, SeatPlayers, ShowHands,
        Turn, UserError,
    };

    fn init_game() -> Game<SeatPlayers> {
        let game = Game::<Lobby>::new();
        let mut game: Game<SeatPlayers> = game.into();
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
        for (i, blind) in [
            0,
            game.data.settings.min_small_blind,
            game.data.settings.min_big_blind,
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(
                game.data.players[i].user.money,
                game.data.settings.starting_stack - blind
            );
        }
    }

    #[test]
    fn deal() {
        let game = init_game_at_deal();
        assert_eq!(game.get_num_community_cards(), 0);
        assert_eq!(game.data.deck_idx, 2 * game.get_num_users());
        for player in game.data.players.iter() {
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
        // (as 1) counts as a high ace as well, so seat 1 wins
        // the showdown with a higher flush.
        game.data.board = vec![
            Card(4, Suit::Diamond),
            Card(5, Suit::Diamond),
            Card(6, Suit::Diamond),
            Card(7, Suit::Diamond),
        ];
        game.data.players[1].cards = vec![Card(1, Suit::Diamond), Card(7, Suit::Heart)];
        game.data.players[2].cards = vec![Card(2, Suit::Diamond), Card(5, Suit::Heart)];
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for (i, money) in [
            game.data.settings.starting_stack,
            2 * game.data.settings.starting_stack,
            0,
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(game.data.players[i].user.money, *money);
        }
    }

    #[test]
    fn early_showdown_2_winners() {
        let mut game = init_game_at_showdown_with_2_all_ins();
        game.data.board = vec![
            Card(2, Suit::Diamond),
            Card(4, Suit::Diamond),
            Card(5, Suit::Diamond),
            Card(6, Suit::Diamond),
            Card(7, Suit::Diamond),
        ];
        game.data.players[1].cards = vec![Card(1, Suit::Heart), Card(7, Suit::Heart)];
        game.data.players[2].cards = vec![Card(2, Suit::Heart), Card(5, Suit::Heart)];
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for i in 0..3 {
            assert_eq!(
                game.data.players[i].user.money,
                game.data.settings.starting_stack
            );
        }
    }

    #[test]
    fn early_showdown_3_decreasing_all_ins() {
        let game = init_game();
        let mut game: Game<MoveButton> = game.into();
        for i in 0..3 {
            game.data.players[i].user.money = game.data.settings.starting_stack * (3 - i as u32);
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
            Card(1, Suit::Spade),
            Card(4, Suit::Diamond),
            Card(5, Suit::Diamond),
            Card(6, Suit::Diamond),
            Card(7, Suit::Diamond),
        ];
        game.data.players[0].cards = vec![Card(3, Suit::Heart), Card(1, Suit::Diamond)];
        game.data.players[1].cards = vec![Card(1, Suit::Heart), Card(10, Suit::Diamond)];
        game.data.players[2].cards = vec![Card(2, Suit::Heart), Card(9, Suit::Diamond)];
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for (i, money) in [6 * game.data.settings.starting_stack, 0, 0]
            .iter()
            .enumerate()
        {
            assert_eq!(game.data.players[i].user.money, *money);
        }
    }

    #[test]
    fn early_showdown_3_increasing_all_ins() {
        let game = init_game();
        let mut game: Game<MoveButton> = game.into();
        for i in 0..3 {
            game.data.players[i].user.money = game.data.settings.starting_stack * (i as u32 + 1);
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
            Card(1, Suit::Spade),
            Card(4, Suit::Diamond),
            Card(5, Suit::Diamond),
            Card(6, Suit::Diamond),
            Card(7, Suit::Diamond),
        ];
        game.data.players[0].cards = vec![Card(3, Suit::Heart), Card(1, Suit::Diamond)];
        game.data.players[1].cards = vec![Card(1, Suit::Heart), Card(10, Suit::Diamond)];
        game.data.players[2].cards = vec![Card(2, Suit::Heart), Card(9, Suit::Diamond)];
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for (i, money) in [
            3 * game.data.settings.starting_stack,
            2 * game.data.settings.starting_stack,
            game.data.settings.starting_stack,
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(game.data.players[i].user.money, *money);
        }
    }

    #[test]
    fn manipulating_user_in_lobby() {
        let mut game = Game::<SeatPlayers>::new();
        let username = "ognf";

        game.new_user(username).unwrap();
        assert!(game.contains_spectator(username));

        assert_eq!(game.new_user(username), Err(UserError::UserAlreadyExists));

        game.waitlist_user(username).unwrap();
        assert!(game.contains_waitlister(username));

        game.spectate_user(username).unwrap();
        assert!(game.contains_spectator(username));

        game.remove_user(username).unwrap();
        assert!(!game.contains_user(username));

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
        assert!(game.contains_spectator(username));

        game.waitlist_user(username).unwrap();
        assert!(game.contains_waitlister(username));

        game.remove_user(username).unwrap();
        assert!(!game.contains_user(username));

        for i in 0..game.data.settings.max_users {
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
        let game = Game::<Lobby>::new();
        let mut game: Game<SeatPlayers> = game.into();
        for i in 0..game.data.settings.max_users {
            let username = i.to_string();
            game.new_user(&username).unwrap();
            game.waitlist_user(&username).unwrap();
        }
        let game: Game<MoveButton> = game.into();
        let mut game: Game<CollectBlinds> = game.into();
        for i in 3..game.get_num_players() {
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
            Card(1, Suit::Spade),
            Card(4, Suit::Diamond),
            Card(5, Suit::Diamond),
            Card(6, Suit::Diamond),
            Card(7, Suit::Diamond),
        ];
        game.data.players[0].cards = vec![Card(3, Suit::Heart), Card(8, Suit::Diamond)];
        game.data.players[1].cards = vec![Card(1, Suit::Heart), Card(7, Suit::Heart)];
        game.data.players[2].cards = vec![Card(2, Suit::Heart), Card(5, Suit::Heart)];
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<RemovePlayers> = game.into();
        let game: Game<DivideDonations> = game.into();
        let game: Game<UpdateBlinds> = game.into();
        let game: Game<BootPlayers> = game.into();
        assert_eq!(game.data.big_blind, 3 * game.data.settings.min_big_blind);
        let game: Game<Lobby> = game.into();
        assert_eq!(game.get_num_players(), 1);
    }

    #[test]
    fn remove_player() {
        let mut game = init_game_at_showdown_with_2_all_ins();
        game.data.board = vec![
            Card(2, Suit::Diamond),
            Card(4, Suit::Diamond),
            Card(5, Suit::Diamond),
            Card(6, Suit::Diamond),
            Card(7, Suit::Diamond),
        ];
        game.data.players[1].cards = vec![Card(1, Suit::Heart), Card(7, Suit::Heart)];
        game.data.players[2].cards = vec![Card(2, Suit::Heart), Card(5, Suit::Heart)];
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<RemovePlayers> = game.into();
        let mut game: Game<DivideDonations> = game.into();
        game.remove_user("0").unwrap();
        assert!(!game.contains_user("0"));
        assert!(game.contains_player("1"));
        assert!(game.contains_player("2"));
        let game: Game<UpdateBlinds> = game.into();
        for i in 0..2 {
            assert_eq!(
                game.data.players[i].user.money,
                game.data.settings.starting_stack + game.data.settings.starting_stack / 2
            );
        }
        let mut expected_open_seats = Vec::from_iter(3..game.data.settings.max_players);
        expected_open_seats.push(0);
        assert_eq!(game.data.open_seats, expected_open_seats)
    }

    #[test]
    fn remove_player_with_queue() {
        let mut game = init_game_at_showdown_with_2_all_ins();
        game.data.board = vec![
            Card(2, Suit::Diamond),
            Card(4, Suit::Diamond),
            Card(5, Suit::Diamond),
            Card(6, Suit::Diamond),
            Card(7, Suit::Diamond),
        ];
        game.data.players[1].cards = vec![Card(1, Suit::Heart), Card(7, Suit::Heart)];
        game.data.players[2].cards = vec![Card(2, Suit::Heart), Card(5, Suit::Heart)];
        game.remove_user("0").unwrap();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<RemovePlayers> = game.into();
        let game: Game<DivideDonations> = game.into();
        assert!(!game.contains_user("0"));
        assert!(game.contains_player("1"));
        assert!(game.contains_player("2"));
        let game: Game<UpdateBlinds> = game.into();
        for i in 0..2 {
            assert_eq!(
                game.data.players[i].user.money,
                game.data.settings.starting_stack + game.data.settings.starting_stack / 2
            );
        }
        let mut expected_open_seats = Vec::from_iter(3..game.data.settings.max_players);
        expected_open_seats.push(0);
        assert_eq!(game.data.open_seats, expected_open_seats)
    }

    #[test]
    fn seat_players() {
        let game = init_game_at_seat_players();
        assert_eq!(game.data.num_players_active, game.get_num_players());
        assert!(game.contains_player("0"));
        assert!(game.contains_player("1"));
        assert!(game.contains_player("2"));
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
