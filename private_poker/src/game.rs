use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use std::{
    cmp::{Ordering, max, min},
    collections::{HashMap, HashSet, VecDeque},
    fmt,
};
use thiserror::Error;

pub mod constants;
pub mod entities;
pub mod functional;

use constants::{DEFAULT_MAX_USERS, MAX_PLAYERS};
use entities::{
    Action, ActionChoice, ActionChoices, Bet, BetAction, Blinds, Card, DEFAULT_BUY_IN,
    DEFAULT_MIN_BIG_BLIND, DEFAULT_MIN_SMALL_BLIND, Deck, GameView, GameViews, PlayPositions,
    Player, PlayerCounts, PlayerQueues, PlayerState, PlayerView, Pot, PotView, SeatIndex, Usd,
    User, Username, Vote,
};

#[derive(Debug, Deserialize, Eq, Error, PartialEq, Serialize)]
pub enum UserError {
    #[error("can't show hand")]
    CannotShowHand,
    #[error("can't start unless you're waitlisted or a player")]
    CannotStartGame,
    #[error("can't vote on yourself")]
    CannotVoteOnSelf,
    #[error("game is full")]
    CapacityReached,
    #[error("game already in progress")]
    GameAlreadyInProgress,
    #[error("game already starting")]
    GameAlreadyStarting,
    #[error("need >= ${big_blind} for the big blind")]
    InsufficientFunds { big_blind: Usd },
    #[error("invalid action")]
    InvalidAction,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum GameEvent {
    KickQueue(Username),
    Kicked(Username),
    RemoveQueue(Username),
    Removed(Username),
    SpectateQueue(Username),
    Spectated(Username),
    Waitlisted(Username),
    ResetUserMoneyQueue(Username),
    ResetUserMoney(Username),
    ResetAllMoneyQueue,
    ResetAllMoney,
    PassedVote(Vote),
    SplitPot(Username, Usd),
    JoinedTable(Username),
}

impl fmt::Display for GameEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            Self::KickQueue(username) => {
                format!("{username} will be kicked after the game")
            }
            Self::Kicked(username) => format!("{username} kicked from the game"),
            Self::RemoveQueue(username) => {
                format!("{username} will be removed after the game")
            }
            Self::Removed(username) => format!("{username} removed from the game"),
            Self::SpectateQueue(username) => {
                format!("{username} will move to spectate after the game")
            }
            Self::Spectated(username) => format!("{username} moved to spectate"),
            Self::Waitlisted(username) => format!("{username} waitlisted"),
            Self::ResetUserMoneyQueue(username) => {
                format!("{username}'s money will be reset after the game")
            }
            Self::ResetUserMoney(username) => format!("reset {username}'s money"),
            Self::ResetAllMoneyQueue => "everyone's money will be reset after the game".to_string(),
            Self::ResetAllMoney => "reset everyone's money".to_string(),
            Self::PassedVote(vote) => format!("vote to {vote} passed"),
            Self::SplitPot(username, amount) => format!("{username} won ${amount}"),
            Self::JoinedTable(username) => format!("{username} joined the table"),
        };
        write!(f, "{repr}")
    }
}

#[derive(Debug)]
pub struct GameSettings {
    pub buy_in: Usd,
    pub min_small_blind: Usd,
    pub min_big_blind: Usd,
    pub max_players: usize,
    pub max_users: usize,
}

impl GameSettings {
    #[must_use]
    pub fn new(max_players: usize, max_users: usize, buy_in: Usd) -> Self {
        let min_big_blind = buy_in / 60;
        let min_small_blind = min_big_blind / 2;
        Self {
            buy_in,
            min_small_blind,
            min_big_blind,
            max_players,
            max_users,
        }
    }
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            buy_in: DEFAULT_BUY_IN,
            min_small_blind: DEFAULT_MIN_SMALL_BLIND,
            min_big_blind: DEFAULT_MIN_BIG_BLIND,
            max_players: MAX_PLAYERS,
            max_users: DEFAULT_MAX_USERS,
        }
    }
}

#[derive(Debug)]
pub struct GameData {
    /// Deck of cards. This is instantiated once and reshuffled
    /// each deal.
    deck: Deck,
    pub blinds: Blinds,
    pub spectators: HashSet<User>,
    pub waitlist: VecDeque<User>,
    pub open_seats: VecDeque<SeatIndex>,
    pub players: Vec<Player>,
    /// Community cards shared amongst all players.
    pub board: Vec<Card>,
    /// Mapping of running votes to users that are for those running votes.
    votes: HashMap<Vote, HashSet<Username>>,
    player_counts: PlayerCounts,
    pub pot: Pot,
    /// Queues of players to do things with at a later point of
    /// an active game.
    player_queues: PlayerQueues,
    pub play_positions: PlayPositions,
    /// Stack of game events that give more insight as to what kind
    /// of game updates occur due to user actions or game state
    /// changes.
    events: VecDeque<GameEvent>,
    /// Mapping of username to money they had when they were
    /// last connected to the game. This is updated when a
    /// user is removed/kicked from the game. When a user reconnects,
    /// the value in here is used as their starting money stack.
    ledger: HashMap<Username, Usd>,
    /// If this is set, then users voted to reset everyone's
    /// money, but a game was in progress, so everyone's money
    /// will be reset after the game is over.
    reset_all_money_after_game: bool,
    settings: GameSettings,
}

impl GameData {
    fn new() -> Self {
        let settings = GameSettings::default();
        settings.into()
    }
}

