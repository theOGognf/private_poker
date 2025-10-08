use rand::{seq::SliceRandom, thread_rng};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    borrow::Borrow,
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    fmt::{self},
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
            Self::Club => "c",
            Self::Spade => "s",
            Self::Diamond => "d",
            Self::Heart => "h",
            Self::Wild => "w",
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
            Self::HighCard => "hi",
            Self::OnePair => "1p",
            Self::TwoPair => "2p",
            Self::ThreeOfAKind => "3k",
            Self::Straight => "s8",
            Self::Flush => "fs",
            Self::FullHouse => "fh",
            Self::FourOfAKind => "4k",
            Self::StraightFlush => "sf",
        };
        write!(f, "{repr}")
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SubHand {
    pub rank: Rank,
    pub values: Vec<Value>,
}

#[derive(Debug)]
pub struct Deck {
    cards: [Card; 52],
    pub deck_idx: usize,
}

impl Deck {
    pub fn deal_card(&mut self) -> Card {
        let card = self.cards[self.deck_idx];
        self.deck_idx += 1;
        card
    }

    pub fn shuffle(&mut self) {
        self.cards.shuffle(&mut thread_rng());
        self.deck_idx = 0;
    }
}

impl Default for Deck {
    fn default() -> Self {
        let mut cards: [Card; 52] = [Card(0, Suit::Wild); 52];
        for (i, value) in (1u8..14u8).enumerate() {
            for (j, suit) in [Suit::Club, Suit::Spade, Suit::Diamond, Suit::Heart]
                .into_iter()
                .enumerate()
            {
                cards[4 * i + j] = Card(value, suit);
            }
        }
        Self { cards, deck_idx: 0 }
    }
}

/// Type alias for whole dollars. All bets and player stacks are represented
/// as whole dollars (there's no point arguing over pennies).
///
/// If the total money in a game ever surpasses ~4.2 billion, then we may
/// have a problem.
pub type Usd = u32;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Username(String);

impl Username {
    pub fn new(s: &str) -> Self {
        let mut username: String = s
            .chars()
            .map(|c| if c.is_ascii_whitespace() { '_' } else { c })
            .collect();
        username.truncate(constants::MAX_USER_INPUT_LENGTH / 2);
        Self(username)
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'de> Deserialize<'de> for Username {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::new(&s))
    }
}

impl From<String> for Username {
    fn from(value: String) -> Self {
        Self::new(&value)
    }
}

/// Type alias for seat positions during the game.
pub type SeatIndex = usize;

/// Play positions used for tracking who is paying what blinds and whose
/// turn is next.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PlayPositions {
    pub small_blind_idx: SeatIndex,
    pub big_blind_idx: SeatIndex,
    pub starting_action_idx: SeatIndex,
    pub next_action_idx: Option<SeatIndex>,
}

