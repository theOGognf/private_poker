use anyhow::Error;
use std::{
    sync::mpsc::{channel, Receiver, Sender},
    time::{Duration, Instant},
};

use crate::poker::{entities::UserState, game::UserError, PokerState};

use super::messages;

pub const STATE_CHANGE_WAIT_DURATION: u64 = 5;

fn change_user_state(
    state: &mut PokerState,
    username: &str,
    user_state: &UserState,
) -> Result<(), UserError> {
    match user_state {
        UserState::Playing => state.waitlist_user(username)?,
        UserState::Spectating => state.spectate_user(username)?,
        UserState::Waiting => state.waitlist_user(username)?,
    }
    Ok(())
}

// Lots of TODO: Just making sure server run generally works with the state
// mutation and references.
pub fn run() -> Result<(), Error> {
    let (_, rx_client): (
        Sender<messages::ClientMessage>,
        Receiver<messages::ClientMessage>,
    ) = channel();
    let (tx_server, _): (
        Sender<messages::ServerMessage>,
        Receiver<messages::ServerMessage>,
    ) = channel();

    let mut state = PokerState::new();
    loop {
        state = state.step();
        let mut wait_duration = Duration::from_secs(STATE_CHANGE_WAIT_DURATION);
        while wait_duration.as_secs() > 0 {
            let start = Instant::now();
            if let Ok(msg) = rx_client.recv_timeout(wait_duration) {
                match msg.command {
                    messages::ClientCommand::Connect => {
                        if let Err(error) = state.new_user(&msg.username) {
                            tx_server.send(messages::ServerMessage::Error {
                                username: msg.username,
                                error,
                            })?;
                        }
                    }
                    messages::ClientCommand::ChangeState(new_user_state) => {
                        if let Err(error) =
                            change_user_state(&mut state, &msg.username, &new_user_state)
                        {
                            tx_server.send(messages::ServerMessage::Error {
                                username: msg.username,
                                error,
                            })?;
                        }
                    }
                    messages::ClientCommand::ShowHand => {
                        if let Err(error) = state.show_hand(&msg.username) {
                            tx_server.send(messages::ServerMessage::Error {
                                username: msg.username,
                                error,
                            })?;
                        }
                    }
                    messages::ClientCommand::StartGame => {
                        if let Err(error) = state.init_game_start() {
                            tx_server.send(messages::ServerMessage::Error {
                                username: msg.username,
                                error,
                            })?;
                        }
                    }
                    messages::ClientCommand::TakeAction(action) => {
                        if let Err(error) = state.take_action(&msg.username, action) {
                            tx_server.send(messages::ServerMessage::Error {
                                username: msg.username,
                                error,
                            })?;
                        }
                    }
                }
            }
            wait_duration -= Instant::now() - start;
        }
    }
}
