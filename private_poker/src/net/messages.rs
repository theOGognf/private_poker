use serde::{Deserialize, Serialize};
use std::fmt;

use super::super::game::{
    entities::{Action, ActionChoices, GameView, Username, Vote},
    Game, GameEvent, TakeAction, UserError,
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
    /// User disconnected. This is really just a
    /// friendly courtesy and doesn't need to be sent by
    /// clients.
    Disconnect,
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
    /// User wants to cast a vote.
    CastVote(Vote),
}

impl fmt::Display for UserCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match &self {
            UserCommand::ChangeState(state) => &format!("requested to join the {state}s"),
            UserCommand::Connect => "connected",
            UserCommand::Disconnect => "disconnected",
            UserCommand::ShowHand => "showed their hand",
            UserCommand::StartGame => "started the game",
            UserCommand::TakeAction(action) => &action.to_string(),
            UserCommand::CastVote(vote) => &format!("voted to {vote}"),
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
    /// An internal game event that can be shared with all clients.
    GameEvent(GameEvent),
    /// The game state as viewed from the client's perspective.
    GameView(GameView),
    /// The game state represented as a string.
    Status(String),
    /// A sginal indicating that it's the user's turn.
    TurnSignal(ActionChoices),
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
            ServerMessage::GameEvent(event) => event.to_string(),
            ServerMessage::GameView(_) => "game view".to_string(),
            ServerMessage::Status(status) => status.to_string(),
            ServerMessage::TurnSignal(action_choices) => {
                Game::<TakeAction>::action_choices_to_string(action_choices)
            }
            ServerMessage::UserError(error) => error.to_string(),
        };
        write!(f, "{repr}")
    }
}
