//! A low-level TCP poker client.
//!
//! This client is blocking and so is primarily used as a testing utility
//! rather than an actual poker client.

use anyhow::{bail, Error};
use std::{net::TcpStream, thread, time::Duration};

use crate::game::{entities::Action, UserError};

use super::{
    messages::{ClientError, ClientMessage, GameView, ServerMessage, UserCommand, UserState},
    utils,
};

pub const READ_TIMEOUT: Duration = Duration::from_secs(10);
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

pub struct Client {
    pub username: String,
    pub addr: String,
    pub stream: TcpStream,
}

impl Client {
    pub fn change_state(&mut self, state: UserState) -> Result<(), Error> {
        let msg = ClientMessage {
            username: self.username.clone(),
            command: UserCommand::ChangeState(state),
        };
        utils::write_prefixed(&mut self.stream, &msg)?;
        Ok(())
    }

    pub fn connect(username: &str, addr: &str) -> Result<(Self, GameView), Error> {
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
                        command: UserCommand::Connect,
                    };
                    utils::write_prefixed(&mut stream, &msg)?;
                    Client::recv_ack(&mut stream)?;
                    // Then receive the game view.
                    match Client::recv_view(&mut stream) {
                        Ok(view) => {
                            return Ok((
                                Self {
                                    username: username.to_string(),
                                    addr: addr.to_string(),
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
        bail!("couldn't connect to {addr} as {username}")
    }

    pub fn recv(&mut self) -> Result<ServerMessage, Error> {
        match utils::read_prefixed::<ServerMessage, TcpStream>(&mut self.stream) {
            Ok(ServerMessage::ClientError(error)) => bail!(error),
            Ok(ServerMessage::UserError(error)) => bail!(error),
            Ok(msg) => Ok(msg),
            Err(error) => bail!(error),
        }
    }

    pub fn recv_ack(stream: &mut TcpStream) -> Result<(), Error> {
        match utils::read_prefixed::<ServerMessage, TcpStream>(stream) {
            Ok(ServerMessage::Ack(_)) => Ok(()),
            Ok(ServerMessage::ClientError(error)) => bail!(error),
            Ok(ServerMessage::UserError(error)) => bail!(error),
            Ok(response) => {
                bail!("invalid server response: {response}")
            }
            Err(error) => bail!(error),
        }
    }

    pub fn recv_client_error(stream: &mut TcpStream) -> Result<ClientError, Error> {
        match utils::read_prefixed::<ServerMessage, TcpStream>(stream) {
            Ok(ServerMessage::ClientError(error)) => Ok(error),
            Ok(response) => {
                bail!("invalid server response: {response}")
            }
            Err(error) => bail!(error),
        }
    }

    pub fn recv_user_error(stream: &mut TcpStream) -> Result<UserError, Error> {
        match utils::read_prefixed::<ServerMessage, TcpStream>(stream) {
            Ok(ServerMessage::UserError(error)) => Ok(error),
            Ok(response) => {
                bail!("invalid server response: {response}")
            }
            Err(error) => bail!(error),
        }
    }

    pub fn recv_view(stream: &mut TcpStream) -> Result<GameView, Error> {
        match utils::read_prefixed::<ServerMessage, TcpStream>(stream) {
            Ok(ServerMessage::ClientError(error)) => bail!(error),
            Ok(ServerMessage::GameView(view)) => Ok(view),
            Ok(ServerMessage::UserError(error)) => bail!(error),
            Ok(response) => {
                bail!("invalid server response: {response}")
            }
            Err(error) => bail!(error),
        }
    }

    pub fn show_hand(&mut self) -> Result<(), Error> {
        let msg = ClientMessage {
            username: self.username.to_string(),
            command: UserCommand::ShowHand,
        };
        utils::write_prefixed(&mut self.stream, &msg)?;
        Ok(())
    }

    pub fn start_game(&mut self) -> Result<(), Error> {
        let msg = ClientMessage {
            username: self.username.to_string(),
            command: UserCommand::StartGame,
        };
        utils::write_prefixed(&mut self.stream, &msg)?;
        Ok(())
    }

    pub fn take_action(&mut self, action: Action) -> Result<(), Error> {
        let msg = ClientMessage {
            username: self.username.to_string(),
            command: UserCommand::TakeAction(action),
        };
        utils::write_prefixed(&mut self.stream, &msg)?;
        Ok(())
    }
}
