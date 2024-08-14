use anyhow::{bail, Error};
use mio::net::TcpStream;

use crate::poker::entities::Action;

use super::{
    messages::{ClientCommand, ClientMessage, GameView, ServerResponse, UserState},
    utils,
};

pub struct Client {
    pub username: String,
    pub stream: TcpStream,
}

impl Client {
    pub fn change_state(&mut self, state: UserState) -> Result<GameView, Error> {
        let msg = ClientMessage {
            username: self.username.clone(),
            command: ClientCommand::ChangeState(state),
        };
        utils::write_prefixed(&mut self.stream, &msg)?;
        Client::recv_view(&mut self.stream)
    }

    pub fn connect(addr: &str, username: &str) -> Result<(Self, GameView), Error> {
        let addr = addr.parse()?;
        let mut stream = TcpStream::connect(addr)?;
        let msg = ClientMessage {
            username: username.to_string(),
            command: ClientCommand::Connect,
        };
        utils::write_prefixed(&mut stream, &msg)?;
        // First receive the ack that the connection is OK.
        match utils::read_prefixed::<ServerResponse, TcpStream>(&mut stream) {
            Ok(ServerResponse::Ack(_)) => {}
            Ok(ServerResponse::ClientError(error)) => bail!(error),
            Ok(ServerResponse::GameView(_)) => bail!("Invalid server response."),
            Ok(ServerResponse::TurnSignal(_)) => {
                bail!("Invalid server response.")
            }
            Ok(ServerResponse::UserError(error)) => bail!(error),
            Err(error) => bail!(error),
        }
        // Then receive the game view.
        match Client::recv_view(&mut stream) {
            Ok(view) => Ok((
                Self {
                    username: username.to_string(),
                    stream,
                },
                view,
            )),
            Err(error) => bail!(error),
        }
    }

    pub fn recv(&mut self) -> Result<ServerResponse, Error> {
        match utils::read_prefixed::<ServerResponse, TcpStream>(&mut self.stream) {
            Ok(ServerResponse::UserError(error)) => bail!(error),
            Ok(msg) => Ok(msg),
            Err(error) => bail!(error),
        }
    }

    fn recv_view(stream: &mut TcpStream) -> Result<GameView, Error> {
        match utils::read_prefixed::<ServerResponse, TcpStream>(stream) {
            Ok(ServerResponse::Ack(_)) => {
                bail!("Invalid server response.")
            }
            Ok(ServerResponse::ClientError(error)) => bail!(error),
            Ok(ServerResponse::GameView(view)) => Ok(view),
            Ok(ServerResponse::TurnSignal(_)) => {
                bail!("Invalid server response.")
            }
            Ok(ServerResponse::UserError(error)) => bail!(error),
            Err(error) => bail!(error),
        }
    }

    pub fn show_hand(&mut self) -> Result<GameView, Error> {
        let msg = ClientMessage {
            username: self.username.to_string(),
            command: ClientCommand::ShowHand,
        };
        utils::write_prefixed(&mut self.stream, &msg)?;
        Client::recv_view(&mut self.stream)
    }

    pub fn start_game(&mut self) -> Result<GameView, Error> {
        let msg = ClientMessage {
            username: self.username.to_string(),
            command: ClientCommand::StartGame,
        };
        utils::write_prefixed(&mut self.stream, &msg)?;
        Client::recv_view(&mut self.stream)
    }

    pub fn take_action(&mut self, action: Action) -> Result<GameView, Error> {
        let msg = ClientMessage {
            username: self.username.to_string(),
            command: ClientCommand::TakeAction(action),
        };
        utils::write_prefixed(&mut self.stream, &msg)?;
        Client::recv_view(&mut self.stream)
    }
}
