use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::{
    cmp::{max, min, Ordering},
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    fmt,
};
use thiserror::Error;

pub mod constants;
pub mod entities;
pub mod functional;

use constants::{DEFAULT_MAX_USERS, MAX_PLAYERS};
use entities::{
    Action, Bet, BetAction, Card, GameView, GameViews, Player, PlayerState, PlayerView, Pot,
    PotView, SubHand, Usd, Usdf, User, DEFAULT_BUY_IN, DEFAULT_MIN_BIG_BLIND,
    DEFAULT_MIN_SMALL_BLIND,
};

#[derive(Debug, Deserialize, Eq, Error, PartialEq, Serialize)]
pub enum UserError {
    #[error("can't show hand")]
    CannotShowHand,
    #[error("can't start unless you're waitlisted or a player")]
    CannotStartGame,
    #[error("game is full")]
    CapacityReached,
    #[error("game already in progress")]
    GameAlreadyInProgress,
    #[error("game already starting")]
    GameAlreadyStarting,
    #[error("need >= ${big_blind} for the big blind")]
    InsufficientFunds { big_blind: Usd },
    #[error("{action} is invalid")]
    InvalidAction { action: Action },
    #[error("illegal {bet}")]
    InvalidBet { bet: Bet },
    #[error("need 2+ players")]
    NotEnoughPlayers,
    #[error("not your turn")]
    OutOfTurnAction,
    #[error("user already exists")]
    UserAlreadyExists,
    #[error("user does not exist")]
    UserDoesNotExist,
    #[error("not playing")]
    UserNotPlaying,
    #[error("already showing hand")]
    UserAlreadyShowingHand,
}

#[derive(Debug)]
pub struct GameSettings {
    pub buy_in: Usd,
    pub min_big_blind: Usd,
    pub min_small_blind: Usd,
    pub max_players: usize,
    pub max_users: usize,
}

impl GameSettings {
    pub fn new(max_players: usize, max_users: usize, buy_in: Usd) -> Self {
        let min_big_blind = buy_in / 20;
        let min_small_blind = min_big_blind / 2;
        Self {
            buy_in,
            min_big_blind,
            min_small_blind,
            max_players,
            max_users,
        }
    }
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            buy_in: DEFAULT_BUY_IN,
            min_big_blind: DEFAULT_MIN_BIG_BLIND,
            min_small_blind: DEFAULT_MIN_SMALL_BLIND,
            max_players: MAX_PLAYERS,
            max_users: DEFAULT_MAX_USERS,
        }
    }
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
    pub pot: Pot,
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
    settings: GameSettings,
}

