use serde::{Deserialize, Serialize};
use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    hash::{Hash, Hasher},
    mem::discriminant,
};

use super::constants;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Suit {
    Club,
    Spade,
    Diamond,
    Heart,
    // Wild is used to initialize a deck of cards.
    // Might be a good choice for a joker's suit.
    Wild,
}

impl fmt::Display for Suit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match self {
            Suit::Club => "c",
            Suit::Spade => "s",
            Suit::Diamond => "d",
            Suit::Heart => "h",
            Suit::Wild => "w",
        };
        write!(f, "{repr}")
    }
}

/// Placeholder for card values.
pub type Value = u8;

/// A card is a tuple of a uInt8 value (ace=1u8 ... ace=14u8)
/// and a suit. A joker is depicted as 0u8.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Card(pub Value, pub Suit);

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = match self.0 {
            1 | 14 => "A",
            11 => "J",
            12 => "Q",
            13 => "K",
            v => &v.to_string(),
        };
        let repr = format!("{value}/{}", self.1);
        write!(f, "{repr:>4}")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Rank {
    HighCard,
    OnePair,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            Rank::HighCard => "hi",
            Rank::OnePair => "1p",
            Rank::TwoPair => "2p",
            Rank::ThreeOfAKind => "3k",
            Rank::Straight => "s8",
            Rank::Flush => "fs",
            Rank::FullHouse => "fh",
            Rank::FourOfAKind => "4k",
            Rank::StraightFlush => "sf",
        };
        write!(f, "{repr}")
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SubHand {
    pub rank: Rank,
    pub values: Vec<Value>,
}

impl fmt::Display for SubHand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = self
            .values
            .iter()
            .map(|v| match v {
                1 | 14 => "A".to_string(),
                11 => "J".to_string(),
                12 => "Q".to_string(),
                13 => "K".to_string(),
                v => v.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" ");
        write!(f, "{} {repr}", self.rank)
    }
}

/// Type alias for whole dollars. All bets and player stacks are represented
/// as whole dollars (there's no point arguing over pennies).
///
/// If the total money in a game ever surpasses ~4.2 billion, then we may
/// have a problem.
pub type Usd = u32;
/// Type alias for decimal dollars. Only used to represent the remainder of
/// whole dollars in the cases where whole dollars can't be distributed evenly
/// amongst users.
pub type Usdf = f32;

/// Type alias for poker user usernames.
pub type Username = String;

// By default, a player will be cleaned if they fold 20 rounds with the big
// blind.
pub const DEFAULT_BUY_IN: Usd = 200;
pub const DEFAULT_MIN_BIG_BLIND: Usd = DEFAULT_BUY_IN / 20;
pub const DEFAULT_MIN_SMALL_BLIND: Usd = DEFAULT_MIN_BIG_BLIND / 2;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct User {
    pub name: Username,
    pub money: Usd,
}

impl Hash for User {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Borrow<str> for User {
    fn borrow(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let money = format!("${}", self.money);
        write!(f, "{:16} {:>5}", self.name, money)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Action {
    AllIn,
    Call(Usd),
    Check,
    Fold,
    Raise(Usd),
}

// Can't really convert a usize into an Action safely, and it doesn't
// really make sense to use a try- conversion version, so let's just
// use the into trait.
#[allow(clippy::from_over_into)]
impl Into<usize> for Action {
    fn into(self) -> usize {
        match self {
            Action::AllIn => 0,
            Action::Call(_) => 1,
            Action::Check => 2,
            Action::Fold => 3,
            Action::Raise(_) => 4,
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match self {
            Action::AllIn => "all-in",
            Action::Call(amount) => &format!("call ${amount}"),
            Action::Check => "check",
            Action::Fold => "fold",
            Action::Raise(amount) => &format!("raise ${amount}"),
        };
        write!(f, "{repr}")
    }
}

impl From<Bet> for Action {
    fn from(value: Bet) -> Self {
        match value.action {
            BetAction::AllIn => Action::AllIn,
            BetAction::Call => Action::Call(value.amount),
            BetAction::Raise => Action::Raise(value.amount),
        }
    }
}

impl Action {
    #[must_use]
    pub fn to_action_string(&self) -> String {
        match self {
            Action::AllIn => format!("{self}s (unhinged)"),
            Action::Check | Action::Fold => format!("{self}s"),
            Action::Call(amount) => format!("calls ${amount}"),
            Action::Raise(amount) => format!("raises ${amount}"),
        }
    }

    #[must_use]
    pub fn to_option_string(&self) -> String {
        match self {
            Action::AllIn | Action::Check | Action::Fold => self.to_string(),
            Action::Call(amount) => format!("call (== ${amount})"),
            Action::Raise(amount) => format!("raise (>= ${amount})"),
        }
    }
}

// We don't care about the values within `Action::Call` and
// `Action::Raise`. We just perform checks against the enum
// variant to verify a user is choosing an action that's available
// within their presented action options. Actual bet validation
// is done during the `TakeAction` game state.
impl Eq for Action {}

impl Hash for Action {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
    }
}

impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        discriminant(self) == discriminant(other)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BetAction {
    AllIn,
    Call,
    Raise,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Bet {
    pub action: BetAction,
    pub amount: Usd,
}

impl fmt::Display for Bet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let amount = self.amount;
        let repr = match self.action {
            BetAction::AllIn => format!("all-in of ${amount}"),
            BetAction::Call => format!("call of ${amount}"),
            BetAction::Raise => format!("raise of ${amount}"),
        };
        write!(f, "{repr}")
    }
}

/// For users that're in a pot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum PlayerState {
    // Player put in their whole stack.
    AllIn,
    // Player calls.
    Call,
    // Player checks.
    Check,
    // Player forfeited their stack for the pot.
    Fold,
    // Player raises and is waiting for other player actions.
    Raise,
    // Player is in the pot but is waiting for their move.
    Wait,
}

impl fmt::Display for PlayerState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match self {
            PlayerState::AllIn => "all-in",
            PlayerState::Call => "call",
            PlayerState::Check => "check",
            PlayerState::Fold => "folded",
            PlayerState::Raise => "raise",
            PlayerState::Wait => "waiting",
        };
        write!(f, "{repr:7}")
    }
}

