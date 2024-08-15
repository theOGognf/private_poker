use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

pub use crate::poker::game::GameView;
use crate::poker::{entities::Action, game::UserError};

#[derive(Debug, Deserialize, Eq, thiserror::Error, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClientMessage {
    pub username: String,
    pub command: ClientCommand,
}

impl fmt::Display for ClientMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.command {
            ClientCommand::ChangeState(state) => {
                write!(f, "{} joined the {}s.", self.username, state)
            }
            ClientCommand::Connect => write!(f, "{} connected.", self.username),
            ClientCommand::Leave => write!(f, "{} left the game.", self.username),
            ClientCommand::ShowHand => write!(f, "{} showed their hand.", self.username),
            ClientCommand::StartGame => write!(f, "{} started the game.", self.username),
            ClientCommand::TakeAction(action) => {
                write!(f, "{} decided to {}.", self.username, action)
            }
        }
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
                write!(f, "It's your turn! You can ")?;
                let num_options = action_options.len();
                for (i, action) in action_options.iter().enumerate() {
                    match i {
                        0 if num_options == 1 => write!(f, "{}.", action)?,
                        0 if num_options == 2 => write!(f, "{} ", action)?,
                        0 if num_options >= 3 => write!(f, "{}, ", action)?,
                        i if i == num_options - 1 && num_options != 1 => {
                            write!(f, "or {}.", action)?
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