impl From<GameSettings> for GameData {
    fn from(value: GameSettings) -> Self {
        Self {
            deck: Deck::default(),
            blinds: Blinds {
                small: value.min_small_blind,
                big: value.min_big_blind,
            },
            spectators: HashSet::with_capacity(value.max_users),
            waitlist: VecDeque::with_capacity(value.max_users),
            open_seats: VecDeque::from_iter(0..value.max_players),
            players: Vec::with_capacity(value.max_players),
            board: Vec::with_capacity(5),
            votes: HashMap::with_capacity(2 * value.max_users + 1),
            player_counts: PlayerCounts::default(),
            pot: Pot::new(value.max_players),
            player_queues: PlayerQueues::default(),
            play_positions: PlayPositions::default(),
            events: VecDeque::new(),
            ledger: HashMap::with_capacity(value.max_users),
            reset_all_money_after_game: false,
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
    #[must_use]
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
    pub action_choices: Option<ActionChoices>,
}

#[derive(Debug)]
pub struct Flop {}

#[derive(Debug)]
pub struct Turn {}

#[derive(Debug)]
pub struct River {}

#[derive(Clone, Debug)]
pub struct ShowHands {}

#[derive(Debug)]
pub struct DistributePot {}

#[derive(Debug)]
pub struct RemovePlayers {}

#[derive(Debug)]
pub struct UpdateBlinds {}

#[derive(Debug)]
pub struct BootPlayers {}

#[enum_dispatch]
pub trait GameStateManagement {
    fn drain_events(&mut self) -> VecDeque<GameEvent>;
    fn get_views(&self) -> GameViews;
}

#[enum_dispatch]
pub trait PhaseDependentUserManagement {
    fn kick_user(&mut self, username: &Username) -> Result<Option<bool>, UserError>;
    fn remove_user(&mut self, username: &Username) -> Result<Option<bool>, UserError>;
    fn reset_all_money(&mut self) -> bool;
    fn reset_user_money(&mut self, username: &Username) -> Result<Option<bool>, UserError>;
    fn spectate_user(&mut self, username: &Username) -> Result<Option<bool>, UserError>;
}

#[enum_dispatch]
pub trait PhaseIndependentUserManagement {
    fn cast_vote(&mut self, username: &Username, vote: Vote) -> Result<Option<Vote>, UserError>;
    fn new_user(&mut self, username: &Username) -> Result<bool, UserError>;
    fn waitlist_user(&mut self, username: &Username) -> Result<Option<bool>, UserError>;
}

/// A poker game with data and logic for running a poker game end-to-end.
/// Any kind of networking, client-server, or complex user management logic
/// is out-of-scope for this object as its sole focus is game data and logic.
#[derive(Debug)]
pub struct Game<T> {
    pub data: GameData,
    pub state: T,
}

/// General game methods that can or will be used at various stages of gameplay.
impl<T> Game<T> {
    fn as_view(&self, username: &Username) -> GameView {
        let mut players = Vec::with_capacity(self.data.settings.max_players);
        for player in &self.data.players {
            let cards = if &player.user.name == username || player.showing {
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
        GameView {
            blinds: self.data.blinds.clone(),
            spectators: self.data.spectators.clone(),
            waitlist: self.data.waitlist.clone(),
            open_seats: self.data.open_seats.clone(),
            players,
            board: self.data.board.clone(),
            pot: PotView {
                size: self.data.pot.get_size(),
            },
            play_positions: self.data.play_positions.clone(),
        }
    }

    /// Handle a user being removed/kicked from the game. Rescind all votes
    /// for or by the user, and update the ledger.
    fn cleanup_user(&mut self, user: User) {
        let User { name, money } = user;
        self.data.votes.remove(&Vote::Kick(name.clone()));
        self.data.votes.remove(&Vote::Reset(Some(name.clone())));
        for votes in self.data.votes.values_mut() {
            votes.remove(&name);
        }
        self.data.ledger.insert(name, money);
    }

    pub fn contains_player(&self, username: &Username) -> bool {
        self.data.players.iter().any(|p| &p.user.name == username)
    }

    fn contains_user(&self, username: &Username) -> bool {
        self.data.spectators.contains(username)
            || self
                .data
                .waitlist
                .iter()
                .chain(self.data.players.iter().map(|p| &p.user))
                .any(|u| &u.name == username)
    }

    pub fn contains_spectator(&self, username: &Username) -> bool {
        self.data.spectators.contains(username)
    }

    pub fn contains_waitlister(&self, username: &Username) -> bool {
        self.data.waitlist.iter().any(|u| &u.name == username)
    }

    /// Return the index of the player who has the next action, or
    /// nothing if no one has the next turn.
    fn get_next_action_idx(&self, new_phase: bool) -> Option<SeatIndex> {
        let starting_idx = self
            .data
            .play_positions
            .next_action_idx
            .map(|idx| idx + usize::from(!new_phase))?;
        let num_players = self.data.players.len();
        (0..num_players)
            .map(|player_idx| (starting_idx + player_idx) % num_players)
            .find(|&player_idx| self.data.players[player_idx].state == PlayerState::Wait)
    }

    /// Return the set of possible actions the next player can
    /// make, or nothing if there are no actions possible for the current
    /// state.
    fn get_next_action_choices(&self) -> Option<ActionChoices> {
        self.data.play_positions.next_action_idx.map(|action_idx| {
            let mut action_choices = HashSet::from([ActionChoice::Fold]);
            let user = &self.data.players[action_idx].user;
            let raise = self.data.pot.get_min_raise_by_player_idx(action_idx);
            let call = self.data.pot.get_call_by_player_idx(action_idx);
            if self.data.player_counts.num_active > 1 || call >= user.money {
                action_choices.insert(ActionChoice::AllIn);
            }
            if call > 0 && call < user.money {
                action_choices.insert(ActionChoice::Call(call));
            } else if call == 0 {
                action_choices.insert(ActionChoice::Check);
            }
            if self.data.player_counts.num_active > 1 && user.money > raise {
                action_choices.insert(ActionChoice::Raise(raise));
            }
            ActionChoices(action_choices)
        })
    }

    /// Return the username of the user that has the next turn (or nothing
    /// if there is no turn next). Helps determine whether to notify the
    /// player that their turn has come.
    pub fn get_next_action_username(&self) -> Option<Username> {
        self.data
            .play_positions
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

    /// Return whether the game is ready to move onto the next phase
    /// now that the betting round is over.
    fn is_end_of_round(&self) -> bool {
        self.data.player_counts.num_active == self.data.player_counts.num_called
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
        let no_turns_left = self.data.player_counts.num_active <= 1;
        self.data
            .play_positions
            .next_action_idx
            .map_or(no_turns_left, |action_idx| {
                no_turns_left && self.data.pot.get_call_by_player_idx(action_idx) == 0
            })
    }

    /// Return whether it's the user's turn. This helps determine whether
    /// a user trying to take an action can actually take an action, or
    /// if they're violating rules of play.
    pub fn is_turn(&self, username: &Username) -> bool {
        self.data
            .play_positions
            .next_action_idx
            .is_some_and(|idx| &self.data.players[idx].user.name == username)
    }

    #[must_use]
    pub fn new() -> Game<Lobby> {
        Game {
            data: GameData::new(),
            state: Lobby::new(),
        }
    }

    fn pass_vote_with_event(&mut self, vote: Vote) {
        self.data.votes.remove(&vote);
        self.data.events.push_back(GameEvent::PassedVote(vote));
    }

    /// Reset the next action index and return the possible actions
    /// for that player. This should be called prior to each game phase
    /// in preparation for a new round of betting.
    fn prepare_for_next_phase(&mut self) -> Option<ActionChoices> {
        self.data.player_counts.num_called = 0;
        // Reset player states for players that are still in the hand.
        for player in self.data.players.iter_mut().filter(|player| {
            matches!(
                player.state,
                PlayerState::Call | PlayerState::Check | PlayerState::Raise
            )
        }) {
            player.state = PlayerState::Wait;
        }
        self.data.play_positions.next_action_idx =
            Some(self.data.play_positions.starting_action_idx);
        self.data.play_positions.next_action_idx = self.get_next_action_idx(true);
        self.get_next_action_choices()
    }

    fn queue_player_for_kick_with_event(&mut self, username: &Username) {
        self.data
            .events
            .push_back(GameEvent::KickQueue(username.clone()));
        self.data.player_queues.to_kick.insert(username.clone());
    }

    fn queue_player_for_remove_with_event(&mut self, username: &Username) {
        self.data
            .events
            .push_back(GameEvent::RemoveQueue(username.clone()));
        // Need to remove the player from other queues just in
        // case they changed their mind.
        self.data.player_queues.to_spectate.remove(username);
        self.data.player_queues.to_remove.insert(username.clone());
    }

    fn queue_player_for_reset_with_event(&mut self, username: &Username) {
        self.data
            .events
            .push_back(GameEvent::ResetUserMoneyQueue(username.clone()));
        self.data.player_queues.to_reset.insert(username.clone());
    }

    fn queue_player_for_spectate_with_event(&mut self, username: &Username) {
        self.data
            .events
            .push_back(GameEvent::SpectateQueue(username.clone()));
        // Need to remove the player from other queues just in
        // case they changed their mind.
        self.data.player_queues.to_remove.remove(username);
        self.data.player_queues.to_spectate.insert(username.clone());
    }

    fn seat_player_with_event(&mut self, player: Player) {
        self.data
            .events
            .push_back(GameEvent::JoinedTable(player.user.name.clone()));
        self.data.players.push(player);
    }

    fn spectate_user_with_event(&mut self, user: User) {
        self.data
            .events
            .push_back(GameEvent::Spectated(user.name.clone()));
        self.data.spectators.insert(user);
    }
}

impl<T> GameStateManagement for Game<T> {
    fn drain_events(&mut self) -> VecDeque<GameEvent> {
        self.data.events.drain(..).collect()
    }

    /// Return independent views of the game for each user. For non-players,
    /// only the board is shown until the showdown. For players, only their
    /// hand and the board is shown until the showdown.
    fn get_views(&self) -> GameViews {
        let mut views = HashMap::with_capacity(self.data.settings.max_users);
        for username in self
            .data
            .spectators
            .iter()
            .map(|u| &u.name)
            .chain(self.data.waitlist.iter().map(|u| &u.name))
            .chain(self.data.players.iter().map(|p| &p.user.name))
        {
            views.insert(username.clone(), self.as_view(username));
        }
        views
    }
}

impl<T> PhaseIndependentUserManagement for Game<T> {
    /// A user casts a vote. Returns true if the vote is recorded and results
    /// in a passing vote, and false otherwise.
    fn cast_vote(&mut self, username: &Username, vote: Vote) -> Result<Option<Vote>, UserError> {
        let num_users = self.get_num_users();
        if num_users > 1 {
            // Make sure the vote is even possible.
            match &vote {
                Vote::Kick(user_target) | Vote::Reset(Some(user_target)) => {
                    if username == user_target {
                        return Err(UserError::CannotVoteOnSelf);
                    } else if !self.contains_user(user_target) {
                        return Err(UserError::UserDoesNotExist);
                    }
                }
                // No vote-specific validation necessary for other votes.
                Vote::Reset(None) => {}
            }
            let votes = self.data.votes.entry(vote.clone()).or_default();
            let is_vote_passing = votes.insert(username.clone()) && votes.len() > num_users / 2;
            // Add an event on the vote's passage, and return a copy of the vote that passed.
            if is_vote_passing {
                self.pass_vote_with_event(vote.clone());
                Ok(Some(vote))
            } else {
                Ok(None)
            }
        } else {
            Err(UserError::NotEnoughPlayers)
        }
    }

    /// Add a new user to the game, making them a spectator.
    fn new_user(&mut self, username: &Username) -> Result<bool, UserError> {
        if self.get_num_users() == self.data.settings.max_users {
            return Err(UserError::CapacityReached);
        } else if self.contains_user(username) {
            // Check if player already exists but is queued for removal.
            // This probably means the user disconnected and is trying
            // to reconnect.
            return self
                .data
                .player_queues
                .to_remove
                .remove(username)
                .then_some(false)
                .ok_or(UserError::UserAlreadyExists);
        }
        // Check the ledger for some memory of the user's money stack.
        // There are a couple of flaws with this. If a user runs out
        // of money, they can leave and then rejoin under a different
        // name to get more money, and if a user uses another user's
        // name, then they'll take ownership of their money stack.
        // However, both of these flaws can be avoided by running a
        // server with some kind of user management.
        let money = self
            .data
            .ledger
            .remove(username)
            .unwrap_or(self.data.settings.buy_in);
        self.data.spectators.insert(User {
            name: username.clone(),
            money,
        });
        Ok(true)
    }

    /// Add a user to the waitlist, putting them in queue to play. The queue
    /// is eventually drained until the table is full and there are no more
    /// seats available for play.
    fn waitlist_user(&mut self, username: &Username) -> Result<Option<bool>, UserError> {
        // Need to remove the player from the removal and spectate sets just in
        // case they wanted to do one of those, but then changed their mind and
        // want to play again.
        self.data.player_queues.to_spectate.remove(username);
        self.data.player_queues.to_remove.remove(username);
        if let Some(user) = self.data.spectators.take(username) {
            if user.money < self.data.blinds.big {
                self.data.spectators.insert(user);
                return Err(UserError::InsufficientFunds {
                    big_blind: self.data.blinds.big,
                });
            }
            self.data.waitlist.push_back(user);
            self.data
                .events
                .push_back(GameEvent::Waitlisted(username.clone()));
            Ok(Some(true))
        } else if self.contains_player(username) {
            // The user is already playing, so we don't need to do anything,
            // but we should acknowledge that the user still isn't
            // technically waitlisted.
            Ok(None)
        } else if self.contains_waitlister(username) {
            // The user is already waitlisted.
            Ok(None)
        } else {
            Err(UserError::UserDoesNotExist)
        }
    }
}

// User management implementations for game states where user states can be
// immediately updated since it wouldn't interfere with gameplay.
macro_rules! impl_user_managers {
    ($($t:ty),+) => {
        $(impl PhaseDependentUserManagement for $t {
            fn kick_user(&mut self, username: &Username) -> Result<Option<bool>, UserError> {
                let user = if let Some(user) = self.data.spectators.take(username) {
                    user
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| &u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister should exist")
                } else if let Some(player_idx) = self.data.players.iter().position(|p| &p.user.name == username) {
                    self.data.player_queues.to_spectate.remove(username);
                    let player = self.data.players.remove(player_idx);
                    self.data.open_seats.push_back(player.seat_idx);
                    player.user
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.events.push_back(GameEvent::Kicked(username.clone()));
                self.cleanup_user(user);
                Ok(Some(true))
            }

            fn remove_user(&mut self, username: &Username) -> Result<Option<bool>, UserError> {
                let user = if let Some(user) = self.data.spectators.take(username) {
                    user
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| &u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister should exist")
                } else if let Some(player_idx) = self.data.players.iter().position(|p| &p.user.name == username) {
                    self.data.player_queues.to_spectate.remove(username);
                    let player = self.data.players.remove(player_idx);
                    self.data.open_seats.push_back(player.seat_idx);
                    player.user
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.events.push_back(GameEvent::Removed(username.clone()));
                self.cleanup_user(user);
                Ok(Some(true))
            }

            fn reset_user_money(&mut self, username: &Username) -> Result<Option<bool>, UserError> {
                if let Some(mut user) = self.data.spectators.take(username) {
                    user.money = self.data.settings.buy_in;
                    self.data.spectators.insert(user);
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| &u.name == username) {
                    let user = self.data.waitlist.get_mut(waitlist_idx).expect("waitlister should exist");
                    user.money = self.data.settings.buy_in;
                } else if let Some(player_idx) = self.data.players.iter().position(|p| &p.user.name == username) {
                    let player = self.data.players.get_mut(player_idx).expect("player should exist");
                    player.user.money = self.data.settings.buy_in;
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.events.push_back(GameEvent::ResetUserMoney(username.clone()));
                Ok(Some(true))
            }

            fn reset_all_money(&mut self) -> bool {
                self.data.reset_all_money_after_game = false;
                let mut spectators: Vec<User> = self.data.spectators.drain().collect();
                for user in spectators
                    .iter_mut()
                    .chain(self.data.waitlist.iter_mut())
                    .chain(self.data.players.iter_mut().map(|p| &mut p.user))
                {
                    user.money = self.data.settings.buy_in;
                }
                for user in spectators {
                    self.data.spectators.insert(user);
                }
                self.data.events.push_back(GameEvent::ResetAllMoney);
                true
            }

            fn spectate_user(&mut self, username: &Username) -> Result<Option<bool>, UserError> {
                // The player has already been queued for spectate. Just wait for
                // the next spectate phase.
                if self.data.player_queues.to_spectate.contains(username) {
                    return Ok(Some(false));
                }
                let user = if self.data.spectators.contains(username) {
                    return Ok(None);
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| &u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister should exist")
                } else if let Some(player_idx) = self.data.players.iter().position(|p| &p.user.name == username) {
                    self.data.player_queues.to_remove.remove(username);
                    let player = self.data.players.remove(player_idx);
                    self.data.open_seats.push_back(player.seat_idx);
                    player.user
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.events.push_back(GameEvent::Spectated(username.clone()));
                self.data.spectators.insert(user);
                Ok(Some(true))
            }
        })*
    }
}

// User management implementations for game states where user states can't be
// immediately updated since it'd interfere with gameplay. Generally, if a user
// state can't be updated, they're instead queued to be updated and are then
// updated at a later game state.
macro_rules! impl_user_managers_with_queue {
    ($($t:ty),+) => {
        $(impl PhaseDependentUserManagement for $t {
            fn kick_user(&mut self, username: &Username) -> Result<Option<bool>, UserError> {
                // The player has already been queued for kick. Just wait for
                // the next kick phase.
                if self.data.player_queues.to_kick.contains(username) {
                    return Ok(Some(false));
                }
                let user = if let Some(user) = self.data.spectators.take(username) {
                    user
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| &u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister should exist")
                } else if let Some(_) = self.data.players.iter().position(|p| &p.user.name == username) {
                    self.queue_player_for_kick_with_event(username);
                    return Ok(Some(false));
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.events.push_back(GameEvent::Kicked(username.clone()));
                self.cleanup_user(user);
                Ok(Some(true))
            }

            fn remove_user(&mut self, username: &Username) -> Result<Option<bool>, UserError> {
                // The player has already been queued for removal. Just wait for
                // the next removal phase.
                if self.data.player_queues.to_remove.contains(username) {
                    return Ok(Some(false));
                }
                let user = if let Some(user) = self.data.spectators.take(username) {
                    user
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| &u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister should exist")
                } else if let Some(_) = self.data.players.iter().position(|p| &p.user.name == username) {
                    self.queue_player_for_remove_with_event(username);
                    return Ok(Some(false));
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.events.push_back(GameEvent::Removed(username.clone()));
                self.cleanup_user(user);
                Ok(Some(true))
            }

            fn reset_user_money(&mut self, username: &Username) -> Result<Option<bool>, UserError> {
                if let Some(mut user) = self.data.spectators.take(username) {
                    user.money = self.data.settings.buy_in;
                    self.data.spectators.insert(user);
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| &u.name == username) {
                    let user = self.data.waitlist.get_mut(waitlist_idx).expect("waitlister should exist");
                    user.money = self.data.settings.buy_in;
                } else if let Some(_) = self.data.players.iter().position(|p| &p.user.name == username) {
                    self.queue_player_for_reset_with_event(username);
                    return Ok(Some(false));
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.events.push_back(GameEvent::ResetUserMoney(username.clone()));
                Ok(Some(true))
            }

            fn reset_all_money(&mut self) -> bool {
                self.data.reset_all_money_after_game = true;
                self.data.events.push_back(GameEvent::ResetAllMoneyQueue);
                false
            }

            fn spectate_user(&mut self, username: &Username) -> Result<Option<bool>, UserError> {
                // The player has already been queued for spectate. Just wait for
                // the next spectate phase.
                if self.data.player_queues.to_spectate.contains(username) {
                    return Ok(Some(false));
                }
                let user = if self.data.spectators.contains(username) {
                    return Ok(None)
                } else if let Some(waitlist_idx) = self.data.waitlist.iter().position(|u| &u.name == username) {
                    self.data.waitlist.remove(waitlist_idx).expect("waitlister should exist")
                } else if let Some(_) = self.data.players.iter().position(|p| &p.user.name == username) {
                    self.queue_player_for_spectate_with_event(username);
                    return Ok(Some(false));
                } else {
                    return Err(UserError::UserDoesNotExist);
                };
                self.data.events.push_back(GameEvent::Spectated(username.clone()));
                self.data.spectators.insert(user);
                Ok(Some(true))
            }
        })*
    }
}

impl_user_managers!(
    Game<Lobby>,
    Game<SeatPlayers>,
    Game<RemovePlayers>,
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
        if self.get_num_potential_players() < 2 {
            Err(UserError::NotEnoughPlayers)
        } else if self.state.start_game {
            Err(UserError::GameAlreadyStarting)
        } else {
            self.state.start_game = true;
            Ok(())
        }
    }

    #[must_use]
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
            let open_seat_idx = value
                .data
                .open_seats
                .pop_front()
                .expect("open seats should exist");
            let user = value
                .data
                .waitlist
                .pop_front()
                .expect("waitlisters should exist");
            if user.money < value.data.blinds.big {
                value.spectate_user_with_event(user);
            } else {
                let player = Player::new(user, open_seat_idx);
                value.seat_player_with_event(player);
            }
        }
        value.data.players.sort_by_key(|p| p.seat_idx);
        value.data.player_counts.num_active = value.get_num_players();
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
            .skip(value.data.play_positions.big_blind_idx + 1);
        value.data.play_positions.big_blind_idx =
            seats.next().expect("big blind position should exist");
        value.data.play_positions.starting_action_idx =
            seats.next().expect("starting action position should exist");
        value.data.play_positions.next_action_idx =
            Some(value.data.play_positions.starting_action_idx);
        // Reverse the table search to find the small blind position relative
        // to the big blind position since the small blind must always trail the big
        // blind.
        let mut seats = player_indices
            .rev()
            .cycle()
            .skip(num_players - value.data.play_positions.big_blind_idx);
        value.data.play_positions.small_blind_idx =
            seats.next().expect("small blind position should exist");
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
            (
                value.data.play_positions.small_blind_idx,
                value.data.blinds.small,
            ),
            (
                value.data.play_positions.big_blind_idx,
                value.data.blinds.big,
            ),
        ] {
            let player = &mut value.data.players[player_idx];
            let bet = match player.user.money.cmp(&blind) {
                Ordering::Equal => {
                    player.state = PlayerState::AllIn;
                    value.data.player_counts.num_active -= 1;
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
                Ordering::Less => unreachable!(
                    "a player can't be in a game if they don't have enough for the big blind"
                ),
            };
            // Impossible for a side pot to be created from the blinds, so
            // we don't even need to check.
            value.data.pot.bet(player_idx, &bet);
            player.user.money -= blind;
        }
        value.data.player_counts.num_called = 0;
        Self {
            data: value.data,
            state: Deal {},
        }
    }
}

/// Shuffle the game's deck and deal 2 cards to each player.
impl From<Game<Deal>> for Game<TakeAction> {
    fn from(mut value: Game<Deal>) -> Self {
        value.data.deck.shuffle();

        let num_players = value.get_num_players();
        let mut seats = (0..num_players)
            .cycle()
            .skip(value.data.play_positions.small_blind_idx);
        // Deal 2 cards per player, looping over players and dealing them 1 card
        // at a time.
        for _ in 0..(2 * num_players) {
            let deal_idx = seats.next().expect("dealing position should exist");
            let player = &mut value.data.players[deal_idx];
            let card = value.data.deck.deal_card();
            player.cards.push(card);
        }
        let action_choices = value.prepare_for_next_phase();
        Self {
            data: value.data,
            state: TakeAction { action_choices },
        }
    }
}

impl Game<TakeAction> {
    pub fn act(&mut self, action: Action) -> Result<Action, UserError> {
        let sanitized_action = self.affect(action)?;
        self.data.play_positions.next_action_idx = self.get_next_action_idx(false);
        if self.is_ready_for_next_phase() {
            self.data.play_positions.next_action_idx = None;
        }
        self.state.action_choices = self.get_next_action_choices();
        Ok(sanitized_action)
    }

    fn affect(&mut self, action: Action) -> Result<Action, UserError> {
        match (
            self.data.play_positions.next_action_idx,
            &self.state.action_choices,
        ) {
            (Some(player_idx), Some(action_choices)) => {
                if !action_choices.contains(&action) {
                    return Err(UserError::InvalidAction);
                }
                let player = &mut self.data.players[player_idx];
                let pot_call = self.data.pot.get_call();
                let player_investment = self.data.pot.get_investment_by_player_idx(player_idx);
                let player_call = pot_call - player_investment;
                let player_raise = 2 * pot_call - player_investment;
                // Convert the action to a valid bet. Sanitize the bet amount according
                // to the player's intended action.
                let mut bet = match action {
                    Action::AllIn => Bet {
                        action: BetAction::AllIn,
                        amount: player.user.money,
                    },
                    Action::Call => Bet {
                        action: BetAction::Call,
                        amount: player_call,
                    },
                    Action::Check => {
                        self.data.player_counts.num_called += 1;
                        player.state = PlayerState::Check;
                        return Ok(action);
                    }
                    Action::Fold => {
                        self.data.player_counts.num_active -= 1;
                        player.state = PlayerState::Fold;
                        return Ok(action);
                    }
                    Action::Raise(Some(amount)) => Bet {
                        action: BetAction::Raise,
                        amount,
                    },
                    Action::Raise(None) => Bet {
                        action: BetAction::Raise,
                        amount: player_raise,
                    },
                };
                if bet.amount >= player.user.money {
                    bet.action = BetAction::AllIn;
                    bet.amount = player.user.money;
                }
                // Do some additional bet validation based on the bet's amount.
                let new_player_investment = player_investment + bet.amount;
                match bet.action {
                    BetAction::AllIn => {
                        self.data.player_counts.num_active -= 1;
                        if new_player_investment > pot_call {
                            self.data.player_counts.num_called = 0;
                        }
                        player.state = PlayerState::AllIn;
                    }
                    BetAction::Call => {
                        self.data.player_counts.num_called += 1;
                        player.state = PlayerState::Call;
                    }
                    BetAction::Raise => {
                        if new_player_investment < (2 * pot_call) {
                            return Err(UserError::InvalidBet { bet });
                        }
                        self.data.player_counts.num_called = 1;
                        player.state = PlayerState::Raise;
                    }
                }
                // The player's bet is OK. Remove the bet amount from the player's
                // stack and start distributing it appropriately amongst all the pots.
                player.user.money -= bet.amount;
                self.data.pot.bet(player_idx, &bet);

                // Reset other player states that're still in the hand based on the bet.
                if self.data.player_counts.num_called <= 1 {
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
                        player.state = PlayerState::Wait;
                    }
                }

                // Return the santized action.
                Ok(bet.into())
            }
            _ => Err(UserError::OutOfTurnAction),
        }
    }

    #[must_use]
    pub fn get_action_choices(&self) -> Option<ActionChoices> {
        self.state.action_choices.clone()
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
            state: ShowHands {},
        }
    }
}

impl Game<Flop> {
    fn step(&mut self) {
        for _ in 0..3 {
            let card = self.data.deck.deal_card();
            self.data.board.push(card);
        }
    }
}

/// Put the first 3 cards on the board.
impl From<Game<Flop>> for Game<TakeAction> {
    fn from(mut value: Game<Flop>) -> Self {
        value.step();
        let action_choices = value.prepare_for_next_phase();
        Self {
            data: value.data,
            state: TakeAction { action_choices },
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
        let card = self.data.deck.deal_card();
        self.data.board.push(card);
    }
}

/// Put the 4th card on the board.
impl From<Game<Turn>> for Game<TakeAction> {
    fn from(mut value: Game<Turn>) -> Self {
        value.step();
        let action_choices = value.prepare_for_next_phase();
        Self {
            data: value.data,
            state: TakeAction { action_choices },
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
        let card = self.data.deck.deal_card();
        self.data.board.push(card);
    }
}

/// Put the 5th card on the board.
impl From<Game<River>> for Game<TakeAction> {
    fn from(mut value: Game<River>) -> Self {
        value.step();
        let action_choices = value.prepare_for_next_phase();
        Self {
            data: value.data,
            state: TakeAction { action_choices },
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
            state: ShowHands {},
        }
    }
}

macro_rules! impl_show_hands {
    ($($t:ty),+) => {
        $(impl $t {
            pub fn show_hand(&mut self, username: &Username) -> Result<(), UserError> {
                match self
                    .data
                    .players
                    .iter_mut()
                    .find(|p| &p.user.name == username)
                {
                    Some(player) => {
                        if !player.showing {
                            player.showing = true;
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
    Game<UpdateBlinds>
);

impl From<Game<ShowHands>> for Game<DistributePot> {
    fn from(mut value: Game<ShowHands>) -> Self {
        let num_players_remaining: usize = value
            .data
            .players
            .iter()
            .map(|p| usize::from(p.state != PlayerState::Fold))
            .sum();
        if num_players_remaining > 1 {
            for player_idx in value.data.pot.investments.keys() {
                let player = &mut value.data.players[*player_idx];
                if player.state != PlayerState::Fold {
                    player.showing = true;
                }
            }
        }
        Self {
            data: value.data,
            state: DistributePot {},
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
                // If the player's investment doesn't match the pot's call, then
                // that means they and everyone with smaller investements aren't
                // included in the pot.
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
                    let mut cards = player.cards.clone();
                    cards.extend(self.data.board.clone());
                    functional::prepare_hand(&mut cards);
                    let hand = functional::eval(&cards);
                    hands_in_pot.push(hand);
                }
            }
            let winner_indices = functional::argmax(&hands_in_pot);

            // Finally, split the pot amongst all the winners. Pot remainder
            // goes to the house (disappears).
            let num_winners = winner_indices.len();
            let pot_split = pot_size / num_winners as Usd;
            for winner_idx in winner_indices {
                let winner_player_idx = seats_in_pot[winner_idx];
                let player = &mut self.data.players[*winner_player_idx];
                player.user.money += pot_split;
                self.data
                    .events
                    .push_back(GameEvent::SplitPot(player.user.name.clone(), pot_split));
            }
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
            state: ShowHands {},
        }
    }
}

impl From<Game<DistributePot>> for Game<RemovePlayers> {
    fn from(mut value: Game<DistributePot>) -> Self {
        value.distribute();
        value.data.player_counts.num_active = 0;
        Self {
            data: value.data,
            state: RemovePlayers {},
        }
    }
}

impl From<Game<RemovePlayers>> for Game<UpdateBlinds> {
    fn from(mut value: Game<RemovePlayers>) -> Self {
        while let Some(username) = value.data.player_queues.to_remove.pop_first() {
            // It is possible for a user to leave in this state but right before
            // this state transition occurs. That'd cause this method to return
            // an error, but it's really OK if they left since they were going
            // to be removed anyways.
            let _ = value.remove_user(&username);
        }
        while let Some(username) = value.data.player_queues.to_kick.pop_first() {
            // See above comment for why this is OK.
            let _ = value.kick_user(&username);
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
        if value.data.reset_all_money_after_game {
            value.data.player_queues.to_reset.clear();
            value.reset_all_money();
        }
        while let Some(username) = value.data.player_queues.to_reset.pop_first() {
            let _ = value.reset_user_money(&username);
        }
        let min_playable_money = value
            .data
            .spectators
            .iter()
            .map(|u| u.money)
            .chain(value.data.waitlist.iter().map(|u| u.money))
            .chain(value.data.players.iter().map(|p| p.user.money))
            .filter(|money| *money >= value.data.settings.min_big_blind)
            .min()
            .unwrap_or(value.data.settings.min_big_blind);
        let multiple = max(1, min_playable_money / value.data.settings.buy_in);
        value.data.blinds.small = multiple * value.data.settings.min_small_blind;
        value.data.blinds.big = multiple * value.data.settings.min_big_blind;
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
        for player in &mut value.data.players {
            if player.user.money < value.data.blinds.big {
                value.data.open_seats.push_back(player.seat_idx);
                value
                    .data
                    .player_queues
                    .to_spectate
                    .insert(player.user.name.clone());
            } else {
                player.reset();
            }
        }
        while let Some(username) = value.data.player_queues.to_spectate.pop_first() {
            // It is possible for a user to leave in this state but right before
            // this state transition occurs. That'd cause this method to return
            // an error, but it's really OK if they left since spectating them
            // is a softer action.
            let _ = value.spectate_user(&username);
        }
        Self {
            data: value.data,
            state: Lobby::new(),
        }
    }
}

/// A poker finite state machine. Wrapper around all possible game states,
/// managing the transition from one state to the next.
#[derive(Debug)]
#[enum_dispatch(
    GameStateManagement,
    PhaseDependentUserManagement,
    PhaseIndependentUserManagement
)]
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
            Self::Lobby(_) => "in lobby",
            Self::SeatPlayers(_) => "seating players",
            Self::MoveButton(_) => "moving button",
            Self::CollectBlinds(game) => {
                let big_blind = game.data.blinds.big;
                let big_blind_username = &game.data.players[game.data.play_positions.big_blind_idx]
                    .user
                    .name;
                let small_blind = game.data.blinds.small;
                let small_blind_username = &game.data.players
                    [game.data.play_positions.small_blind_idx]
                    .user
                    .name;
                &format!(
                    "collecting ${small_blind} from {small_blind_username} and ${big_blind} from {big_blind_username}"
                )
            }
            Self::Deal(_) => "dealing cards",
            Self::TakeAction(game) => {
                if game.is_ready_for_next_phase() {
                    "end of betting round"
                } else {
                    "betting round transition"
                }
            }
            Self::Flop(_) => "the flop",
            Self::Turn(_) => "the turn",
            Self::River(_) => "the river",
            Self::ShowHands(game) => {
                let num_pots = game.get_num_pots();
                match num_pots {
                    1 => "showing main pot",
                    i => &format!("showing side pot #{}", i - 1),
                }
            }
            Self::DistributePot(game) => {
                let num_pots = game.get_num_pots();
                match num_pots {
                    1 => "distributing main pot",
                    i => &format!("distributing side pot #{}", i - 1),
                }
            }
            Self::RemovePlayers(_) => "updating players that joined spectators or left",
            Self::UpdateBlinds(_) => "updating blinds",
            Self::BootPlayers(_) => "spectating players that can't afford the big blind",
        };
        write!(f, "{repr}")
    }
}

impl PokerState {
    #[must_use]
    pub fn get_action_choices(&self) -> Option<ActionChoices> {
        match self {
            Self::TakeAction(game) => game.get_action_choices(),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_next_action_username(&self) -> Option<Username> {
        match self {
            Self::TakeAction(game) => game.get_next_action_username(),
            _ => None,
        }
    }

    pub fn init_start(&mut self, username: &Username) -> Result<(), UserError> {
        match self {
            Self::Lobby(game) => {
                if game.contains_waitlister(username) || game.contains_player(username) {
                    game.init_start()?;
                    Ok(())
                } else {
                    Err(UserError::CannotStartGame)
                }
            }
            Self::SeatPlayers(_) => Err(UserError::GameAlreadyStarting),
            _ => Err(UserError::GameAlreadyInProgress),
        }
    }

    #[must_use]
    pub fn new() -> Self {
        let game = Game::<Lobby>::new();
        Self::Lobby(game)
    }

    fn phase_transition(game: Game<TakeAction>) -> Self {
        match game.get_num_community_cards() {
            0 => Self::Flop(game.into()),
            3 => Self::Turn(game.into()),
            4 => Self::River(game.into()),
            5 => Self::ShowHands(game.into()),
            _ => unreachable!(
                "there can only be 0, 3, 4, or 5 community cards on the board at a time"
            ),
        }
    }

    pub fn show_hand(&mut self, username: &Username) -> Result<(), UserError> {
        match self {
            Self::ShowHands(game) => {
                game.show_hand(username)?;
            }
            Self::DistributePot(game) => {
                game.show_hand(username)?;
            }
            Self::RemovePlayers(game) => {
                game.show_hand(username)?;
            }
            Self::UpdateBlinds(game) => {
                game.show_hand(username)?;
            }
            _ => return Err(UserError::CannotShowHand),
        }
        Ok(())
    }

    /// Main state transitions.
    #[must_use]
    pub fn step(self) -> Self {
        match self {
            Self::Lobby(game) => {
                if game.is_ready_to_start() {
                    Self::SeatPlayers(game.into())
                } else {
                    Self::Lobby(game)
                }
            }
            Self::SeatPlayers(game) => {
                if game.get_num_potential_players() >= 2 {
                    Self::MoveButton(game.into())
                } else {
                    Self::Lobby(game.into())
                }
            }
            Self::MoveButton(game) => Self::CollectBlinds(game.into()),
            Self::CollectBlinds(game) => Self::Deal(game.into()),
            Self::Deal(game) => Self::TakeAction(game.into()),
            Self::TakeAction(mut game) => {
                if game.is_ready_for_next_phase() {
                    Self::phase_transition(game)
                } else {
                    game.act(Action::Fold).expect("force folding should be OK");
                    if game.is_ready_for_next_phase() {
                        Self::phase_transition(game)
                    } else {
                        Self::TakeAction(game)
                    }
                }
            }
            Self::Flop(game) => {
                if game.is_ready_for_showdown() {
                    Self::Turn(game.into())
                } else {
                    Self::TakeAction(game.into())
                }
            }
            Self::Turn(game) => {
                if game.is_ready_for_showdown() {
                    Self::River(game.into())
                } else {
                    Self::TakeAction(game.into())
                }
            }
            Self::River(game) => {
                if game.is_ready_for_showdown() {
                    Self::ShowHands(game.into())
                } else {
                    Self::TakeAction(game.into())
                }
            }
            Self::ShowHands(game) => Self::DistributePot(game.into()),
            Self::DistributePot(game) => {
                if game.get_num_pots() >= 2 {
                    Self::ShowHands(game.into())
                } else {
                    Self::RemovePlayers(game.into())
                }
            }
            Self::RemovePlayers(game) => Self::UpdateBlinds(game.into()),
            Self::UpdateBlinds(game) => Self::BootPlayers(game.into()),
            Self::BootPlayers(game) => Self::Lobby(game.into()),
        }
    }

    pub fn take_action(
        &mut self,
        username: &Username,
        action: Action,
    ) -> Result<Action, UserError> {
        match self {
            Self::TakeAction(game) if !game.is_ready_for_next_phase() && game.is_turn(username) => {
                let sanitized_action = game.act(action)?;
                Ok(sanitized_action)
            }
            _ => Err(UserError::OutOfTurnAction),
        }
    }
}

impl From<GameSettings> for PokerState {
    fn from(value: GameSettings) -> Self {
        let game: Game<Lobby> = value.into();
        Self::Lobby(game)
    }
}

#[cfg(test)]
mod game_tests {
    use super::{
        BootPlayers, CollectBlinds, Deal, DistributePot, Flop, Game, Lobby, MoveButton,
        PhaseDependentUserManagement, PhaseIndependentUserManagement, RemovePlayers, River,
        SeatPlayers, ShowHands, TakeAction, Turn, UpdateBlinds, UserError,
        entities::{Action, ActionChoice, Card, PlayerState, Suit, Username},
    };

    fn init_2_player_game() -> Game<SeatPlayers> {
        let game = Game::<Lobby>::new();
        let mut game: Game<SeatPlayers> = game.into();
        for i in 0..2 {
            let username = i.to_string().into();
            game.new_user(&username).unwrap();
            game.waitlist_user(&username).unwrap();
        }
        game
    }

    fn init_3_player_game() -> Game<SeatPlayers> {
        let game = Game::<Lobby>::new();
        let mut game: Game<SeatPlayers> = game.into();
        for i in 0..3 {
            let username = i.to_string().into();
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
        assert_eq!(game.data.deck.deck_idx, 2 * game.get_num_users());
        for player in &game.data.players {
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
            game.data.settings.buy_in + game.data.blinds.small + game.data.blinds.big,
            game.data.settings.buy_in - game.data.blinds.small,
            game.data.settings.buy_in - game.data.blinds.big,
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
        assert_eq!(game.act(Action::Call), Ok(Action::Call));
        assert_eq!(game.get_next_action_choices(), None);
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
            game.get_next_action_choices(),
            Some([ActionChoice::AllIn, ActionChoice::Fold,].into())
        );
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_choices(),
            Some([ActionChoice::AllIn, ActionChoice::Fold,].into())
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
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(195),
                    ActionChoice::Fold,
                ]
                .into()
            )
        );
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_choices(),
            Some([ActionChoice::Call(390), ActionChoice::Fold,].into())
        );
        assert_eq!(game.act(Action::Call), Ok(Action::Call));
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
        let username = Username::new("ognf");

        assert_eq!(game.new_user(&username), Ok(true));
        assert!(game.contains_spectator(&username));

        assert_eq!(game.new_user(&username), Err(UserError::UserAlreadyExists));

        assert_eq!(game.waitlist_user(&username), Ok(Some(true)));
        assert!(game.contains_waitlister(&username));

        assert_eq!(game.spectate_user(&username), Ok(Some(true)));
        assert!(game.contains_spectator(&username));

        assert_eq!(game.remove_user(&username), Ok(Some(true)));
        assert!(!game.contains_user(&username));

        assert_eq!(
            game.remove_user(&username),
            Err(UserError::UserDoesNotExist)
        );
        assert_eq!(
            game.waitlist_user(&username),
            Err(UserError::UserDoesNotExist)
        );
        assert_eq!(
            game.spectate_user(&username),
            Err(UserError::UserDoesNotExist)
        );

        assert_eq!(game.new_user(&username), Ok(true));
        assert!(game.contains_spectator(&username));

        assert_eq!(game.waitlist_user(&username), Ok(Some(true)));
        assert!(game.contains_waitlister(&username));

        assert_eq!(game.remove_user(&username), Ok(Some(true)));
        assert!(!game.contains_user(&username));

        for i in 0..game.data.settings.max_users {
            assert_eq!(game.new_user(&i.to_string().into()), Ok(true));
        }
        assert_eq!(game.new_user(&username), Err(UserError::CapacityReached));
    }

    #[test]
    fn move_button() {
        let game = init_game_at_move_button();
        assert_eq!(game.data.play_positions.small_blind_idx, 1);
        assert_eq!(game.data.play_positions.big_blind_idx, 2);
        assert_eq!(game.data.play_positions.starting_action_idx, 0);
        assert_eq!(
            game.data.play_positions.next_action_idx,
            Some(game.data.play_positions.starting_action_idx)
        );
    }

    // Fill a game to capacity and then move the action index around.
    // Every player should get their turn.
    #[test]
    fn move_next_action_idx() {
        let game = Game::<Lobby>::new();
        let mut game: Game<SeatPlayers> = game.into();
        for i in 0..game.data.settings.max_users {
            let username = i.to_string().into();
            assert_eq!(game.new_user(&username), Ok(true));
            assert_eq!(game.waitlist_user(&username), Ok(Some(true)));
        }
        let game: Game<MoveButton> = game.into();
        let mut game: Game<CollectBlinds> = game.into();
        for i in 3..game.get_num_players() {
            assert_eq!(game.data.play_positions.next_action_idx, Some(i));
            game.data.play_positions.next_action_idx = game.get_next_action_idx(false);
        }
        for i in 0..3 {
            assert_eq!(game.data.play_positions.next_action_idx, Some(i));
            game.data.play_positions.next_action_idx = game.get_next_action_idx(false);
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
        let game: Game<UpdateBlinds> = game.into();
        let game: Game<BootPlayers> = game.into();
        assert_eq!(game.data.blinds.big, 3 * game.data.settings.min_big_blind);
        let game: Game<Lobby> = game.into();
        assert_eq!(game.get_num_players(), 1);
    }

    #[test]
    fn remove_player() {
        let mut game = init_game_at_showdown_with_2_all_ins();
        let username0 = Username::new("0");
        let username1 = Username::new("1");
        let username2 = Username::new("2");
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
        let mut game: Game<UpdateBlinds> = game.into();
        assert_eq!(game.remove_user(&username0), Ok(Some(true)));
        assert!(!game.contains_user(&username0));
        assert!(game.contains_player(&username1));
        assert!(game.contains_player(&username2));
        for i in 0..2 {
            assert_eq!(game.data.players[i].user.money, game.data.settings.buy_in);
        }
        let mut expected_open_seats = Vec::from_iter(3..game.data.settings.max_players);
        expected_open_seats.push(0);
        assert_eq!(game.data.open_seats, expected_open_seats);
    }

    #[test]
    fn remove_player_with_queue() {
        let mut game = init_game_at_showdown_with_2_all_ins();
        let username0 = Username::new("0");
        let username1 = Username::new("1");
        let username2 = Username::new("2");
        game.data.board = vec![
            Card(2, Suit::Diamond),
            Card(4, Suit::Diamond),
            Card(5, Suit::Diamond),
            Card(6, Suit::Diamond),
            Card(7, Suit::Diamond),
        ];
        game.data.players[1].cards = vec![Card(1, Suit::Heart), Card(7, Suit::Heart)];
        game.data.players[2].cards = vec![Card(2, Suit::Heart), Card(5, Suit::Heart)];
        assert_eq!(game.remove_user(&username0), Ok(Some(false)));
        let game: Game<DistributePot> = game.into();
        let game: Game<RemovePlayers> = game.into();
        let game: Game<UpdateBlinds> = game.into();
        assert!(!game.contains_user(&username0));
        assert!(game.contains_player(&username1));
        assert!(game.contains_player(&username2));
        for i in 0..2 {
            assert_eq!(game.data.players[i].user.money, game.data.settings.buy_in);
        }
        let mut expected_open_seats = Vec::from_iter(3..game.data.settings.max_players);
        expected_open_seats.push(0);
        assert_eq!(game.data.open_seats, expected_open_seats);
    }

    #[test]
    fn show_hands_after_checks() {
        let game = init_3_player_game();
        let game: Game<MoveButton> = game.into();
        let game: Game<CollectBlinds> = game.into();
        let game: Game<Deal> = game.into();
        let mut game: Game<TakeAction> = game.into();
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(game.act(Action::Call), Ok(Action::Call));
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
        for (i, state) in [PlayerState::Fold, PlayerState::Check, PlayerState::Check]
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
        assert_eq!(game.act(Action::Call), Ok(Action::Call));
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
        assert_eq!(
            game.act(Action::Raise(Some(20))),
            Ok(Action::Raise(Some(20)))
        );
        assert_eq!(game.act(Action::Call), Ok(Action::Call));
        let game: Game<ShowHands> = game.into();
        let game: Game<DistributePot> = game.into();
        for (i, state) in [PlayerState::Fold, PlayerState::Raise, PlayerState::Call]
            .iter()
            .enumerate()
        {
            assert_eq!(game.data.players[i].state, *state);
        }
    }

    #[test]
    fn seat_players() {
        let game = init_game_at_seat_players();
        let username0 = Username::new("0");
        let username1 = Username::new("1");
        let username2 = Username::new("2");
        assert_eq!(game.data.player_counts.num_active, game.get_num_players());
        assert!(game.contains_player(&username0));
        assert!(game.contains_player(&username1));
        assert!(game.contains_player(&username2));
    }

    #[test]
    fn take_action_2_all_ins() {
        let mut game = init_game_at_deal();
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(10),
                    ActionChoice::Fold,
                    ActionChoice::Raise(20)
                ]
                .into()
            )
        );
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_choices(),
            Some([ActionChoice::AllIn, ActionChoice::Fold].into())
        );
        assert_eq!(game.act(Action::AllIn), Ok(Action::AllIn));
        assert_eq!(
            game.get_next_action_choices(),
            Some([ActionChoice::AllIn, ActionChoice::Fold].into())
        );
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(game.get_next_action_choices(), None);
    }

    #[test]
    fn take_action_2_calls_1_check() {
        let mut game = init_game_at_deal();
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(10),
                    ActionChoice::Fold,
                    ActionChoice::Raise(20)
                ]
                .into()
            )
        );
        assert_eq!(game.act(Action::Call), Ok(Action::Call));
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(5),
                    ActionChoice::Fold,
                    ActionChoice::Raise(15)
                ]
                .into()
            )
        );
        assert_eq!(game.act(Action::Call), Ok(Action::Call));
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Check,
                    ActionChoice::Fold,
                    ActionChoice::Raise(20)
                ]
                .into()
            )
        );
        assert_eq!(game.act(Action::Check), Ok(Action::Check));
        assert_eq!(game.get_next_action_choices(), None);
    }

    #[test]
    fn take_action_2_folds() {
        let mut game = init_game_at_deal();
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(10),
                    ActionChoice::Fold,
                    ActionChoice::Raise(20)
                ]
                .into()
            )
        );
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(5),
                    ActionChoice::Fold,
                    ActionChoice::Raise(15)
                ]
                .into()
            )
        );
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(game.get_next_action_choices(), None);
    }

    #[test]
    fn take_action_2_reraises() {
        let mut game = init_game_at_deal();
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(10),
                    ActionChoice::Fold,
                    ActionChoice::Raise(20)
                ]
                .into()
            )
        );
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(5),
                    ActionChoice::Fold,
                    ActionChoice::Raise(15)
                ]
                .into()
            )
        );
        // Total call is 20
        assert_eq!(
            game.act(Action::Raise(Some(15))),
            Ok(Action::Raise(Some(15)))
        );
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(10),
                    ActionChoice::Fold,
                    ActionChoice::Raise(30)
                ]
                .into()
            )
        );
        // Total call is 40
        assert_eq!(
            game.act(Action::Raise(Some(30))),
            Ok(Action::Raise(Some(30)))
        );
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(20),
                    ActionChoice::Fold,
                    ActionChoice::Raise(60)
                ]
                .into()
            )
        );
        // Total call is 80
        assert_eq!(
            game.act(Action::Raise(Some(60))),
            Ok(Action::Raise(Some(60)))
        );
        assert_eq!(
            game.get_next_action_choices(),
            Some(
                [
                    ActionChoice::AllIn,
                    ActionChoice::Call(40),
                    ActionChoice::Fold,
                    ActionChoice::Raise(120)
                ]
                .into()
            )
        );
        assert_eq!(game.act(Action::Fold), Ok(Action::Fold));
        assert_eq!(game.get_next_action_choices(), None);
    }
}

