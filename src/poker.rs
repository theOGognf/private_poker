pub mod functional;
pub mod game;

use game::{
    Action, BootPlayers, CollectBlinds, Deal, DivideDonations, Flop, Game, MoveButton,
    RemovePlayers, River, SeatPlayers, Showdown, TakeAction, Turn, UpdateBlinds, UserError,
};

/// A poker finite state machine. State transitions are defined in
/// `PokerState::step`.
///
/// # Examples
///
/// ```
/// // Make a new poker game.
/// let mut poker = PokerState::new();
///
/// // Create some users and waitlist them for play.
/// poker = new_user(poker, "foo").unwrap();
/// poker = new_user(poker, "bar").unwrap();
/// poker = waitlist_user(poker, "foo").unwrap();
/// poker = waitlist_user(poker, "bar").unwrap();
///
/// // Seat the players, move the button, collect blinds, and deal.
/// poker = seat_players(poker);
/// poker = move_button(poker);
/// poker = collect_blinds(poker);
/// poker = deal(poker);
///
/// // Players must take actions now. "foo" and "bar" both check.
/// // When using the poker game under a server, you may want to
/// // continue taking actions until the other returned value
/// // indicates that the betting round is over.
/// (poker, _) = take_action(poker, Action::Check);
/// (poker, _) = take_action(poker, Action::Check);
///
/// // Here's the flop. Both players go all-in (unhinged).
/// poker = flop(poker);
/// (poker, _) = take_action(poker, Action::AllIn).unwrap();
/// (poker, _) = take_action(poker, Action::AllIn).unwrap();
///
/// // Continue to the showdown.
/// poker = turn(poker);
/// poker = river(poker);
/// poker = showdown(poker);
///
/// // Perform post-game duties.
/// poker = remove_players(poker);
/// poker = divide_donations(poker);
/// poker = update_blinds(poker);
/// poker = boot_players(poker);
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

pub fn seat_players(state: PokerState) -> PokerState {
    match state {
        PokerState::SeatPlayers(_) => state.step(),
        _ => panic!(),
    }
}

pub fn move_button(state: PokerState) -> PokerState {
    match state {
        PokerState::MoveButton(_) => state.step(),
        _ => panic!(),
    }
}

pub fn collect_blinds(state: PokerState) -> PokerState {
    match state {
        PokerState::CollectBlinds(_) => state.step(),
        _ => panic!(),
    }
}

pub fn deal(state: PokerState) -> PokerState {
    match state {
        PokerState::Deal(_) => state.step(),
        _ => panic!(),
    }
}

pub fn take_action(mut state: PokerState, action: Action) -> Result<(PokerState, bool), UserError> {
    let is_ready_for_next_phase = match state {
        PokerState::TakeAction(ref mut game) => {
            game.act(action)?;
            game.is_ready_for_next_phase()
        }
        _ => panic!(),
    };
    Ok((state, is_ready_for_next_phase))
}

pub fn flop(state: PokerState) -> PokerState {
    match state {
        PokerState::Flop(_) => state.step(),
        _ => panic!(),
    }
}

pub fn turn(state: PokerState) -> PokerState {
    match state {
        PokerState::Turn(_) => state.step(),
        _ => panic!(),
    }
}

pub fn river(state: PokerState) -> PokerState {
    match state {
        PokerState::River(_) => state.step(),
        _ => panic!(),
    }
}

pub fn showdown(mut state: PokerState) -> PokerState {
    match state {
        PokerState::Showdown(ref mut game) => {
            if !game.distribute() {
                state = state.step();
            }
        }
        _ => panic!(),
    }
    state
}

pub fn remove_players(state: PokerState) -> PokerState {
    match state {
        PokerState::RemovePlayers(_) => state.step(),
        _ => panic!(),
    }
}

pub fn divide_donations(state: PokerState) -> PokerState {
    match state {
        PokerState::DivideDonations(_) => state.step(),
        _ => panic!(),
    }
}

pub fn update_blinds(state: PokerState) -> PokerState {
    match state {
        PokerState::UpdateBlinds(_) => state.step(),
        _ => panic!(),
    }
}

pub fn boot_players(state: PokerState) -> PokerState {
    match state {
        PokerState::BootPlayers(_) => state.step(),
        _ => panic!(),
    }
}