#[derive(Clone, Debug)]
pub struct Player {
    pub user: User,
    pub state: PlayerState,
    pub cards: Vec<Card>,
    pub showing: bool,
    pub seat_idx: usize,
}

impl Player {
    #[must_use]
    pub fn new(user: User, seat_idx: usize) -> Player {
        Player {
            user,
            state: PlayerState::Wait,
            cards: Vec::with_capacity(2),
            showing: false,
            seat_idx,
        }
    }

    pub fn reset(&mut self) {
        self.state = PlayerState::Wait;
        self.cards.clear();
        self.showing = false;
    }
}

#[derive(Clone, Debug)]
pub struct Pot {
    // Map seat indices (players) to their investment in the pot.
    pub investments: HashMap<usize, Usd>,
}

impl Default for Pot {
    fn default() -> Self {
        Self::new(constants::MAX_PLAYERS)
    }
}

impl Pot {
    pub fn bet(&mut self, player_idx: usize, bet: &Bet) {
        let investment = self.investments.entry(player_idx).or_default();
        *investment += bet.amount;
    }

    #[must_use]
    pub fn get_call(&self) -> Usd {
        *self.investments.values().max().unwrap_or(&0)
    }

    /// Return the amount the player must bet to remain in the hand, and
    /// the minimum the player must raise by for it to be considered
    /// a valid raise.
    #[must_use]
    pub fn get_call_by_player_idx(&self, player_idx: usize) -> Usd {
        self.get_call() - self.get_investment_by_player_idx(player_idx)
    }

    /// Return the amount the player has invested in the pot.
    #[must_use]
    pub fn get_investment_by_player_idx(&self, player_idx: usize) -> Usd {
        *self.investments.get(&player_idx).unwrap_or(&0)
    }

    /// Return the minimum amount a player has to bet in order for their
    /// raise to be considered a valid raise.
    #[must_use]
    pub fn get_min_raise_by_player_idx(&self, player_idx: usize) -> Usd {
        2 * self.get_call() - self.get_investment_by_player_idx(player_idx)
    }

    #[must_use]
    pub fn get_size(&self) -> Usd {
        self.investments.values().sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.get_size() == 0
    }

    #[must_use]
    pub fn new(max_players: usize) -> Pot {
        Pot {
            investments: HashMap::with_capacity(max_players),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Vote {
    // Vote to kick another user.
    Kick(Username),
    // Vote to reset money (for a specific user or for everyone).
    Reset(Option<Username>),
}

impl fmt::Display for Vote {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match self {
            Self::Kick(username) => format!("kick {username}"),
            Self::Reset(None) => "reset everyone's money".to_string(),
            Self::Reset(Some(username)) => format!("reset {username}'s money").to_string(),
        };
        write!(f, "{repr}")
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlayerView {
    pub user: User,
    pub state: PlayerState,
    pub cards: Vec<Card>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PotView {
    pub size: Usd,
}

impl fmt::Display for PotView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${}", self.size)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GameView {
    pub donations: Usdf,
    pub small_blind: Usd,
    pub big_blind: Usd,
    pub spectators: HashSet<User>,
    pub waitlist: VecDeque<User>,
    pub open_seats: VecDeque<usize>,
    pub players: Vec<PlayerView>,
    pub board: Vec<Card>,
    pub pot: PotView,
    pub small_blind_idx: usize,
    pub big_blind_idx: usize,
    pub next_action_idx: Option<usize>,
}

pub type GameViews = HashMap<Username, GameView>;
