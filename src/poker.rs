pub mod constants;
pub mod entities;
pub mod functional;
pub mod game;

use entities::Action;
use game::{
    BootPlayers, CollectBlinds, Deal, DivideDonations, Flop, Game, MoveButton, RemovePlayers,
    River, SeatPlayers, Showdown, TakeAction, Turn, UpdateBlinds, UserError,
};

use std::backtrace;
use std::panic;

/// A poker finite state machine. State transitions are defined in
/// `PokerState::step`.
///
/// # Examples
///
/// ```
/// // Make a new poker game.
/// let mut state = PokerState::new();
///
/// // Create some users and waitlist them for play.
/// state = new_user(state, "foo").unwrap();
/// state = new_user(state, "bar").unwrap();
/// state = waitlist_user(state, "foo").unwrap();
/// state = waitlist_user(state, "bar").unwrap();
///
/// // Seat the players, move the button, collect blinds, and deal.
/// state = seat_players(state);
/// state = move_button(state);
/// state = collect_blinds(state);
/// state = deal(state);
///
/// // Players must take actions now. "foo" and "bar" both check.
/// // When using the poker game under a server, you may want to
/// // continue taking actions until the other returned values
/// // indicate that the betting round is over or that the game is
/// // ready for showdown.
/// (state, _, _) = take_action(state, Action::Check);
/// (state, _, _) = take_action(state, Action::Check);
///
/// // Here's the flop. Both players go all-in (unhinged).
/// state = flop(state);
/// (state, _, _) = take_action(state, Action::AllIn).unwrap();
/// (state, _, _) = take_action(state, Action::AllIn).unwrap();
///
/// // Continue to the showdown.
/// state = turn(state);
/// state = river(state);
/// state = showdown(state);
///
/// // Perform post-game duties.
/// state = remove_players(state);
/// state = divide_donations(state);
/// state = update_blinds(state);
/// boot_players(state);
/// ```
#[derive(Debug)]
pub enum PokerState {
    SeatPlayers(Game<SeatPlayers>),
    MoveButton(Game<MoveButton>),
    CollectBlinds(Game<CollectBlinds>),
    Deal(Game<Deal>),
    TakeAction(Game<TakeAction>),
    Flop(Game<Flop>),
    Turn(Game<Turn>),
    River(Game<River>),
    Showdown(Game<Showdown>),
    RemovePlayers(Game<RemovePlayers>),
    DivideDonations(Game<DivideDonations>),
    UpdateBlinds(Game<UpdateBlinds>),
    BootPlayers(Game<BootPlayers>),
}

/// Registered when a new poker state is instantiated. Provides helpful debug
/// info if an invalid state transition is made.
fn poker_state_transition_panic_hook(info: &std::panic::PanicInfo) {
    let backtrace = backtrace::Backtrace::capture();
    match backtrace.status() {
        backtrace::BacktraceStatus::Captured => {
            let msg = backtrace.to_string();
            eprintln!("{msg}");
        }
        backtrace::BacktraceStatus::Disabled => {
            eprintln!("Backtrace is disabled. Try setting `RUST_BACKTRACE=1` to see the backtrace.")
        }
        _ => eprintln!("Backtrase is not supported by this platform."),
    }

    let payload = info.payload();
    let state_verb = match payload.downcast_ref::<PokerState>() {
        Some(PokerState::SeatPlayers(_)) => "seat_players",
        Some(PokerState::MoveButton(_)) => "move_button",
        Some(PokerState::CollectBlinds(_)) => "collect_blinds",
        Some(PokerState::Deal(_)) => "deal",
        Some(PokerState::TakeAction(_)) => "take_action",
        Some(PokerState::Flop(_)) => "flop",
        Some(PokerState::Turn(_)) => "turn",
        Some(PokerState::River(_)) => "river",
        Some(PokerState::Showdown(_)) => "showdown",
        Some(PokerState::RemovePlayers(_)) => "remove_players",
        Some(PokerState::DivideDonations(_)) => "divide_donations",
        Some(PokerState::UpdateBlinds(_)) => "update_blinds",
        Some(PokerState::BootPlayers(_)) => "boot_players",
        None => {
            match payload.downcast_ref::<&str>() {
                Some(s) => eprintln!("{s}"),
                None => eprintln!("{payload:#?}"),
            }
            return;
        }
    };

    eprintln!("You attempted an invalid state transition. You should've transitioned with `{state_verb}`.");
}

impl PokerState {
    pub fn new() -> Self {
        panic::set_hook(Box::new(poker_state_transition_panic_hook));
        let game = Game::<SeatPlayers>::new();
        PokerState::SeatPlayers(game)
    }

