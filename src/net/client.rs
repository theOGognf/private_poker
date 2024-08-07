use anyhow::{bail, Error};
use std::net::TcpStream;

use crate::poker::entities::{Action, UserState};

use super::{
    messages::{ClientCommand, ClientMessage, GameView, ServerMessage},
    utils,
};

pub struct Client {
    stream: TcpStream,
    username: String,
}

impl Client {
    pub fn change_state(&mut self, state: UserState) -> Result<GameView, Error> {
        let request = ClientMessage {
            username: self.username.clone(),
            command: ClientCommand::ChangeState(state),
        };
        utils::write_prefixed(&mut self.stream, &request)?;
        self.recv_view()
    }

    pub fn connect(addr: &str, username: &str) -> Result<(Self, GameView), Error> {
        let mut stream = TcpStream::connect(addr)?;
        let request = ClientMessage {
            username: username.to_string(),
            command: ClientCommand::Connect,
        };
        utils::write_prefixed(&mut stream, &request)?;
        match utils::read_prefixed::<ServerMessage, TcpStream>(&mut stream) {
            Ok(ServerMessage::TurnSignal(_)) => {
                bail!("Invalid server response.")
            }
            Ok(ServerMessage::Error { error, .. }) => bail!(error),
            Ok(ServerMessage::GameView(view)) => Ok((
                Self {
                    stream,
                    username: username.to_string(),
                },
                view,
            )),
            Err(error) => bail!(error),
        }
    }

    pub fn recv(&mut self) -> Result<ServerMessage, Error> {
        match utils::read_prefixed::<ServerMessage, TcpStream>(&mut self.stream) {
            Ok(ServerMessage::Error { error, .. }) => bail!(error),
            Ok(msg) => Ok(msg),
            Err(error) => bail!(error),
        }
    }

    fn recv_view(&mut self) -> Result<GameView, Error> {
        match utils::read_prefixed::<ServerMessage, TcpStream>(&mut self.stream) {
            Ok(ServerMessage::TurnSignal(_)) => {
                bail!("Invalid server response.")
            }
            Ok(ServerMessage::Error { error, .. }) => bail!(error),
            Ok(ServerMessage::GameView(view)) => Ok(view),
            Err(error) => bail!(error),
        }
    }

    pub fn show_hand(&mut self) -> Result<GameView, Error> {
        let request = ClientMessage {
            username: self.username.to_string(),
            command: ClientCommand::ShowHand,
        };
        utils::write_prefixed(&mut self.stream, &request)?;
        self.recv_view()
    }

    pub fn start_game(&mut self) -> Result<GameView, Error> {
        let request = ClientMessage {
            username: self.username.to_string(),
            command: ClientCommand::StartGame,
        };
        utils::write_prefixed(&mut self.stream, &request)?;
        self.recv_view()
    }

    pub fn take_action(&mut self, action: Action) -> Result<GameView, Error> {
        let request = ClientMessage {
            username: self.username.to_string(),
            command: ClientCommand::TakeAction(action),
        };
        utils::write_prefixed(&mut self.stream, &request)?;
        self.recv_view()
    }
}
