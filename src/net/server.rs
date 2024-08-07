use anyhow::Error;
use std::{
    sync::mpsc::{channel, Receiver, Sender},
    time::{Duration, Instant},
};

use crate::poker::{entities::UserState, game::UserError, PokerState};

use super::messages::{ClientCommand, ClientMessage, ServerMessage, ServerResponse};

pub const STATE_CHANGE_WAIT_DURATION: u64 = 5;

fn change_user_state(
    state: &mut PokerState,
    username: &str,
    user_state: &UserState,
) -> Result<(), UserError> {
    match user_state {
        UserState::Playing(_) => state.waitlist_user(username)?,
        UserState::Spectating => state.spectate_user(username)?,
        UserState::Waiting => state.waitlist_user(username)?,
    }
    Ok(())
}

// Lots of TODO: Just making sure server run generally works with the state
// mutation and references.
pub fn run() -> Result<(), Error> {
    let (_, rx_client): (Sender<ClientMessage>, Receiver<ClientMessage>) = channel();
    let (tx_server, _): (Sender<ServerMessage>, Receiver<ServerMessage>) = channel();

    let mut state = PokerState::new();
    loop {
        state = state.step();
        let mut wait_duration = Duration::from_secs(STATE_CHANGE_WAIT_DURATION);
        while wait_duration.as_secs() > 0 {
            let start = Instant::now();
            if let Ok(msg) = rx_client.recv_timeout(wait_duration) {
                match msg.command {
                    ClientCommand::ChangeState(new_user_state) => {
                        if let Err(error) =
                            change_user_state(&mut state, &msg.username, &new_user_state)
                        {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    }
                    ClientCommand::Connect => {
                        if let Err(error) = state.new_user(&msg.username) {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    }
                    ClientCommand::Leave => {
                        if let Err(error) = state.remove_user(&msg.username) {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    }
                    ClientCommand::ShowHand => {
                        if let Err(error) = state.show_hand(&msg.username) {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    }
                    ClientCommand::StartGame => {
                        if let Err(error) = state.init_game_start() {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    }
                    ClientCommand::TakeAction(action) => {
                        if let Err(error) = state.take_action(&msg.username, action) {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    }
                }
            }
            wait_duration -= Instant::now() - start;
        }
    }
}
