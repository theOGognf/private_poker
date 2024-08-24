use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

pub use crate::game::GameView;
use crate::game::{entities::Action, UserError};

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
        match self {
            UserState::Play => write!(f, "waitlister"),
            UserState::Spectate => write!(f, "spectator"),
        }
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
        match &self {
            ClientCommand::ChangeState(state) => {
                write!(f, "joined the {}s", state)
            }
            ClientCommand::Connect => write!(f, "connected"),
            ClientCommand::Leave => write!(f, "left the game"),
            ClientCommand::ShowHand => write!(f, "showed their hand"),
            ClientCommand::StartGame => write!(f, "started the game"),
            ClientCommand::TakeAction(action) => {
                write!(f, "decided to {}", action)
            }
        }
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
    TurnSignal(HashSet<Action>),
    UserError(UserError),
}

impl fmt::Display for ServerResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            ServerResponse::Ack(msg) => write!(f, "{}", msg),
            ServerResponse::ClientError(error) => write!(f, "{}", error),
            ServerResponse::GameView(view) => write!(f, "{}", view),
            ServerResponse::TurnSignal(action_options) => {
                write!(f, "can ")?;
                let num_options = action_options.len();
                for (i, action) in action_options.iter().enumerate() {
                    match i {
                        0 if num_options == 1 => write!(f, "{}", action)?,
                        0 if num_options == 2 => write!(f, "{} ", action)?,
                        0 if num_options >= 3 => write!(f, "{}, ", action)?,
                        i if i == num_options - 1 && num_options != 1 => {
                            write!(f, "or {}", action)?
                        }
                        _ => write!(f, "{}, ", action)?,
                    }
                }
                Ok(())
            }
            ServerResponse::UserError(error) => write!(f, "{}", error),
        }
    }
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
