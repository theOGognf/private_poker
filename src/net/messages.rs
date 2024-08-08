pub use crate::poker::game::GameView;
use crate::poker::{entities::Action, game::UserError};

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    Views(HashMap<String, GameView>),
}
