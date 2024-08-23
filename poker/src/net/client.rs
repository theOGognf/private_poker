use anyhow::{bail, Error};
use std::{net::TcpStream, thread, time::Duration};

use crate::game::entities::Action;

use super::{
    messages::{ClientCommand, ClientMessage, GameView, ServerResponse, UserState},
    utils,
};

pub const READ_TIMEOUT: Duration = Duration::from_secs(10);
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

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
        let mut connect_timeouts = vec![
            Duration::from_secs(1),
            Duration::from_millis(500),
            Duration::from_millis(100),
        ];
        while let Some(connect_timeout) = connect_timeouts.pop() {
            match TcpStream::connect_timeout(&addr, connect_timeout) {
                Ok(mut stream) => {
                    stream.set_read_timeout(Some(READ_TIMEOUT))?;
                    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
                    let msg = ClientMessage {
                        username: username.to_string(),
                        command: ClientCommand::Connect,
                    };
                    utils::write_prefixed(&mut stream, &msg)?;
                    // First receive the ack that the connection is OK.
                    match utils::read_prefixed::<ServerResponse, TcpStream>(&mut stream) {
                        Ok(ServerResponse::Ack(_)) => {}
                        Ok(ServerResponse::ClientError(error)) => bail!(error),
                        Ok(ServerResponse::UserError(error)) => bail!(error),
                        Ok(response) => {
                            bail!("Invalid server response: {response}.")
                        }
                        Err(error) => bail!(error),
                    }
                    // Then receive the game view.
                    match Client::recv_view(&mut stream) {
                        Ok(view) => {
                            return Ok((
                                Self {
                                    username: username.to_string(),
                                    stream,
                                },
                                view,
                            ))
                        }
                        Err(error) => bail!(error),
                    }
                }
                _ => thread::sleep(connect_timeout),
            }
        }
        bail!("Couldn't connect to {addr} as {username}.")
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
            Ok(ServerResponse::ClientError(error)) => bail!(error),
            Ok(ServerResponse::GameView(view)) => Ok(view),
            Ok(ServerResponse::UserError(error)) => bail!(error),
            Ok(response) => {
                bail!("Invalid server response: {response}.")
            }
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
