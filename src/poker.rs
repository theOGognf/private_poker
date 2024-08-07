pub mod constants;
pub mod entities;
pub mod functional;
pub mod game;

use entities::Action;
use game::{
    BootPlayers, CollectBlinds, Deal, DistributePot, DivideDonations, Flop, Game, Lobby,
    MoveButton, RemovePlayers, River, SeatPlayers, ShowHands, TakeAction, Turn, UpdateBlinds,
    UserError,
};

#[derive(Debug)]
pub enum PokerState {
    Lobby(Game<Lobby>),
    SeatPlayers(Game<SeatPlayers>),
    MoveButton(Game<MoveButton>),
    CollectBlinds(Game<CollectBlinds>),
    Deal(Game<Deal>),
    TakeAction(Game<TakeAction>),
    Flop(Game<Flop>),
    Turn(Game<Turn>),
    River(Game<River>),
    ShowHands(Game<ShowHands>),
    DistributePot(Game<DistributePot>),
    RemovePlayers(Game<RemovePlayers>),
    DivideDonations(Game<DivideDonations>),
    UpdateBlinds(Game<UpdateBlinds>),
    BootPlayers(Game<BootPlayers>),
}

impl PokerState {
    pub fn init_game_start(&mut self) -> Result<(), UserError> {
        match self {
            PokerState::Lobby(ref mut game) => game.init_game_start(),
            _ => Err(UserError::GameAlreadyInProgress),
        }
    }

    pub fn new() -> Self {
        let game = Game::<SeatPlayers>::new();
        PokerState::SeatPlayers(game)
    }

    pub fn show_hand(&mut self, username: &str) -> Result<(), UserError> {
        match self {
            PokerState::ShowHands(ref mut game) => game.show_hand(username),
            _ => Err(UserError::CannotShowHand),
        }
    }

    pub fn step(self) -> Self {
        match self {
            PokerState::Lobby(game) => {
                if game.is_ready_for_game_start() {
                    PokerState::SeatPlayers(game.into())
                } else {
                    PokerState::Lobby(game)
                }
            }
            PokerState::SeatPlayers(game) => {
                if game.get_num_potential_players() >= 2 {
                    PokerState::MoveButton(game.into())
                } else {
                    PokerState::Lobby(game.into())
                }
            }
            PokerState::MoveButton(game) => PokerState::CollectBlinds(game.into()),
            PokerState::CollectBlinds(game) => PokerState::Deal(game.into()),
            PokerState::Deal(game) => PokerState::TakeAction(game.into()),
            PokerState::TakeAction(mut game) => {
                if !game.is_ready_for_next_phase() {
                    match game.act(Action::Fold) {
                        Err(_) => unreachable!("Force folding is OK."),
                        _ => PokerState::TakeAction(game),
                    }
                } else {
                    match game.get_num_community_cards() {
                        0 => PokerState::Flop(game.into()),
                        3 => PokerState::Turn(game.into()),
                        4 => PokerState::River(game.into()),
                        5 => PokerState::ShowHands(game.into()),
                        _ => unreachable!(
                            "There can only be 0, 3, 4, or 5 community cards on the board at a time."
                        ),
                    }
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
                    PokerState::ShowHands(game.into())
                } else {
                    PokerState::TakeAction(game.into())
                }
            }
            PokerState::ShowHands(game) => {
                if !game.is_pot_empty() {
                    PokerState::DistributePot(game.into())
                } else {
                    PokerState::RemovePlayers(game.into())
                }
            }
            PokerState::DistributePot(game) => PokerState::ShowHands(game.into()),
            PokerState::RemovePlayers(game) => PokerState::DivideDonations(game.into()),
            PokerState::DivideDonations(game) => PokerState::UpdateBlinds(game.into()),
            PokerState::UpdateBlinds(game) => PokerState::BootPlayers(game.into()),
            PokerState::BootPlayers(game) => PokerState::Lobby(game.into()),
        }
    }

    pub fn take_action(&mut self, username: &str, action: Action) -> Result<(), UserError> {
        match self {
            PokerState::TakeAction(ref mut game)
                if !game.is_ready_for_next_phase() && game.is_turn(username) =>
            {
                game.act(action)?;
                Ok(())
            }
            _ => Err(UserError::OutOfTurnAction),
        }
    }
}

macro_rules! impl_user_managers {
    ($($name:ident),+) => {
        impl PokerState {
            $(pub fn $name(&mut self, username: &str) -> Result<(), UserError> {
                match self {
                    PokerState::Lobby(ref mut game) => {game.$name(username)?;},
                    PokerState::SeatPlayers(ref mut game) => {game.$name(username)?;},
                    PokerState::MoveButton(ref mut game)  => {game.$name(username)?;},
                    PokerState::CollectBlinds(ref mut game)  => {game.$name(username)?;},
                    PokerState::Deal(ref mut game)  => {game.$name(username)?;},
                    PokerState::TakeAction(ref mut game) => {game.$name(username)?;},
                    PokerState::Flop(ref mut game)  => {game.$name(username)?;},
                    PokerState::Turn(ref mut game)  => {game.$name(username)?;},
                    PokerState::River(ref mut game)  => {game.$name(username)?;},
                    PokerState::ShowHands(ref mut game)  => {game.$name(username)?;},
                    PokerState::DistributePot(ref mut game)  => {game.$name(username)?;},
                    PokerState::RemovePlayers(ref mut game)  => {game.$name(username)?;},
                    PokerState::DivideDonations(ref mut game)  =>{game.$name(username)?;},
                    PokerState::UpdateBlinds(ref mut game)  => {game.$name(username)?;},
                    PokerState::BootPlayers(ref mut game) => {game.$name(username)?;},
                }
                Ok(())
            })*
        }
    }
}

impl_user_managers!(new_user, remove_user, spectate_user, waitlist_user);
