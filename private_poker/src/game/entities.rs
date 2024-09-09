use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
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
        match self {
            Suit::Club => write!(f, "C"),
            Suit::Spade => write!(f, "S"),
            Suit::Diamond => write!(f, "D"),
            Suit::Heart => write!(f, "H"),
            Suit::Wild => write!(f, "W"),
        }
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
        let repr = format!("{value}{}", self.1);
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
            Rank::OnePair => "1pr",
            Rank::TwoPair => "2pr",
            Rank::ThreeOfAKind => "3k",
            Rank::Straight => "str8",
            Rank::Flush => "flush",
            Rank::FullHouse => "full",
            Rank::FourOfAKind => "4k",
            Rank::StraightFlush => "str8 flush",
        };
        write!(f, "{repr}")
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
        let repr = format!("{repr} {}", self.rank);
        write!(f, "{repr:16}")
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

// By default, a player will be cleaned if they fold 20 rounds with the big
// blind.
pub const DEFAULT_BUY_IN: Usd = 200;
pub const DEFAULT_MIN_BIG_BLIND: Usd = DEFAULT_BUY_IN / 20;
pub const DEFAULT_MIN_SMALL_BLIND: Usd = DEFAULT_MIN_BIG_BLIND / 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub name: String,
    pub money: Usd,
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let money = format!("${}", self.money);
        write!(f, "{:>10}: {:>6}", self.name, money)
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

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match self {
            Action::AllIn => "all-in".to_string(),
            Action::Call(amount) => format!("call ${amount}"),
            Action::Check => "check".to_string(),
            Action::Fold => "fold".to_string(),
            Action::Raise(amount) => format!("raise ${amount}"),
        };
        write!(f, "{repr}")
    }
}

impl Action {
    pub fn to_action_string(&self) -> String {
        match self {
            Action::AllIn | Action::Check | Action::Fold => format!("{self}s"),
            Action::Call(amount) => format!("calls ${amount}"),
            Action::Raise(amount) => format!("raises ${amount}"),
        }
    }

    pub fn to_option_string(&self) -> String {
        match self {
            Action::AllIn | Action::Check | Action::Fold => self.to_string(),
            Action::Call(amount) => format!("call (== ${})", amount),
            Action::Raise(amount) => format!("raise (>= ${})", amount),
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
        match self.action {
            BetAction::AllIn => write!(f, "all-in with ${amount}"),
            BetAction::Call => write!(f, "call with ${amount}"),
            BetAction::Raise => write!(f, "raise with ${amount}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SidePotState {
    AllIn,
    Raise,
}

/// For users that're in a pot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum PlayerState {
    // Player is in the pot but is waiting for their move.
    Wait,
    // Player put in their whole stack.
    AllIn,
    // Player forfeited their stack for the pot.
    Fold,
    // Player shows their cards at the end of the game.
    Show,
}

impl fmt::Display for PlayerState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match self {
            PlayerState::Wait => "waiting",
            PlayerState::AllIn => "all-in",
            PlayerState::Fold => "folded",
            PlayerState::Show => "showing",
        };
        write!(f, "{repr:7}")
    }
}

#[derive(Clone, Debug)]
pub struct Player {
    pub user: User,
    pub state: PlayerState,
    pub cards: Vec<Card>,
    pub seat_idx: usize,
}

impl Player {
    pub fn new(user: User, seat_idx: usize) -> Player {
        Player {
            user,
            state: PlayerState::Wait,
            cards: Vec::with_capacity(2),
            seat_idx,
        }
    }

    pub fn reset(&mut self) {
        self.state = PlayerState::Wait;
        self.cards.clear();
    }
}

#[derive(Clone, Debug)]
pub struct Pot {
    // The total investment for each player to remain in the hand.
    pub call: Usd,
    // Size is just the sum of all investments in the pot.
    pub size: Usd,
    // Map seat indices (players) to their investment in the pot.
    pub investments: HashMap<usize, Usd>,
    // Used to check whether to spawn a side pot from this pot.
    // Should be `None` if no side pot conditions are met.
    side_pot_state: Option<SidePotState>,
}

impl Default for Pot {
    fn default() -> Self {
        Self::new()
    }
}

impl Pot {
    pub fn bet(&mut self, player_idx: usize, bet: &Bet) -> Option<Pot> {
        let investment = self.investments.entry(player_idx).or_default();
        let mut new_call = self.call;
        let mut new_investment = *investment + bet.amount;
        let mut pot_increase = bet.amount;
        match bet.action {
            BetAction::Call => {}
            BetAction::Raise => {
                new_call = new_investment;
            }
            BetAction::AllIn => {
                if new_investment > self.call {
                    new_call = new_investment;
                }
            }
        }
        // Need to check whether a side pot is created. A side pot is created
        // when a player all-ins and then a subsequent player raises (an all-in
        // that is more than the previous all-in is considered a raise).
        // In this case, the call for the current pot remains unchanged, and
        // the pot is only increased by the original call. The excess
        // is used to start a new pot.
        let mut maybe_side_pot = None;
        match (bet.action, self.side_pot_state) {
            (BetAction::AllIn, None) => self.side_pot_state = Some(SidePotState::AllIn),
            (BetAction::AllIn, Some(SidePotState::AllIn))
            | (BetAction::Raise, Some(SidePotState::AllIn)) => {
                if new_investment > self.call {
                    self.side_pot_state = Some(SidePotState::Raise);
                    let mut side_pot = Pot::new();
                    side_pot.bet(
                        player_idx,
                        &Bet {
                            action: bet.action,
                            amount: new_investment - self.call,
                        },
                    );
                    maybe_side_pot = Some(side_pot);
                    // The call for the pot hasn't change.
                    new_call = self.call;
                    // The pot increase is just the pot's call remaining for the player.
                    pot_increase = self.call - *investment;
                    // The player has now matched the call for the pot.
                    new_investment = self.call;
                }
            }
            _ => {}
        }
        // Finally, update the call, the pot, and the player's investment
        // in the current pot.
        self.call = new_call;
        self.size += pot_increase;
        *investment = new_investment;
        maybe_side_pot
    }

    /// Return the amount the player must bet to remain in the hand, and
    /// the minimum the player must raise by for it to be considered
    /// a valid raise.
    pub fn get_call_by_player_idx(&self, player_idx: usize) -> Usd {
        self.call - self.get_investment_by_player_idx(player_idx)
    }

    /// Return the amount the player has invested in the pot.
    pub fn get_investment_by_player_idx(&self, player_idx: usize) -> Usd {
        self.investments
            .get(&player_idx)
            .copied()
            .unwrap_or_default()
    }

    pub fn new() -> Pot {
        Pot {
            call: 0,
            size: 0,
            investments: HashMap::with_capacity(constants::MAX_PLAYERS),
            side_pot_state: None,
        }
    }
}