impl Default for PlayPositions {
    fn default() -> Self {
        Self {
            small_blind_idx: 0,
            big_blind_idx: 1,
            starting_action_idx: 2,
            next_action_idx: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct PlayerCounts {
    /// Count of the number of players active in a hand.
    /// All-in and folding are considered INACTIVE since they
    /// have no more moves to make. Once `num_players_called`
    /// is equal to this value, the round of betting is concluded.
    pub num_active: usize,
    /// Count of the number of players that have matched the minimum
    /// call. Coupled with `num_players_active`, this helps track
    /// whether a round of betting has ended. This value is reset
    /// at the beginning of each betting round and whenever a player
    /// raises (since they've increased the minimum call).
    pub num_called: usize,
}

#[derive(Debug, Default)]
pub struct PlayerQueues {
    /// Queue of users that've been voted to be kicked. We can't
    /// safely remove them from the game mid gameplay, so we instead queue
    /// them for removal.
    pub to_kick: BTreeSet<Username>,
    /// Queue of users that're playing the game but have opted
    /// to spectate. We can't safely remove them from the game mid gameplay,
    /// so we instead queue them for removal.
    pub to_spectate: BTreeSet<Username>,
    /// Queue of users that're playing the game but have opted
    /// to leave. We can't safely remove them from the game mid gameplay,
    /// so we instead queue them for removal.
    pub to_remove: BTreeSet<Username>,
    /// Queue of users whose money we'll reset. We can't safely
    /// reset them mid gameplay, so we instead queue them for reset.
    pub to_reset: BTreeSet<Username>,
}

// By default, a player will be cleaned if they fold 60 rounds with the big
// blind.
pub const DEFAULT_BUY_IN: Usd = 600;
pub const DEFAULT_MIN_BIG_BLIND: Usd = DEFAULT_BUY_IN / 60;
pub const DEFAULT_MIN_SMALL_BLIND: Usd = DEFAULT_MIN_BIG_BLIND / 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Blinds {
    pub small: Usd,
    pub big: Usd,
}

impl fmt::Display for Blinds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = format!("${}/{}", self.small, self.big);
        write!(f, "{repr}")
    }
}

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

impl Borrow<Username> for User {
    fn borrow(&self) -> &Username {
        &self.name
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Action {
    AllIn,
    Call,
    Check,
    Fold,
    Raise(Option<Usd>),
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match self {
            Self::AllIn => "all-ins (unhinged)",
            Self::Call => "calls",
            Self::Check => "checks",
            Self::Fold => "folds",
            Self::Raise(Some(amount)) => &format!("raises ${amount}"),
            Self::Raise(None) => "raises",
        };
        write!(f, "{repr}")
    }
}

impl From<Bet> for Action {
    fn from(value: Bet) -> Self {
        match value.action {
            BetAction::AllIn => Self::AllIn,
            BetAction::Call => Self::Call,
            BetAction::Raise => Self::Raise(Some(value.amount)),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ActionChoice {
    AllIn,
    Call(Usd),
    Check,
    Fold,
    Raise(Usd),
}

// Can't really convert a usize into an ActionChoice safely, and it doesn't
// really make sense to use a try- conversion version, so let's just
// use the into trait.
#[allow(clippy::from_over_into)]
impl Into<usize> for ActionChoice {
    fn into(self) -> usize {
        match self {
            Self::AllIn => 0,
            Self::Call(_) => 1,
            Self::Check => 2,
            Self::Fold => 3,
            Self::Raise(_) => 4,
        }
    }
}

impl fmt::Display for ActionChoice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match self {
            Self::AllIn => "all-in".to_string(),
            Self::Call(amount) => format!("call (== ${amount})"),
            Self::Check => "check".to_string(),
            Self::Fold => "fold".to_string(),
            Self::Raise(amount) => format!("raise (>= ${amount})"),
        };
        write!(f, "{repr}")
    }
}

// We don't care about the values within `ActionChoice::Call` and
// `ActionChoice::Raise`. We just perform checks against the enum
// variant to verify a user is choosing an action that's available
// within their presented action choices. Actual bet validation
// is done during the `TakeAction` game state.
impl Eq for ActionChoice {}

impl Hash for ActionChoice {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
    }
}

impl PartialEq for ActionChoice {
    fn eq(&self, other: &Self) -> bool {
        discriminant(self) == discriminant(other)
    }
}

impl From<ActionChoice> for Action {
    fn from(value: ActionChoice) -> Self {
        match value {
            ActionChoice::AllIn => Self::AllIn,
            ActionChoice::Call(_) => Self::Call,
            ActionChoice::Check => Self::Check,
            ActionChoice::Fold => Self::Fold,
            ActionChoice::Raise(amount) => Self::Raise(Some(amount)),
        }
    }
}

/// Type alias for a set of action choices.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ActionChoices(pub HashSet<ActionChoice>);

impl ActionChoices {
    pub fn contains(&self, action: &Action) -> bool {
        // ActionChoice uses variant discriminates for hashes, so we
        // don't need to care about the actual call/raise values.
        let action_choice: ActionChoice = match action {
            Action::AllIn => ActionChoice::AllIn,
            Action::Call => ActionChoice::Call(0),
            Action::Check => ActionChoice::Check,
            Action::Fold => ActionChoice::Fold,
            Action::Raise(_) => ActionChoice::Raise(0),
        };
        self.0.contains(&action_choice)
    }
}

impl fmt::Display for ActionChoices {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let num_options = self.0.len();
        let repr = self
            .0
            .iter()
            .enumerate()
            .map(|(i, action_choice)| {
                let repr = action_choice.to_string();
                match i {
                    0 if num_options == 1 => repr,
                    0 if num_options == 2 => format!("{repr} "),
                    0 if num_options >= 3 => format!("{repr}, "),
                    i if i == num_options - 1 && num_options != 1 => format!("or {repr}"),
                    _ => format!("{repr}, "),
                }
            })
            .collect::<String>();
        write!(f, "{repr}")
    }
}

impl<I> From<I> for ActionChoices
where
    I: IntoIterator<Item = ActionChoice>,
{
    fn from(iter: I) -> Self {
        Self(iter.into_iter().collect::<HashSet<_>>())
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
            Self::AllIn => "all-in",
            Self::Call => "call",
            Self::Check => "check",
            Self::Fold => "folded",
            Self::Raise => "raise",
            Self::Wait => "waiting",
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
    pub fn new(user: User, seat_idx: usize) -> Self {
        Self {
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
    pub fn new(max_players: usize) -> Self {
        Self {
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
            Self::Reset(Some(username)) => format!("reset {username}'s money"),
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
    pub blinds: Blinds,
    pub spectators: HashSet<User>,
    pub waitlist: VecDeque<User>,
    pub open_seats: VecDeque<usize>,
    pub players: Vec<PlayerView>,
    pub board: Vec<Card>,
    pub pot: PotView,
    pub play_positions: PlayPositions,
}

pub type GameViews = HashMap<Username, GameView>;
