use crate::poker::{
    constants::MAX_PLAYERS,
    entities::{Action, Card, PlayerState, Usd, Usdf, User},
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

impl<T> Game<T> {
    pub fn as_view(&self, username: &str) -> GameView {
        let mut players = Vec::with_capacity(MAX_PLAYERS);
        for player in self.data.players.iter() {
            let cards = if player.user.name == username || player.state == PlayerState::Show {
                Some(player.cards.clone())
            } else {
                None
            };
            let player_view = PlayerView {
                name: player.user.name.clone(),
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
}

#[derive(Debug, Deserialize, Serialize)]
pub enum UserState {
    Spectating,
    Playing,
    Waiting,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ClientCommand {
    ChangeState(UserState),
    Connect,
    Leave,
    ShowHand,
    StartGame,
    TakeAction(Action),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ClientMessage {
    pub username: String,
    pub command: ClientCommand,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ServerResponse {
    Error(UserError),
    GameView(GameView),
    TurnSignal(HashSet<Action>),
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Response {
        username: String,
        data: Box<ServerResponse>,
    },
    Views {
        username_to_view: HashMap<String, GameView>,
    },
}