impl GameData {
    fn new() -> Self {
        let settings = GameSettings::default();
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
            pot: Pot::new(settings.max_players),
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

impl From<GameSettings> for GameData {
    fn from(value: GameSettings) -> Self {
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
            pot: Pot::new(value.max_players),
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

impl Default for Lobby {
    fn default() -> Self {
        Self::new()
    }
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

impl Default for ShowHands {
    fn default() -> Self {
        Self::new()
    }
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
    pub fn action_options_to_string(action_options: &HashSet<Action>) -> String {
        let num_options = action_options.len();
        action_options
            .iter()
            .enumerate()
            .map(|(i, action)| {
                let repr = action.to_option_string();
                match i {
                    0 if num_options == 1 => repr,
                    0 if num_options == 2 => format!("{repr} "),
                    0 if num_options >= 3 => format!("{repr}, "),
                    i if i == num_options - 1 && num_options != 1 => format!("or {repr}"),
                    _ => format!("{repr}, "),
                }
            })
            .collect::<Vec<_>>()
            .join("")
    }

    fn as_view(&self, username: &str) -> GameView {
        let mut players = Vec::with_capacity(self.data.settings.max_players);
        for player in self.data.players.iter() {
            let cards = if player.user.name == username || player.state == PlayerState::Show {
                player.cards.clone()
            } else {
                vec![]
            };
            let player_view = PlayerView {
                user: player.user.clone(),
                state: player.state.clone(),
                cards,
            };
            players.push(player_view);
        }
        // Action index doesn't matter if the turn is being transitioned.
        let next_action_idx = if self.is_ready_for_next_phase() {
            None
        } else {
            self.data.next_action_idx
        };
        GameView {
            donations: self.data.donations,
            small_blind: self.data.small_blind,
            big_blind: self.data.big_blind,
            spectators: self.data.spectators.clone(),
            waitlist: self.data.waitlist.clone(),
            open_seats: self.data.open_seats.clone(),
            players,
            board: self.data.board.clone(),
            pot: PotView {
                size: self.data.pot.get_size(),
            },
            small_blind_idx: self.data.small_blind_idx,
            big_blind_idx: self.data.big_blind_idx,
            next_action_idx,
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

    pub fn contains_spectator(&self, username: &str) -> bool {
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
                let mut action_options = HashSet::from([Action::Fold]);
                let user = &self.data.players[action_idx].user;
                let raise = self.data.pot.get_min_raise_by_player_idx(action_idx);
                let call = self.data.pot.get_call_by_player_idx(action_idx);
                if self.data.num_players_active > 1 || call >= user.money {
                    action_options.insert(Action::AllIn);
                }
                if call > 0 && call < user.money {
                    action_options.insert(Action::Call(call));
                } else if call == 0 {
                    action_options.insert(Action::Check);
                }
                if self.data.num_players_active > 1 && user.money > raise {
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

    /// Return the number of pots, indicating whether to continue
    /// showing player hands and distributing the pots, or whether
    /// to move on to other post-game phases.
    pub fn get_num_pots(&self) -> usize {
        let unique_investments: HashSet<_> = self
            .data
            .pot
            .investments
            .iter()
            .filter(|(player_idx, _)| {
                let player = &self.data.players[**player_idx];
                player.state != PlayerState::Fold
            })
            .map(|(_, investment)| *investment)
            .collect();
        unique_investments.len()
    }

    fn get_num_users(&self) -> usize {
        self.data.spectators.len() + self.data.waitlist.len() + self.data.players.len()
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

    pub fn is_pot_empty(&self) -> bool {
        self.data.pot.is_empty()
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
                    && self.data.pot.get_call_by_player_idx(action_idx) == 0
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
                money: self.data.settings.buy_in,
            },
        );
        Ok(true)
    }

    /// Reset the next action index and return the possible actions
    /// for that player. This should be called prior to each game phase
    /// in preparation for a new round of betting.
    fn prepare_for_next_phase(&mut self) -> Option<HashSet<Action>> {
        self.data.num_players_called = 0;
        // Reset player states for players that are still in the hand.
        for player in self.data.players.iter_mut().filter(|player| {
            matches!(
                player.state,
                PlayerState::Call | PlayerState::Check | PlayerState::Raise
            )
        }) {
            player.state = PlayerState::Wait
        }
        self.data.next_action_idx = Some(self.data.starting_action_idx);
        self.data.next_action_idx = self.get_next_action_idx(true);
        self.get_next_action_options()
    }

    fn redistribute_user_money(&mut self, money: &mut Usd) {
        self.data.donations += (*money as Usdf) - (self.data.settings.buy_in as Usdf);
        *money = 0;
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
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister exists")
                } else if let Some(player_idx) = self.data.players.iter().position(|p| p.user.name == username) {
                    self.data.players_to_spectate.remove(username);
                    let player = self.data.players.remove(player_idx);
                    self.data.open_seats.push_back(player.seat_idx);
                    player.user
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.redistribute_user_money(&mut user.money);
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
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister exists")
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
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister exists")
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
                self.redistribute_user_money(&mut user.money);
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
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister exists")
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

impl From<GameSettings> for Game<Lobby> {
    fn from(value: GameSettings) -> Self {
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
        while !value.data.open_seats.is_empty() && !value.data.waitlist.is_empty() {
            let open_seat_idx = value.data.open_seats.pop_front().expect("not empty");
            let user = value.data.waitlist.pop_front().expect("not empty");
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
        let player_indices = value
            .data
            .players
            .iter()
            .enumerate()
            .map(|(player_idx, _)| player_idx);
        // Search for the big blind and starting positions.
        let mut seats = player_indices
            .clone()
            .cycle()
            .skip(value.data.big_blind_idx + 1);
        value.data.big_blind_idx = seats.next().expect("big blind position exists");
        value.data.starting_action_idx = seats.next().expect("starting action position exists");
        value.data.next_action_idx = Some(value.data.starting_action_idx);
        // Reverse the table search to find the small blind position relative
        // to the big blind position since the small blind must always trail the big
        // blind.
        let mut seats = player_indices
            .rev()
            .cycle()
            .skip(num_players - value.data.big_blind_idx);
        value.data.small_blind_idx = seats.next().expect("small blind position exists");
        Self {
            data: value.data,
            state: CollectBlinds {},
        }
    }
}

/// Collect blinds, initializing the main pot.
impl From<Game<CollectBlinds>> for Game<Deal> {
    fn from(mut value: Game<CollectBlinds>) -> Self {
        value.data.pot = Pot::new(value.data.settings.max_players);
        for (player_idx, blind) in [
            (value.data.small_blind_idx, value.data.small_blind),
            (value.data.big_blind_idx, value.data.big_blind),
        ] {
            let player = &mut value.data.players[player_idx];
            let bet = match player.user.money.cmp(&blind) {
                Ordering::Equal => {
                    player.state = PlayerState::AllIn;
                    value.data.num_players_active -= 1;
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
                    "a player can't be in a game if they don't have enough for the big blind"
                ),
            };
            // Impossible for a side pot to be created from the blinds, so
            // we don't even need to check.
            value.data.pot.bet(player_idx, &bet);
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
            let deal_idx = seats.next().expect("dealing position exists");
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
    pub fn act(&mut self, action: Action) -> Result<Action, UserError> {
        let sanitized_action = self.affect(action)?;
        self.data.next_action_idx = self.get_next_action_idx(false);
        self.state.action_options = self.get_next_action_options();
        Ok(sanitized_action)
    }

    fn affect(&mut self, action: Action) -> Result<Action, UserError> {
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
                        player.state = PlayerState::Check;
                        return Ok(action);
                    }
                    Action::Fold => {
                        self.data.num_players_active -= 1;
                        player.state = PlayerState::Fold;
                        return Ok(action);
                    }
                    Action::Raise(amount) => Bet {
                        action: BetAction::Raise,
                        amount,
                    },
                };
                if bet.amount >= player.user.money {
                    bet.action = BetAction::AllIn;
                    bet.amount = player.user.money;
                }
                // Do some additional bet validation based on the bet's amount.
                let call = self.data.pot.get_call();
                let investment = self.data.pot.get_investment_by_player_idx(player_idx);
                let new_investment = investment + bet.amount;
                match bet.action {
                    BetAction::AllIn => {
                        self.data.num_players_active -= 1;
                        if new_investment > call {
                            self.data.num_players_called = 0;
                        }
                        player.state = PlayerState::AllIn;
                    }
                    BetAction::Call => {
                        if new_investment != call {
                            return Err(UserError::InvalidBet { bet });
                        }
                        self.data.num_players_called += 1;
                        player.state = PlayerState::Call;
                    }
                    BetAction::Raise => {
                        if new_investment < (2 * call) {
                            return Err(UserError::InvalidBet { bet });
                        }
                        self.data.num_players_called = 1;
                        player.state = PlayerState::Raise;
                    }
                }
                // The player's bet is OK. Remove the bet amount from the player's
                // stack and start distributing it appropriately amongst all the pots.
                player.user.money -= bet.amount;
                self.data.pot.bet(player_idx, &bet);

                // Reset other player states that're still in the hand based on the bet.
                if self.data.num_players_called <= 1 {
                    for player in self
                        .data
                        .players
                        .iter_mut()
                        .enumerate()
                        .filter(|(idx, player)| {
                            matches!(player.state,
                            PlayerState::Call | PlayerState::Check | PlayerState::Raise
                                if *idx != player_idx)
                        })
                        .map(|(_, player)| player)
                    {
                        player.state = PlayerState::Wait
                    }
                }

                // Return the santized action.
                Ok(bet.into())
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

macro_rules! impl_show_hands {
    ($($t:ty),+) => {
        $(impl $t {
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
        })*
    }
}

impl_show_hands!(
    Game<ShowHands>,
    Game<DistributePot>,
    Game<RemovePlayers>,
    Game<DivideDonations>,
    Game<UpdateBlinds>
);

impl From<Game<ShowHands>> for Game<DistributePot> {
    fn from(mut value: Game<ShowHands>) -> Self {
        let num_players_remaining: usize = value
            .data
            .players
            .iter()
            .map(|p| if p.state == PlayerState::Fold { 0 } else { 1 })
            .sum();
        if num_players_remaining > 1 {
            for player_idx in value.data.pot.investments.keys() {
                let player = &mut value.data.players[*player_idx];
                if player.state != PlayerState::Fold {
                    player.state = PlayerState::Show
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

impl Game<DistributePot> {
    /// Get all players in the pot that haven't folded and compare their
    /// hands to one another. Get the winning indices and distribute
    /// the pot accordingly.
    fn distribute(&mut self) {
        let mut investments = Vec::from_iter(self.data.pot.investments.iter_mut());
        investments
            .sort_unstable_by(|(_, investment1), (_, investment2)| investment1.cmp(investment2));
        if let Some((_, largest_call)) = investments.last() {
            // Get the pot size and the player indices in the pot.
            let mut pot_idx = investments.len() - 1;
            let mut pot_call = **largest_call;
            for (idx, (player_idx, investment)) in investments.iter().enumerate().rev() {
                let player = &self.data.players[**player_idx];
                if player.state != PlayerState::Fold && investment < largest_call {
                    pot_call = **largest_call - **investment;
                    break;
                }
                pot_idx = idx;
            }

            // Evaluate the hands in the pot and get the winners.
            let mut pot_size: Usd = 0;
            let mut seats_in_pot = Vec::with_capacity(self.data.settings.max_players);
            let mut hands_in_pot = Vec::with_capacity(self.data.settings.max_players);
            for (player_idx, investment) in investments[pot_idx..].as_mut() {
                let pot_investment = min(pot_call, **investment);
                pot_size += pot_investment;
                **investment -= pot_investment;
                let player = &mut self.data.players[**player_idx];
                if player.state != PlayerState::Fold {
                    seats_in_pot.push(*player_idx);
                    let hand_eval = || {
                        let mut cards = player.cards.clone();
                        cards.extend(self.data.board.clone());
                        functional::prepare_hand(&mut cards);
                        functional::eval(&cards)
                    };
                    let hand = self
                        .state
                        .hand_eval_cache
                        .entry(**player_idx)
                        .or_insert_with(hand_eval);
                    hands_in_pot.push(hand.clone());
                }
            }
            let winner_indices = functional::argmax(&hands_in_pot);

            // Finally, split the pot amongst all the winners. There's
            // a possibility for the pot to not split perfectly
            // amongst all players; in this case, the remainder is
            // put in the donations and will eventually be redistributed
            // amongst remaining users. This also encourages users to
            // stay in the game so they can be donated these breadcrumbs
            // and continue playing with them.
            let num_winners = winner_indices.len();
            let pot_split = pot_size / num_winners as Usd;
            let mut pot_remainder = pot_size as Usdf;
            for winner_idx in winner_indices {
                let winner_player_idx = seats_in_pot[winner_idx];
                let player = &mut self.data.players[*winner_player_idx];
                player.user.money += pot_split;
                pot_remainder -= pot_split as Usdf;
            }
            self.data.donations += pot_remainder;
        }

        // Remove null investments.
        self.data
            .pot
            .investments
            .retain(|_, investment| *investment > 0);
    }
}

impl From<Game<DistributePot>> for Game<ShowHands> {
    fn from(mut value: Game<DistributePot>) -> Self {
        value.distribute();
        Self {
            data: value.data,
            state: ShowHands {
                hand_eval_cache: value.state.hand_eval_cache,
            },
        }
    }
}

impl From<Game<DistributePot>> for Game<RemovePlayers> {
    fn from(mut value: Game<DistributePot>) -> Self {
        value.distribute();
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
            // It is possible for a user to leave in this state but right before
            // this state transition occurs. That'd cause this method to return
            // an error, but it's really OK if they left since they were going
            // to be removed anyways.
            value.remove_user(&username).ok();
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
        if num_users > 0 && value.data.donations > 0 as Usdf {
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
            let multiple = max(1, min_money / value.data.settings.buy_in);
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
            // It is possible for a user to leave in this state but right before
            // this state transition occurs. That'd cause this method to return
            // an error, but it's really OK if they left since spectating them
            // is a softer action.
            value.spectate_user(&username).ok();
        }
        Self {
            data: value.data,
            state: Lobby::new(),
        }
    }
}

#[derive(Debug)]
pub enum PokerState {
    Lobby(Game<Lobby>),
    SeatPlayers(Game<SeatPlayers>),
    MoveButton(Game<MoveButton>),
    CollectBlinds(Game<CollectBlinds>),
    Deal(Game<Deal>),
    TakeAction(Game<TakeAction>),
    Flop(Game<Flop>),
    Turn(Game<Turn>),
    River(Game<River>),
    ShowHands(Game<ShowHands>),
    DistributePot(Game<DistributePot>),
    RemovePlayers(Game<RemovePlayers>),
    DivideDonations(Game<DivideDonations>),
    UpdateBlinds(Game<UpdateBlinds>),
    BootPlayers(Game<BootPlayers>),
}

impl Default for PokerState {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PokerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match &self {
            PokerState::Lobby(_) => "in lobby",
            PokerState::SeatPlayers(_) => "seating players",
            PokerState::MoveButton(_) => "moving button",
            PokerState::CollectBlinds(ref game) => {
                let big_blind = game.data.big_blind;
                let big_blind_username = &game.data.players[game.data.big_blind_idx].user.name;
                let small_blind = game.data.small_blind;
                let small_blind_username = &game.data.players[game.data.small_blind_idx].user.name;
                &format!("collecting ${big_blind} from {big_blind_username} and ${small_blind} from {small_blind_username}")
            }
            PokerState::Deal(_) => "dealing cards",
            PokerState::TakeAction(ref game) => {
                if game.is_ready_for_next_phase() {
                    "end of betting round"
                } else {
                    "betting round transition"
                }
            }
            PokerState::Flop(_) => "the flop",
            PokerState::Turn(_) => "the turn",
            PokerState::River(_) => "the river",
            PokerState::ShowHands(ref game) => {
                let num_pots = game.get_num_pots();
                match num_pots {
                    1 => "showing main pot",
                    i => &format!("showing side pot #{}", i - 1),
                }
            }
            PokerState::DistributePot(ref game) => {
                let num_pots = game.get_num_pots();
                match num_pots {
                    1 => "distributing main pot",
                    i => &format!("distributing side pot #{}", i - 1),
                }
            }
            PokerState::RemovePlayers(_) => "updating players that joined spectators or left",
            PokerState::DivideDonations(_) => "dividing donations",
            PokerState::UpdateBlinds(_) => "updating blinds",
            PokerState::BootPlayers(_) => "spectating players that can't afford the big blind",
        };
        write!(f, "{repr}")
    }
}

impl PokerState {
    pub fn get_action_options(&self) -> Option<HashSet<Action>> {
        match self {
            PokerState::TakeAction(ref game) => game.get_action_options(),
            _ => None,
        }
    }

    pub fn get_next_action_username(&self) -> Option<String> {
        match self {
            PokerState::TakeAction(ref game) => game.get_next_action_username(),
            _ => None,
        }
    }

    pub fn get_views(&self) -> GameViews {
        match self {
            PokerState::Lobby(ref game) => game.get_views(),
            PokerState::SeatPlayers(ref game) => game.get_views(),
            PokerState::MoveButton(ref game) => game.get_views(),
            PokerState::CollectBlinds(ref game) => game.get_views(),
            PokerState::Deal(ref game) => game.get_views(),
            PokerState::TakeAction(ref game) => game.get_views(),
            PokerState::Flop(ref game) => game.get_views(),
            PokerState::Turn(ref game) => game.get_views(),
            PokerState::River(ref game) => game.get_views(),
            PokerState::ShowHands(ref game) => game.get_views(),
            PokerState::DistributePot(ref game) => game.get_views(),
            PokerState::RemovePlayers(ref game) => game.get_views(),
            PokerState::DivideDonations(ref game) => game.get_views(),
            PokerState::UpdateBlinds(ref game) => game.get_views(),
            PokerState::BootPlayers(ref game) => game.get_views(),
        }
    }

    pub fn init_start(&mut self, username: &str) -> Result<(), UserError> {
        match self {
            PokerState::Lobby(ref mut game) => {
                if game.contains_waitlister(username) || game.contains_player(username) {
                    game.init_start()?;
                    Ok(())
                } else {
                    Err(UserError::CannotStartGame)
                }
            }
            PokerState::SeatPlayers(_) => Err(UserError::GameAlreadyStarting),
            _ => Err(UserError::GameAlreadyInProgress),
        }
    }

    pub fn new() -> Self {
        let game = Game::<Lobby>::new();
        PokerState::Lobby(game)
    }

    fn phase_transition(game: Game<TakeAction>) -> PokerState {
        match game.get_num_community_cards() {
            0 => PokerState::Flop(game.into()),
            3 => PokerState::Turn(game.into()),
            4 => PokerState::River(game.into()),
            5 => PokerState::ShowHands(game.into()),
            _ => unreachable!(
                "there can only be 0, 3, 4, or 5 community cards on the board at a time"
            ),
        }
    }

    pub fn show_hand(&mut self, username: &str) -> Result<(), UserError> {
        match self {
            PokerState::ShowHands(ref mut game) => {
                game.show_hand(username)?;
                Ok(())
            }
            PokerState::DistributePot(ref mut game) => {
                game.show_hand(username)?;
                Ok(())
            }
            PokerState::RemovePlayers(ref mut game) => {
                game.show_hand(username)?;
                Ok(())
            }
            PokerState::UpdateBlinds(ref mut game) => {
                game.show_hand(username)?;
                Ok(())
            }
            _ => Err(UserError::CannotShowHand),
        }
    }

    pub fn step(self) -> Self {
        match self {
            PokerState::Lobby(game) => {
                if game.is_ready_to_start() {
                    PokerState::SeatPlayers(game.into())
                } else {
                    PokerState::Lobby(game)
                }
            }
            PokerState::SeatPlayers(game) => {
                if game.get_num_potential_players() >= 2 {
                    PokerState::MoveButton(game.into())
                } else {
                    PokerState::Lobby(game.into())
                }
            }
            PokerState::MoveButton(game) => PokerState::CollectBlinds(game.into()),
            PokerState::CollectBlinds(game) => PokerState::Deal(game.into()),
            PokerState::Deal(game) => PokerState::TakeAction(game.into()),
            PokerState::TakeAction(mut game) => {
                if game.is_ready_for_next_phase() {
                    PokerState::phase_transition(game)
                } else {
                    game.act(Action::Fold).expect("force folding is OK");
                    if game.is_ready_for_next_phase() {
                        PokerState::phase_transition(game)
                    } else {
                        PokerState::TakeAction(game)
                    }
                }
            }
            PokerState::Flop(game) => {
                if game.is_ready_for_showdown() {
                    PokerState::Turn(game.into())
                } else {
                    PokerState::TakeAction(game.into())
                }
            }
            PokerState::Turn(game) => {
                if game.is_ready_for_showdown() {
                    PokerState::River(game.into())
                } else {
                    PokerState::TakeAction(game.into())
                }
            }
            PokerState::River(game) => {
                if game.is_ready_for_showdown() {
                    PokerState::ShowHands(game.into())
                } else {
                    PokerState::TakeAction(game.into())
                }
            }
            PokerState::ShowHands(game) => PokerState::DistributePot(game.into()),
            PokerState::DistributePot(game) => {
                if game.get_num_pots() >= 2 {
                    PokerState::ShowHands(game.into())
                } else {
                    PokerState::RemovePlayers(game.into())
                }
            }
            PokerState::RemovePlayers(game) => PokerState::DivideDonations(game.into()),
            PokerState::DivideDonations(game) => PokerState::UpdateBlinds(game.into()),
            PokerState::UpdateBlinds(game) => PokerState::BootPlayers(game.into()),
            PokerState::BootPlayers(game) => PokerState::Lobby(game.into()),
        }
    }

    pub fn take_action(&mut self, username: &str, action: Action) -> Result<Action, UserError> {
        match self {
            PokerState::TakeAction(ref mut game)
                if !game.is_ready_for_next_phase() && game.is_turn(username) =>
            {
                let sanitized_action = game.act(action)?;
                Ok(sanitized_action)
            }
            _ => Err(UserError::OutOfTurnAction),
        }
    }
}

macro_rules! impl_user_managers {
    ($($name:ident),+) => {
        impl PokerState {
            $(pub fn $name(&mut self, username: &str) -> Result<(), UserError> {
                match self {
                    PokerState::Lobby(ref mut game) => {
                        game.$name(username)?;
                    },
                    PokerState::SeatPlayers(ref mut game) => {
                        game.$name(username)?;
                    },
                    PokerState::MoveButton(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::CollectBlinds(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::Deal(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::TakeAction(ref mut game) => {
                        game.$name(username)?;
                    },
                    PokerState::Flop(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::Turn(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::River(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::ShowHands(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::DistributePot(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::RemovePlayers(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::DivideDonations(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::UpdateBlinds(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::BootPlayers(ref mut game) => {
                        game.$name(username)?;
                    },
                }
                Ok(())
            })*
        }
    }
}

impl_user_managers!(new_user, remove_user, spectate_user, waitlist_user);

impl From<GameSettings> for PokerState {
    fn from(value: GameSettings) -> Self {
        let game: Game<Lobby> = value.into();
        PokerState::Lobby(game)
    }
}

#[cfg(test)]
mod game_tests {
    use std::collections::HashSet;

    use crate::entities::PlayerState;

    use super::{
        entities::{Action, Card, Suit},
        BootPlayers, CollectBlinds, Deal, DistributePot, DivideDonations, Flop, Game, Lobby,
        MoveButton, RemovePlayers, River, SeatPlayers, ShowHands, TakeAction, Turn, UpdateBlinds,
        UserError,
    };

    fn init_2_player_game() -> Game<SeatPlayers> {
        let game = Game::<Lobby>::new();
        let mut game: Game<SeatPlayers> = game.into();
        for i in 0..2 {
            let username = i.to_string();
            game.new_user(&username).unwrap();
            game.waitlist_user(&username).unwrap();
        }
        game
    }

    fn init_3_player_game() -> Game<SeatPlayers> {
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
        let game = init_3_player_game();
        let game: Game<MoveButton> = game.into();
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        game
    }

    fn init_game_at_deal() -> Game<TakeAction> {
        let game = init_3_player_game();
        let game: Game<MoveButton> = game.into();
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let game: Game<TakeAction> = game.into();
        game
    }

    fn init_game_at_move_button() -> Game<CollectBlinds> {
        let game = init_3_player_game();
        let game: Game<MoveButton> = game.into();
        let game: Game<CollectBlinds> = game.into();
        game
    }

    fn init_game_at_seat_players() -> Game<MoveButton> {
        let game = init_3_player_game();
        let game: Game<MoveButton> = game.into();
        game
    }

    fn init_game_at_showdown_with_1_all_in() -> Game<ShowHands> {
        let mut game = init_game_at_deal();
        game.act(Action::AllIn).unwrap();
        game.act(Action::Fold).unwrap();
        game.act(Action::Fold).unwrap();
        let game: Game<Flop> = game.into();
        let game: Game<Turn> = game.into();
        let game: Game<River> = game.into();
        let game: Game<ShowHands> = game.into();
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
                game.data.settings.buy_in - blind
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
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        let game: Game<Flop> = game.into();
        let game: Game<Turn> = game.into();
        assert_eq!(game.get_num_community_cards(), 3);
        let game: Game<River> = game.into();
        assert_eq!(game.get_num_community_cards(), 4);
        let game: Game<ShowHands> = game.into();
        assert_eq!(game.get_num_community_cards(), 5);
    }

    #[test]
    fn early_showdown_1_all_in_2_folds() {
        let game = init_game_at_showdown_with_1_all_in();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for (i, money) in [
            game.data.settings.buy_in + game.data.small_blind + game.data.big_blind,
            game.data.settings.buy_in - game.data.small_blind,
            game.data.settings.buy_in - game.data.big_blind,
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(game.data.players[i].user.money, *money);
        }
    }

    #[test]
    fn early_showdown_1_forced_all_in_and_1_call() {
        let game = init_2_player_game();
        let mut game: Game<MoveButton> = game.into();
        // Want to force two players to all-in
        for i in 0..2 {
            if i == 0 {
                game.data.players[i].user.money = game.data.settings.min_big_blind;
            }
        }
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Call(5)), Ok(Action::Call(5)));
        assert_eq!(game.get_next_action_options(), None);
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
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        let game: Game<ShowHands> = game.into();
        assert!(game.is_pot_empty());
        for (i, money) in [
            2 * game.data.settings.min_big_blind,
            game.data.settings.buy_in - game.data.settings.min_big_blind,
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(game.data.players[i].user.money, *money);
        }
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
        for (i, money) in [game.data.settings.buy_in, 2 * game.data.settings.buy_in, 0]
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
            assert_eq!(game.data.players[i].user.money, game.data.settings.buy_in);
        }
    }

    #[test]
    fn early_showdown_3_decreasing_all_ins() {
        let game = init_3_player_game();
        let mut game: Game<MoveButton> = game.into();
        for i in 0..3 {
            game.data.players[i].user.money = game.data.settings.buy_in * (3 - i as u32);
        }
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([Action::AllIn, Action::Fold,]))
        );
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([Action::AllIn, Action::Fold,]))
        );
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
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
        for (i, money) in [6 * game.data.settings.buy_in, 0, 0].iter().enumerate() {
            assert_eq!(game.data.players[i].user.money, *money);
        }
    }

    #[test]
    fn early_showdown_3_increasing_all_ins() {
        let game = init_3_player_game();
        let mut game: Game<MoveButton> = game.into();
        for i in 0..3 {
            game.data.players[i].user.money = game.data.settings.buy_in * (i as u32 + 1);
        }
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(195),
                Action::Fold,
            ]))
        );
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([Action::Call(390), Action::Fold,]))
        );
        assert_eq!(game.act(Action::Call(390)), Ok(Action::Call(390)));
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
            3 * game.data.settings.buy_in,
            2 * game.data.settings.buy_in,
            game.data.settings.buy_in,
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

        assert_eq!(game.new_user(username), Ok(true));
        assert!(game.contains_spectator(username));

        assert_eq!(game.new_user(username), Err(UserError::UserAlreadyExists));

        assert_eq!(game.waitlist_user(username), Ok(true));
        assert!(game.contains_waitlister(username));

        assert_eq!(game.spectate_user(username), Ok(true));
        assert!(game.contains_spectator(username));

        assert_eq!(game.remove_user(username), Ok(true));
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

        assert_eq!(game.new_user(username), Ok(true));
        assert!(game.contains_spectator(username));

        assert_eq!(game.waitlist_user(username), Ok(true));
        assert!(game.contains_waitlister(username));

        assert_eq!(game.remove_user(username), Ok(true));
        assert!(!game.contains_user(username));

        for i in 0..game.data.settings.max_users {
            assert_eq!(game.new_user(&i.to_string()), Ok(true));
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
            assert_eq!(game.new_user(&username), Ok(true));
            assert_eq!(game.waitlist_user(&username), Ok(true));
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
        let game: Game<RemovePlayers> = game.into();
        let mut game: Game<DivideDonations> = game.into();
        assert_eq!(game.remove_user("0"), Ok(true));
        assert!(!game.contains_user("0"));
        assert!(game.contains_player("1"));
        assert!(game.contains_player("2"));
        let game: Game<UpdateBlinds> = game.into();
        for i in 0..2 {
            assert_eq!(game.data.players[i].user.money, game.data.settings.buy_in);
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
        assert_eq!(game.remove_user("0"), Ok(false));
        let game: Game<DistributePot> = game.into();
        let game: Game<RemovePlayers> = game.into();
        let game: Game<DivideDonations> = game.into();
        assert!(!game.contains_user("0"));
        assert!(game.contains_player("1"));
        assert!(game.contains_player("2"));
        let game: Game<UpdateBlinds> = game.into();
        for i in 0..2 {
            assert_eq!(game.data.players[i].user.money, game.data.settings.buy_in);
        }
        let mut expected_open_seats = Vec::from_iter(3..game.data.settings.max_players);
        expected_open_seats.push(0);
        assert_eq!(game.data.open_seats, expected_open_seats)
    }

    #[test]
    fn show_hands_after_checks() {
        let game = init_3_player_game();
        let game: Game<MoveButton> = game.into();
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(game.act(Action::Call(5)), Ok(Action::Call(5)));
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        let game: Game<Flop> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        let game: Game<Turn> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        let game: Game<River> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        for (i, state) in [PlayerState::Fold, PlayerState::Show, PlayerState::Show]
            .iter()
            .enumerate()
        {
            assert_eq!(game.data.players[i].state, *state);
        }
    }

    #[test]
    fn show_hands_after_raise_and_call() {
        let game = init_3_player_game();
        let game: Game<MoveButton> = game.into();
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(game.act(Action::Call(5)), Ok(Action::Call(5)));
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        let game: Game<Flop> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        let game: Game<Turn> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        let game: Game<River> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Raise(20)), Ok(Action::Raise(20)));
        assert_eq!(game.act(Action::Call(20)), Ok(Action::Call(20)));
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        for (i, state) in [PlayerState::Fold, PlayerState::Show, PlayerState::Show]
            .iter()
            .enumerate()
        {
            assert_eq!(game.data.players[i].state, *state);
        }
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
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([Action::AllIn, Action::Fold]))
        );
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([Action::AllIn, Action::Fold]))
        );
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
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
        assert_eq!(game.act(Action::Call(10)), Ok(Action::Call(10)));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(5),
                Action::Fold,
                Action::Raise(15)
            ]))
        );
        assert_eq!(game.act(Action::Call(5)), Ok(Action::Call(5)));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Check,
                Action::Fold,
                Action::Raise(20)
            ]))
        );
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
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
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(5),
                Action::Fold,
                Action::Raise(15)
            ]))
        );
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
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
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
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
        assert_eq!(game.act(Action::Raise(15)), Ok(Action::Raise(15)));
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
        assert_eq!(game.act(Action::Raise(30)), Ok(Action::Raise(30)));
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
        assert_eq!(game.act(Action::Raise(60)), Ok(Action::Raise(60)));
        assert_eq!(
            game.get_next_action_options(),
            Some(HashSet::from([
                Action::AllIn,
                Action::Call(40),
                Action::Fold,
                Action::Raise(120)
            ]))
        );
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(game.get_next_action_options(), None);
    }
}