    pub fn step(self) -> Self {
        match self {
            PokerState::SeatPlayers(game) => PokerState::MoveButton(game.into()),
            PokerState::MoveButton(game) => PokerState::CollectBlinds(game.into()),
            PokerState::CollectBlinds(game) => PokerState::Deal(game.into()),
            PokerState::Deal(game) => PokerState::TakeAction(game.into()),
            PokerState::TakeAction(game) => {
                if !game.is_ready_for_next_phase() {
                    panic!("The betting round isn't over yet - at least one player has yet to have their turn.")
                }
                match game.get_num_community_cards() {
                    0 => PokerState::Flop(game.into()),
                    3 => PokerState::Turn(game.into()),
                    4 => PokerState::River(game.into()),
                    5 => PokerState::Showdown(game.into()),
                    _ => unreachable!(
                        "There can only be 0, 3, 4, or 5 community cards on the board at a time."
                    ),
                }
            }
            PokerState::Flop(game) => {
                if game.is_ready_for_showdown() {
                    PokerState::Turn(game.into())
                } else {
                    PokerState::TakeAction(game.into())
                }
            }
            PokerState::Turn(game) => {
                if game.is_ready_for_showdown() {
                    PokerState::River(game.into())
                } else {
                    PokerState::TakeAction(game.into())
                }
            }
            PokerState::River(game) => {
                if game.is_ready_for_showdown() {
                    PokerState::Showdown(game.into())
                } else {
                    PokerState::TakeAction(game.into())
                }
            }
            PokerState::Showdown(game) => {
                if !game.is_pot_empty() {
                    panic!(
                        "The showdown isn't over yet - at least one pot has yet to be distributed."
                    )
                }
                PokerState::RemovePlayers(game.into())
            }
            PokerState::RemovePlayers(game) => PokerState::DivideDonations(game.into()),
            PokerState::DivideDonations(game) => PokerState::UpdateBlinds(game.into()),
            PokerState::UpdateBlinds(game) => PokerState::BootPlayers(game.into()),
            PokerState::BootPlayers(game) => PokerState::SeatPlayers(game.into()),
        }
    }
}

macro_rules! impl_user_managers {
    ($($name:ident),+) => {
        $(pub fn $name(mut state: PokerState, username: &str) -> Result<PokerState, UserError> {
            match state {
                PokerState::SeatPlayers(ref mut game) => {game.$name(username)?;},
                PokerState::MoveButton(ref mut game)  => {game.$name(username)?;},
                PokerState::CollectBlinds(ref mut game)  => {game.$name(username)?;},
                PokerState::Deal(ref mut game)  => {game.$name(username)?;},
                PokerState::TakeAction(ref mut game) => {game.$name(username)?;},
                PokerState::Flop(ref mut game)  => {game.$name(username)?;},
                PokerState::Turn(ref mut game)  => {game.$name(username)?;},
                PokerState::River(ref mut game)  => {game.$name(username)?;},
                PokerState::Showdown(ref mut game)  => {game.$name(username)?;},
                PokerState::RemovePlayers(ref mut game)  => {game.$name(username)?;},
                PokerState::DivideDonations(ref mut game)  =>{game.$name(username)?;},
                PokerState::UpdateBlinds(ref mut game)  => {game.$name(username)?;},
                PokerState::BootPlayers(ref mut game) => {game.$name(username)?;},
            }
            Ok(state)
        })*
    }
}

impl_user_managers!(new_user, remove_user, spectate_user, waitlist_user);

/// Finite state machine helpers.
///
/// You may ask yourself "What's the point of using these functions
/// for advancing the poker state when there's a built-in finite state
/// machine with `PokerState::step`?". The answer is: it's simply to
/// improve the readability of the code and to help guarantee
/// `PokerState::step` isn't called when it shouldn't be called.
///
/// As an example, let's say you're writing a server that uses a poker state,
/// and you instantiate and add users like so.
///
/// ```
/// let mut state = PokerState::new();
/// state = new_user(state, "foo").unwrap();
/// state = new_user(state, "bar").unwrap();
/// state = waitlist_user(state, "foo").unwrap();
/// state = waitlist_user(state, "bar").unwrap();
/// ```
///
/// Let's say you want to start the game and update the poker state.
/// Using the finite state machine directly, it's a bit confusing what
/// the final state is at a quick glance.
///
/// ```
/// state = state.step();
/// state = state.step();
/// state = state.step(); // What state is this??
/// ```
///
/// However, using these state transition helpers, the final state is a lot
/// more clear.
///
/// ```
/// state = seat_players(state);
/// state = move_button(state);
/// state = collect_blinds(state);
/// ```

pub fn seat_players(state: PokerState) -> PokerState {
    match state {
        PokerState::SeatPlayers(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn move_button(state: PokerState) -> PokerState {
    match state {
        PokerState::MoveButton(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn collect_blinds(state: PokerState) -> PokerState {
    match state {
        PokerState::CollectBlinds(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn deal(state: PokerState) -> PokerState {
    match state {
        PokerState::Deal(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn take_action(
    mut state: PokerState,
    action: Action,
) -> Result<(PokerState, bool, bool), UserError> {
    let (is_ready_for_next_phase, is_ready_for_showdown) = match state {
        PokerState::TakeAction(ref mut game) => {
            game.act(action)?;
            (game.is_ready_for_next_phase(), game.is_ready_for_showdown())
        }
        other => std::panic::panic_any(other),
    };
    Ok((state, is_ready_for_next_phase, is_ready_for_showdown))
}

pub fn flop(state: PokerState) -> PokerState {
    match state {
        PokerState::Flop(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn turn(state: PokerState) -> PokerState {
    match state {
        PokerState::Turn(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn river(state: PokerState) -> PokerState {
    match state {
        PokerState::River(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn showdown(mut state: PokerState) -> PokerState {
    match state {
        PokerState::Showdown(ref mut game) => {
            if !game.distribute() {
                state = state.step();
            }
        }
        other => std::panic::panic_any(other),
    }
    state
}

pub fn remove_players(state: PokerState) -> PokerState {
    match state {
        PokerState::RemovePlayers(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn divide_donations(state: PokerState) -> PokerState {
    match state {
        PokerState::DivideDonations(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn update_blinds(state: PokerState) -> PokerState {
    match state {
        PokerState::UpdateBlinds(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}

pub fn boot_players(state: PokerState) -> PokerState {
    match state {
        PokerState::BootPlayers(_) => state.step(),
        other => std::panic::panic_any(other),
    }
}
