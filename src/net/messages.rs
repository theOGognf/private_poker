use crate::poker::{
    constants::MAX_PLAYERS,
    entities::{Action, Card, PlayerState, Usd, Usdf, User, UserState},
    game::{Game, UserError},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

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
    seats: Box<[Option<PlayerView>; MAX_PLAYERS]>,
    board: Vec<Card>,
    pots: Vec<PotView>,
    small_blind_idx: usize,
    big_blind_idx: usize,
    next_action_idx: Option<usize>,
}

impl GameView {
    pub fn is_pot_empty(&self) -> bool {
        self.pots.is_empty()
    }
}

impl<T> Game<T> {
    pub fn as_view(&self, seat_idx: usize) -> GameView {
        let mut seats = [const { None }; MAX_PLAYERS];
        for (idx, seat) in self.data.seats.iter().enumerate() {
            if let Some(player) = seat {
                let cards = if idx == seat_idx || player.state == PlayerState::Show {
                    Some(player.cards.clone())
                } else {
                    None
                };
                let player_view = PlayerView {
                    name: player.name.clone(),
                    state: player.state.clone(),
                    cards,
                };
                seats[idx] = Some(player_view);
            }
        }
        GameView {
            donations: self.data.donations,
            small_blind: self.data.small_blind,
            big_blind: self.data.big_blind,
            users: self.data.users.clone(),
            spectators: self.data.spectators.clone(),
            waitlist: self.data.waitlist.clone(),
            seats: Box::new(seats),
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
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Action(Action),
    Connect(String),
    ChangeState(UserState),
    Show,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    ActionSignal(HashSet<Action>),
    Error(UserError),
    GameView(GameView),
}
