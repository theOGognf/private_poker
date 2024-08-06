use anyhow::{bail, Error};
use std::net::TcpStream;

use crate::poker::entities::{Action, UserState};

use super::{messages, utils};

pub struct Client {
    stream: TcpStream,
}

impl Client {
    pub fn take_action(&mut self, action: Action) -> Result<messages::GameView, Error> {
        let request = messages::ClientMessage::Action(action);
        if let Err(error) = utils::write_prefixed(&mut self.stream, &request) {
            bail!(error)
        }
        self.recv_view()
    }

    pub fn change_state(&mut self, state: UserState) -> Result<messages::GameView, Error> {
        let request = messages::ClientMessage::ChangeState(state);
        if let Err(error) = utils::write_prefixed(&mut self.stream, &request) {
            bail!(error)
        }
        self.recv_view()
    }

    pub fn connect(server_ip: &str, username: &str) -> Result<(Self, messages::GameView), Error> {
        match TcpStream::connect(server_ip) {
            Ok(mut stream) => {
                let request = messages::ClientMessage::Connect(username.to_string());
                if let Err(error) = utils::write_prefixed(&mut stream, &request) {
                    bail!(error)
                }
                match utils::read_prefixed::<messages::ServerMessage, TcpStream>(&mut stream) {
                    Ok(messages::ServerMessage::ActionSignal(_)) => {
                        bail!("Invalid server response.")
                    }
                    Ok(messages::ServerMessage::Error(error)) => bail!(error),
                    Ok(messages::ServerMessage::GameView(view)) => Ok((Self { stream }, view)),
                    Err(error) => bail!(error),
                }
            }
            Err(error) => bail!(error),
        }
    }

    pub fn recv(&mut self) -> Result<messages::ServerMessage, Error> {
        match utils::read_prefixed::<messages::ServerMessage, TcpStream>(&mut self.stream) {
            Ok(messages::ServerMessage::Error(error)) => bail!(error),
            Ok(msg) => Ok(msg),
            Err(error) => bail!(error),
        }
    }

    fn recv_view(&mut self) -> Result<messages::GameView, Error> {
        match utils::read_prefixed::<messages::ServerMessage, TcpStream>(&mut self.stream) {
            Ok(messages::ServerMessage::ActionSignal(_)) => {
                bail!("Invalid server response.")
            }
            Ok(messages::ServerMessage::Error(error)) => bail!(error),
            Ok(messages::ServerMessage::GameView(view)) => Ok(view),
            Err(error) => bail!(error),
        }
    }

    pub fn show_hand(&mut self) -> Result<messages::GameView, Error> {
        let request = messages::ClientMessage::Show;
        if let Err(error) = utils::write_prefixed(&mut self.stream, &request) {
            bail!(error)
        }
        self.recv_view()
    }

    pub fn start_game(&mut self) -> Result<messages::GameView, Error> {
        let request = messages::ClientMessage::Start;
        if let Err(error) = utils::write_prefixed(&mut self.stream, &request) {
            bail!(error)
        }
        self.recv_view()
    }
}