#[cfg(test)]
mod state_tests {
    use super::{
        PhaseDependentUserManagement, PhaseIndependentUserManagement, PokerState, UserError,
        entities::{Action, Username},
    };

    fn init_state() -> PokerState {
        let mut state = PokerState::new();
        for i in 0..3 {
            let username = i.to_string().into();
            state.new_user(&username).unwrap();
            state.waitlist_user(&username).unwrap();
        }
        state
    }

    #[test]
    fn cant_start_game() {
        let mut state = init_state();
        let username0 = Username::new("0");
        let username1 = Username::new("1");
        let username2 = Username::new("2");
        assert_eq!(state.init_start(&username0), Ok(()));
        // At SeatPlayers.
        state = state.step();
        assert_eq!(
            state.init_start(&username0),
            Err(UserError::GameAlreadyStarting)
        );
        assert_eq!(state.remove_user(&username1), Ok(Some(true)));
        assert_eq!(state.remove_user(&username2), Ok(Some(true)));
        // Should be back at Lobby.
        state = state.step();
        assert_eq!(
            state.init_start(&username0),
            Err(UserError::NotEnoughPlayers)
        );
    }

    #[test]
    fn early_showdown_1_winner_2_early_folds() {
        let mut state = init_state();
        let username0 = Username::new("0");
        assert_eq!(state.init_start(&username0), Ok(()));
        state = state.step();
        assert!(matches!(state, PokerState::SeatPlayers(_)));
        state = state.step();
        assert!(matches!(state, PokerState::MoveButton(_)));
        state = state.step();
        assert!(matches!(state, PokerState::CollectBlinds(_)));
        state = state.step();
        assert!(matches!(state, PokerState::Deal(_)));
        state = state.step();
        assert!(matches!(state, PokerState::TakeAction(_)));
        // 1st fold
        state = state.step();
        // 2nd fold
        state = state.step();
        assert!(matches!(state, PokerState::Flop(_)));
        state = state.step();
        assert!(matches!(state, PokerState::Turn(_)));
        state = state.step();
        assert!(matches!(state, PokerState::River(_)));
        state = state.step();
        assert!(matches!(state, PokerState::ShowHands(_)));
        state = state.step();
        assert!(matches!(state, PokerState::DistributePot(_)));
        state = state.step();
        assert!(matches!(state, PokerState::RemovePlayers(_)));
        state = state.step();
        assert!(matches!(state, PokerState::UpdateBlinds(_)));
        state = state.step();
        assert!(matches!(state, PokerState::BootPlayers(_)));
        state = state.step();
        assert!(matches!(state, PokerState::Lobby(_)));
        assert_eq!(state.init_start(&username0), Ok(()));
    }

