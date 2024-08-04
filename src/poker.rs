pub mod constants;
pub mod entities;
pub mod functional;
pub mod game;

use entities::Action;
use game::{
    BootPlayers, CollectBlinds, Deal, DivideDonations, Flop, Game, MoveButton, RemovePlayers,
    River, SeatPlayers, Showdown, TakeAction, Turn, UpdateBlinds, UserError,
};

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
///
/// // Players must take actions now. "foo" calls and then "bar" checks.
/// state = deal(state);
/// state = take_action(state, Action::Call(5)).unwrap();
/// state = take_action(state, Action::Check).unwrap();
///
/// // Here's the flop. Both players go all-in (unhinged).
/// // If you're using the poker state as part of the backend for a poker server,
/// // you may want to use a pattern similar to this loop for requesting actions
/// // from clients.
/// state = flop(state);
/// while !state.is_ready_for_next_phase() {
///     state = take_action(state, Action::AllIn).unwrap();
/// }
///
/// // Continue to the showdown. We know we don't ened to check for the showdown
/// // or next phase since players are hard-coded to go all-in.
/// state = turn(state);
/// state = river(state);
///
/// // `reveal` and `distribute` are separate to allow clients time to see
/// // all the hands before money is distributed.
/// while !state.is_pot_empty() {
///     state = reveal_pot_hands(state);
///     state = distribute_pot(state);
/// }
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

impl PokerState {
    pub fn new() -> Self {
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

macro_rules! impl_state_helpers {
    ($($name:ident),+) => {
        $(impl PokerState {
            pub fn $name(&self) -> bool {
                match self {
                    PokerState::SeatPlayers(ref game) => game.$name(),
                    PokerState::MoveButton(ref game)  => game.$name(),
                    PokerState::CollectBlinds(ref game)  => game.$name(),
                    PokerState::Deal(ref game)  => game.$name(),
                    PokerState::TakeAction(ref game) => game.$name(),
                    PokerState::Flop(ref game)  => game.$name(),
                    PokerState::Turn(ref game)  => game.$name(),
                    PokerState::River(ref game)  => game.$name(),
                    PokerState::Showdown(ref game)  => game.$name(),
                    PokerState::RemovePlayers(ref game)  => game.$name(),
                    PokerState::DivideDonations(ref game)  =>game.$name(),
                    PokerState::UpdateBlinds(ref game)  => game.$name(),
                    PokerState::BootPlayers(ref game) => game.$name(),
                }
            }
        })*
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

impl_state_helpers!(is_pot_empty, is_ready_for_next_phase);

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
        other => panic!("{other:#?}"),
    }
}

pub fn move_button(state: PokerState) -> PokerState {
    match state {
        PokerState::MoveButton(_) => state.step(),
        other => panic!("{other:#?}"),
    }
}

pub fn collect_blinds(state: PokerState) -> PokerState {
    match state {
        PokerState::CollectBlinds(_) => state.step(),
        other => panic!("{other:#?}"),
    }
}

pub fn deal(state: PokerState) -> PokerState {
    match state {
        PokerState::Deal(_) => state.step(),
        other => panic!("{other:#?}"),
    }
}

pub fn take_action(mut state: PokerState, action: Action) -> Result<PokerState, UserError> {
    match state {
        PokerState::TakeAction(ref mut game) => {
            game.act(action)?;
        }
        other => panic!("{other:#?}"),
    };
    Ok(state)
}

pub fn flop(mut state: PokerState) -> PokerState {
    match state {
        PokerState::Flop(_) => state.step(),
        PokerState::TakeAction(_) => {
            state = state.step();
            state.step()
        }
        other => panic!("{other:#?}"),
    }
}

pub fn turn(mut state: PokerState) -> PokerState {
    match state {
        PokerState::TakeAction(_) => {
            state = state.step();
            state.step()
        }
        PokerState::Turn(_) => state.step(),
        other => panic!("{other:#?}"),
    }
}

pub fn river(mut state: PokerState) -> PokerState {
    match state {
        PokerState::River(_) => state.step(),
        PokerState::TakeAction(_) => {
            state = state.step();
            state.step()
        }
        other => panic!("{other:#?}"),
    }
}

pub fn reveal_pot_hands(mut state: PokerState) -> PokerState {
    match state {
        PokerState::Showdown(ref mut game) => {
            game.reveal_pot_hands();
        }
        other => panic!("{other:#?}"),
    }
    state
}

pub fn distribute_pot(mut state: PokerState) -> PokerState {
    match state {
        PokerState::Showdown(ref mut game) => {
            game.distribute_pot();
        }
        other => panic!("{other:#?}"),
    }
    state
}

pub fn remove_players(mut state: PokerState) -> PokerState {
    match state {
        PokerState::RemovePlayers(_) => state.step(),
        PokerState::Showdown(_) => {
            state = state.step();
            state.step()
        }
        other => panic!("{other:#?}"),
    }
}

pub fn divide_donations(state: PokerState) -> PokerState {
    match state {
        PokerState::DivideDonations(_) => state.step(),
        other => panic!("{other:#?}"),
    }
}

pub fn update_blinds(state: PokerState) -> PokerState {
    match state {
        PokerState::UpdateBlinds(_) => state.step(),
        other => panic!("{other:#?}"),
    }
}

pub fn boot_players(state: PokerState) -> PokerState {
    match state {
        PokerState::BootPlayers(_) => state.step(),
        other => panic!("{other:#?}"),
    }
}
