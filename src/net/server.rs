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

pub const NO_WAIT_DURATION: Duration = Duration::from_secs(0);
pub const STATE_CHANGE_WAIT_DURATION: Duration = Duration::from_secs(5);
pub const TURN_SIGNAL_WAIT_DURATION: Duration = Duration::from_secs(10);

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
    let mut time_until_next_step = STATE_CHANGE_WAIT_DURATION;
    loop {
        state = state.step();

        let views = state.get_views();
        let msg = ServerMessage::Views(views);
        tx_server.send(msg)?;

        loop {
            if let (Some(username), Some(action_options)) =
                (state.get_next_action_username(), state.get_action_options())
            {
                let msg = ServerMessage::Response {
                    username,
                    data: Box::new(ServerResponse::TurnSignal(action_options)),
                };
                tx_server.send(msg)?;
                time_until_next_step = TURN_SIGNAL_WAIT_DURATION;
            }

            if time_until_next_step.as_secs() == 0 {
                break;
            }

            let start = Instant::now();
            if let Ok(msg) = rx_client.recv_timeout(time_until_next_step) {
                let result = match msg.command {
                    ClientCommand::ChangeState(new_user_state) => {
                        change_user_state(&mut state, &msg.username, &new_user_state)
                    }
                    ClientCommand::Connect => state.new_user(&msg.username),
                    ClientCommand::Leave => state.remove_user(&msg.username),
                    ClientCommand::ShowHand => state.show_hand(&msg.username),
                    ClientCommand::StartGame => state.init_start(&msg.username),
                    ClientCommand::TakeAction(action) => state.take_action(&msg.username, action),
                };
                match result {
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
            time_until_next_step -= Instant::now() - start;
        }
    }
}
