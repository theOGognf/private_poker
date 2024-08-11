use anyhow::Error;
use std::{
    sync::mpsc::{channel, Receiver, Sender},
    time::{Duration, Instant},
};

use crate::poker::{game::UserError, PokerState};

use super::messages::{ClientCommand, ClientMessage, ServerMessage, ServerResponse, UserState};

pub const ACTION_TIMEOUT: Duration = Duration::from_secs(10);
pub const NO_TIMEOUT: Duration = Duration::from_secs(0);
pub const STEP_TIMEOUT: Duration = Duration::from_secs(5);

fn change_user_state(
    state: &mut PokerState,
    username: &str,
    user_state: &UserState,
) -> Result<(), UserError> {
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
    loop {
        state = state.step();

        let views = state.get_views();
        let msg = ServerMessage::Views(views);
        tx_server.send(msg)?;

        let mut next_action_username = state.get_next_action_username();
        let mut timeout = STEP_TIMEOUT;
        'command: loop {
            // Check if it's a user's turn. If so, send them a turn signal
            // and increase the timeout to give them time to make their
            // decision. We also keep track of their username so we
            // can tell if they don't make a decision in time.
            match (state.get_next_action_username(), state.get_action_options()) {
                (Some(username), Some(action_options)) => {
                    // Check if the username from the last turn is the same as the
                    // username from this turn. If so, we need to check if there
                    // was a timeout. If there's a timeout, then that means the
                    // user didn't make a decision, and they have to fold. The
                    // poker state will fold for them, so we just break.
                    if let Some(last_username) = next_action_username {
                        if timeout.as_secs() == 0 && last_username == username {
                            break 'command;
                        }
                    }
                    let msg = ServerMessage::Response {
                        username: username.clone(),
                        data: Box::new(ServerResponse::TurnSignal(action_options)),
                    };
                    tx_server.send(msg)?;
                    next_action_username = Some(username);
                    timeout = ACTION_TIMEOUT;
                }
                // If it's no one's turn and there's a timeout, then we must
                // break to update the poker state.
                _ => {
                    if timeout.as_secs() == 0 {
                        break 'command;
                    }
                }
            }

            while timeout.as_secs() > 0 {
                let start = Instant::now();
                if let Ok(msg) = rx_client.recv_timeout(timeout) {
                    let result = match msg.command {
                        ClientCommand::ChangeState(ref new_user_state) => {
                            change_user_state(&mut state, &msg.username, new_user_state)
                        }
                        ClientCommand::Connect => state.new_user(&msg.username),
                        ClientCommand::Leave => state.remove_user(&msg.username),
                        ClientCommand::ShowHand => state.show_hand(&msg.username),
                        ClientCommand::StartGame => state.init_start(&msg.username),
                        ClientCommand::TakeAction(ref action) => {
                            state.take_action(&msg.username, action.clone()).map(|()| {
                                timeout = NO_TIMEOUT;
                            })
                        }
                    };

                    // Get the result from a client's command. If their command
                    // is OK, update all clients with the client's command and
                    // the new game state. If their command is bad, send an
                    // error back to the client.
                    match result {
                        Ok(()) => {
                            let msg = ServerMessage::Ack(msg);
                            tx_server.send(msg)?;

                            let msg = ServerMessage::Views(state.get_views());
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
                timeout -= Instant::now() - start;
            }
        }
    }
}
