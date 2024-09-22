use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

pub use crate::game::entities::GameView;
use crate::game::{
    entities::{Action, Username},
    Game, TakeAction, UserError,
};

/// Errors due to the poker client's interaction with the poker server
/// and not from the user's particular action.
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

/// Type of user state change requests.
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

/// A user command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum UserCommand {
    /// The user wants to change their state (play or spectate).
    ChangeState(UserState),
    /// A new user wants to connect to the game.
    Connect,
    /// User wants to leave the game. This is really just a
    /// friendly courtesy and doesn't need to be sent by
    /// clients.
    Leave,
    /// User wants to show their hand. Can only occur if they're
    /// a player and the game is in a state that allows hands to
    /// be shown.
    ShowHand,
    /// User wants to start the game. Can only start a game when
    /// there are 2+ potential players.
    StartGame,
    /// User wants to make a bet. Can only occur if they're a
    /// player and it's their turn.
    TakeAction(Action),
}

impl fmt::Display for UserCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match &self {
            UserCommand::ChangeState(state) => &format!("joined the {state}s"),
            UserCommand::Connect => "connected",
            UserCommand::Leave => "left the game",
            UserCommand::ShowHand => "showed their hand",
            UserCommand::StartGame => "started the game",
            UserCommand::TakeAction(action) => &action.to_action_string(),
        };
        write!(f, "{repr}")
    }
}

/// A message from a poker client to the poker server, indicating some
/// type of user action or command request.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClientMessage {
    /// User the message is from.
    pub username: Username,
    /// Action the user is taking.
    pub command: UserCommand,
}

impl fmt::Display for ClientMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.username, self.command)
    }
}

/// A message from the poker server to a poker client.
#[derive(Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    /// An acknowledgement of a client message, signaling that the client's
    /// command was successfully processed by the game thread.
    Ack(ClientMessage),
    /// An indication that the poker client caused an error, resulting in
    /// the client's message not being processed correctly.
    ClientError(ClientError),
    /// The game state as viewed from the client's perspective.
    GameView(GameView),
    /// The game state represented as a string.
    Status(String),
    /// A sginal indicating that it's the user's turn.
    TurnSignal(HashSet<Action>),
    /// An indication that the poker client sent a message that was read
    /// properly, but the type of action that it relayed was invalid
    /// for the game state, resulting in a user error.
    UserError(UserError),
}

impl fmt::Display for ServerMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let repr = match &self {
            ServerMessage::Ack(msg) => msg.to_string(),
            ServerMessage::ClientError(error) => error.to_string(),
            ServerMessage::GameView(_) => "game view".to_string(),
            ServerMessage::Status(status) => status.to_string(),
            ServerMessage::TurnSignal(action_options) => {
                Game::<TakeAction>::action_options_to_string(action_options)
            }
            ServerMessage::UserError(error) => error.to_string(),
        };
        write!(f, "{repr}")
    }
}
