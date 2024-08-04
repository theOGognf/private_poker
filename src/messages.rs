use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::poker::{
    constants::MAX_PLAYERS,
    entities::{Action, Card, PlayerState, Usd, Usdf, User, UserState},
};

type UserView = User;

#[derive(Debug, Deserialize, Serialize)]
pub struct PlayerView {
    name: String,
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
    users: HashMap<String, UserView>,
    spectators: HashSet<String>,
    waitlist: VecDeque<String>,
    seats: [Option<PlayerView>; MAX_PLAYERS],
    pots: Vec<PotView>,
    small_blind_idx: usize,
    big_blind_idx: usize,
    next_action_idx: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Action(Action),
    Connect,
    Show,
    StateRequest(UserState),
}
