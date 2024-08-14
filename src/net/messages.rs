pub use crate::poker::game::GameView;
use crate::poker::{entities::Action, game::UserError};

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Eq, Deserialize, thiserror::Error, PartialEq, Serialize)]
pub enum ClientError {
    #[error("Username already associated.")]
    AlreadyAssociated,
    #[error("Connection does not exist.")]
    DoesNotExist,
    #[error("Connection expired.")]
    Expired,
    #[error("Unassociated username.")]
    Unassociated,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum UserState {
    Play,
    Spectate,
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
    Ack(ClientMessage),
    ClientError(ClientError),
    GameView(GameView),
    TurnSignal(HashSet<Action>),
    UserError(UserError),
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Ack(ClientMessage),
    Response {
        username: String,
        data: Box<ServerResponse>,
    },
    Views(HashMap<String, GameView>),
}