#[cfg(test)]
mod state_tests {
    use super::{entities::Action, PokerState, UserError};

    fn init_state() -> PokerState {
        let mut state = PokerState::new();
        for i in 0..3 {
            let username = i.to_string();
            state.new_user(&username).unwrap();
            state.waitlist_user(&username).unwrap();
        }
        state
    }

    #[test]
    fn cant_start_game() {
        let mut state = init_state();
        assert_eq!(state.init_start("0"), Ok(()));
        // At SeatPlayers.
        state = state.step();
        assert_eq!(state.init_start("0"), Err(UserError::GameAlreadyStarting));
        assert_eq!(state.remove_user("1"), Ok(()));
        assert_eq!(state.remove_user("2"), Ok(()));
        // Should be back at Lobby.
        state = state.step();
        assert_eq!(state.init_start("0"), Err(UserError::NotEnoughPlayers));
    }

    #[test]
    fn early_showdown_1_winner_2_early_folds() {
        let mut state = init_state();
        assert_eq!(state.init_start("0"), Ok(()));
        // SeatPlayers
        state = state.step();
        // MoveButton
        state = state.step();
        // CollectBlinds
        state = state.step();
        // Deal
        state = state.step();
        // TakeAction
        state = state.step();
        // 1st fold
        state = state.step();
        // 2nd fold
        state = state.step();
        // Flop
        state = state.step();
        // Turn
        state = state.step();
        // River
        state = state.step();
        // ShowHands
        state = state.step();
        // DistributePot
        state = state.step();
        // RemovePlayers
        state = state.step();
        // DivideDonations
        state = state.step();
        // UpdateBlinds
        state = state.step();
        // BootPlayers
        state = state.step();
        // Lobby
        state = state.step();
        assert_eq!(state.init_start("0"), Ok(()));
    }

