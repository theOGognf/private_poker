use anyhow::Error;
use std::{
    sync::mpsc::{channel, Receiver, Sender},
    time::{Duration, Instant},
};

use crate::poker::{
    game::{GameViews, UserError},
    PokerState,
};

use super::messages::{ClientCommand, ClientMessage, ServerMessage, ServerResponse, UserState};

pub const STATE_CHANGE_WAIT_DURATION: u64 = 5;
pub const TURN_SIGNAL_WAIT_DURATION: u64 = 10;

fn change_user_state(
    state: &mut PokerState,
    username: &str,
    user_state: &UserState,
) -> Result<GameViews, UserError> {
    match user_state {
        UserState::Play => state.waitlist_user(username),
        UserState::Spectate => state.spectate_user(username),
    }
}

// Lots of TODO: Just making sure server run generally works with the state
// mutation and references.
pub fn run() -> Result<(), Error> {
    let (_, rx_client): (Sender<ClientMessage>, Receiver<ClientMessage>) = channel();
    let (tx_server, _): (Sender<ServerMessage>, Receiver<ServerMessage>) = channel();

    let mut state = PokerState::new();
    let mut wait_duration = Duration::from_secs(STATE_CHANGE_WAIT_DURATION);
    loop {
        state = state.step();

        let views = state.get_views();
        let msg = ServerMessage::Views(views);
        tx_server.send(msg)?;

        if let (Some(username), Some(action_options)) =
            (state.get_next_action_username(), state.get_action_options())
        {
            let msg = ServerMessage::Response {
                username,
                data: Box::new(ServerResponse::TurnSignal(action_options)),
            };
            tx_server.send(msg)?;
            wait_duration = Duration::from_secs(TURN_SIGNAL_WAIT_DURATION);
        }

        while wait_duration.as_secs() > 0 {
            let start = Instant::now();
            if let Ok(msg) = rx_client.recv_timeout(wait_duration) {
                match msg.command {
                    ClientCommand::ChangeState(new_user_state) => {
                        match change_user_state(&mut state, &msg.username, &new_user_state) {
                            Ok(views) => {
                                let msg = ServerMessage::Views(views);
                                tx_server.send(msg)?;
                            }
                            Err(error) => {
                                let msg = ServerMessage::Response {
                                    username: msg.username,
                                    data: Box::new(ServerResponse::Error(error)),
                                };
                                tx_server.send(msg)?;
                            }
                        }
                    }
                    ClientCommand::Connect => match state.new_user(&msg.username) {
                        Ok(views) => {
                            let msg = ServerMessage::Views(views);
                            tx_server.send(msg)?;
                        }
                        Err(error) => {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    },
                    ClientCommand::Leave => match state.remove_user(&msg.username) {
                        Ok(views) => {
                            let msg = ServerMessage::Views(views);
                            tx_server.send(msg)?;
                        }
                        Err(error) => {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    },
                    ClientCommand::ShowHand => match state.show_hand(&msg.username) {
                        Ok(views) => {
                            let msg = ServerMessage::Views(views);
                            tx_server.send(msg)?;
                        }
                        Err(error) => {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    },
                    ClientCommand::StartGame => {
                        if let Err(error) = state.init_start(&msg.username) {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    }
                    ClientCommand::TakeAction(action) => {
                        match state.take_action(&msg.username, action) {
                            Ok(views) => {
                                let msg = ServerMessage::Views(views);
                                tx_server.send(msg)?;

                                if let (Some(username), Some(action_options)) =
                                    (state.get_next_action_username(), state.get_action_options())
                                {
                                    let msg = ServerMessage::Response {
                                        username,
                                        data: Box::new(ServerResponse::TurnSignal(action_options)),
                                    };
                                    tx_server.send(msg)?;
                                    wait_duration = Duration::from_secs(TURN_SIGNAL_WAIT_DURATION);
                                } else {
                                    wait_duration = Duration::from_secs(0);
                                }
                            }
                            Err(error) => {
                                let msg = ServerMessage::Response {
                                    username: msg.username,
                                    data: Box::new(ServerResponse::Error(error)),
                                };
                                tx_server.send(msg)?;
                            }
                        }
                    }
                }
            }
            wait_duration -= Instant::now() - start;
        }
    }
}
