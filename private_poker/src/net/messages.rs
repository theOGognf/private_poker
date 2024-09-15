use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

pub use crate::game::GameView;
use crate::game::{entities::Action, Game, TakeAction, UserError};

#[derive(Debug, Deserialize, Eq, thiserror::Error, PartialEq, Serialize)]
pub enum ClientError {
    #[error("already associated")]
    AlreadyAssociated,
    #[error("does not exist")]
    DoesNotExist,
    #[error("expired")]
    Expired,
    #[error("unassociated")]
    Unassociated,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum UserState {
    Play,
    Spectate,
}

impl fmt::Display for UserState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match self {
            UserState::Play => "waitlister",
            UserState::Spectate => "spectator",
        };
        write!(f, "{repr}")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClientCommand {
    ChangeState(UserState),
    Connect,
    Leave,
    ShowHand,
    StartGame,
    TakeAction(Action),
}

impl fmt::Display for ClientCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match &self {
            ClientCommand::ChangeState(state) => &format!("joined the {state}s"),
            ClientCommand::Connect => "connected",
            ClientCommand::Leave => "left the game",
            ClientCommand::ShowHand => "showed their hand",
            ClientCommand::StartGame => "started the game",
            ClientCommand::TakeAction(action) => &action.to_action_string(),
        };
        write!(f, "{repr}")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClientMessage {
    pub username: String,
    pub command: ClientCommand,
}

impl fmt::Display for ClientMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.username, self.command)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ServerResponse {
    Ack(ClientMessage),
    ClientError(ClientError),
    GameView(GameView),
    Status(String),
    TurnSignal(HashSet<Action>),
    UserError(UserError),
}

impl fmt::Display for ServerResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match &self {
            ServerResponse::Ack(msg) => msg.to_string(),
            ServerResponse::ClientError(error) => error.to_string(),
            ServerResponse::GameView(view) => view.players_to_string(),
            ServerResponse::Status(status) => status.to_string(),
            ServerResponse::TurnSignal(action_options) => {
                Game::<TakeAction>::action_options_to_string(action_options)
            }
            ServerResponse::UserError(error) => error.to_string(),
        };
        write!(f, "{repr}")
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Ack(ClientMessage),
    Response {
        username: String,
        data: Box<ServerResponse>,
    },
    Status(String),
    Views(HashMap<String, GameView>),
}