    #[test]
    fn early_showdown_1_winner_2_folds() {
        let mut state = init_state();
        let username0 = Username::new("0");
        assert_eq!(state.init_start(&username0), Ok(()));
        state = state.step();
        assert!(matches!(state, PokerState::SeatPlayers(_)));
        state = state.step();
        assert!(matches!(state, PokerState::MoveButton(_)));
        state = state.step();
        assert!(matches!(state, PokerState::CollectBlinds(_)));
        state = state.step();
        assert!(matches!(state, PokerState::Deal(_)));
        state = state.step();
        assert!(matches!(state, PokerState::TakeAction(_)));
        // All-in
        assert_eq!(
            state.take_action(&username0, Action::AllIn),
            Ok(Action::AllIn)
        );
        // 1st fold
        state = state.step();
        // 2nd fold
        state = state.step();
        assert!(matches!(state, PokerState::Flop(_)));
        state = state.step();
        assert!(matches!(state, PokerState::Turn(_)));
        state = state.step();
        assert!(matches!(state, PokerState::River(_)));
        state = state.step();
        assert!(matches!(state, PokerState::ShowHands(_)));
        state = state.step();
        assert!(matches!(state, PokerState::DistributePot(_)));
        state = state.step();
        assert!(matches!(state, PokerState::RemovePlayers(_)));
        state = state.step();
        assert!(matches!(state, PokerState::UpdateBlinds(_)));
        state = state.step();
        assert!(matches!(state, PokerState::BootPlayers(_)));
        state = state.step();
        assert!(matches!(state, PokerState::Lobby(_)));
        assert_eq!(state.init_start(&username0), Ok(()));
    }