    #[test]
    fn early_showdown_1_winner_2_folds() {
        let mut state = init_state();
        assert_eq!(state.init_start("0"), Ok(()));
        // SeatPlayers
        state = state.step();
        // MoveButton
        state = state.step();
        // CollectBlinds
        state = state.step();
        // Deal
        state = state.step();
        // TakeAction
        state = state.step();
        // All-in
        assert_eq!(state.take_action("0", Action::AllIn), Ok(Action::AllIn));
        // 1st fold
        state = state.step();
        // 2nd fold
        state = state.step();
        // Flop
        state = state.step();
        // Turn
        state = state.step();
        // River
        state = state.step();
        // ShowHands
        state = state.step();
        // DistributePot
        state = state.step();
        // RemovePlayers
        state = state.step();
        // DivideDonations
        state = state.step();
        // UpdateBlinds
        state = state.step();
        // BootPlayers
        state = state.step();
        // Lobby
        state = state.step();
        assert_eq!(state.init_start("0"), Ok(()));
    }

    #[test]
    fn early_showdown_1_winner_2_late_folds() {
        let mut state = init_state();
        assert_eq!(state.init_start("0"), Ok(()));
        // SeatPlayers
        state = state.step();
        // MoveButton
        state = state.step();
        // CollectBlinds
        state = state.step();
        // Deal
        state = state.step();
        // TakeAction
        state = state.step();
        // Call
        assert_eq!(
            state.take_action("0", Action::Call(10)),
            Ok(Action::Call(10))
        );
        // Check
        assert_eq!(state.take_action("1", Action::Call(5)), Ok(Action::Call(5)));
        // Check
        assert_eq!(state.take_action("2", Action::Check), Ok(Action::Check));
        // Flop
        state = state.step();
        // TakeAction
        state = state.step();
        // Check
        assert_eq!(state.take_action("0", Action::Check), Ok(Action::Check));
        // Check
        assert_eq!(state.take_action("1", Action::Check), Ok(Action::Check));
        // Check
        assert_eq!(state.take_action("2", Action::Check), Ok(Action::Check));
        // Turn
        state = state.step();
        // TakeAction
        state = state.step();
        // Check
        assert_eq!(state.take_action("0", Action::Check), Ok(Action::Check));
        // Check
        assert_eq!(state.take_action("1", Action::Check), Ok(Action::Check));
        // Check
        assert_eq!(state.take_action("2", Action::Check), Ok(Action::Check));
        // River
        state = state.step();
        // TakeAction
        state = state.step();
        // Check
        assert_eq!(state.take_action("0", Action::AllIn), Ok(Action::AllIn));
        // Check
        assert_eq!(state.take_action("1", Action::Fold), Ok(Action::Fold));
        // Check
        assert_eq!(state.take_action("2", Action::Fold), Ok(Action::Fold));
        // ShowHands
        state = state.step();
        // DistributePot
        state = state.step();
        // RemovePlayers
        state = state.step();
        // DivideDonations
        state = state.step();
        // UpdateBlinds
        state = state.step();
        // BootPlayers
        state = state.step();
        // Lobby
        state = state.step();
        assert_eq!(state.init_start("0"), Ok(()));
    }
}