    #[test]
    fn early_showdown_1_winner_2_late_folds() {
        let mut state = init_state();
        let username0 = Username::new("0");
        let username1 = Username::new("1");
        let username2 = Username::new("2");
        assert_eq!(state.init_start(&username0), Ok(()));
        state = state.step();
        assert!(matches!(state, PokerState::SeatPlayers(_)));
        state = state.step();
        assert!(matches!(state, PokerState::MoveButton(_)));
        state = state.step();
        assert!(matches!(state, PokerState::CollectBlinds(_)));
        state = state.step();
        assert!(matches!(state, PokerState::Deal(_)));
        state = state.step();
        assert!(matches!(state, PokerState::TakeAction(_)));
        // Call
        assert_eq!(
            state.take_action(&username0, Action::Call),
            Ok(Action::Call)
        );
        // Check
        assert_eq!(
            state.take_action(&username1, Action::Call),
            Ok(Action::Call)
        );
        // Check
        assert_eq!(
            state.take_action(&username2, Action::Check),
            Ok(Action::Check)
        );
        state = state.step();
        assert!(matches!(state, PokerState::Flop(_)));
        state = state.step();
        assert!(matches!(state, PokerState::TakeAction(_)));
        // Check
        assert_eq!(
            state.take_action(&username0, Action::Check),
            Ok(Action::Check)
        );
        // Check
        assert_eq!(
            state.take_action(&username1, Action::Check),
            Ok(Action::Check)
        );
        // Check
        assert_eq!(
            state.take_action(&username2, Action::Check),
            Ok(Action::Check)
        );
        state = state.step();
        assert!(matches!(state, PokerState::Turn(_)));
        state = state.step();
        assert!(matches!(state, PokerState::TakeAction(_)));
        assert_eq!(
            state.take_action(&username0, Action::Check),
            Ok(Action::Check)
        );
        assert_eq!(
            state.take_action(&username1, Action::Check),
            Ok(Action::Check)
        );
        assert_eq!(
            state.take_action(&username2, Action::Check),
            Ok(Action::Check)
        );
        state = state.step();
        assert!(matches!(state, PokerState::River(_)));
        state = state.step();
        assert!(matches!(state, PokerState::TakeAction(_)));
        assert_eq!(
            state.take_action(&username0, Action::AllIn),
            Ok(Action::AllIn)
        );
        assert_eq!(
            state.take_action(&username1, Action::Fold),
            Ok(Action::Fold)
        );
        assert_eq!(
            state.take_action(&username2, Action::Fold),
            Ok(Action::Fold)
        );
        state = state.step();
        assert!(matches!(state, PokerState::ShowHands(_)));
        state = state.step();
        assert!(matches!(state, PokerState::DistributePot(_)));
        state = state.step();
        assert!(matches!(state, PokerState::RemovePlayers(_)));
        state = state.step();
        assert!(matches!(state, PokerState::UpdateBlinds(_)));
        state = state.step();
        assert!(matches!(state, PokerState::BootPlayers(_)));
        state = state.step();
        assert!(matches!(state, PokerState::Lobby(_)));
        assert_eq!(state.init_start(&username0), Ok(()));
    }
}
