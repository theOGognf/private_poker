pub mod constants;
pub mod entities;
pub mod functional;
pub mod game;

use std::collections::HashSet;

use entities::Action;
use game::{
    BootPlayers, CollectBlinds, Deal, DistributePot, DivideDonations, Flop, Game, GameViews, Lobby,
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
    pub fn get_action_options(&self) -> Option<HashSet<Action>> {
        match self {
            PokerState::TakeAction(ref game) => game.get_action_options(),
            _ => None,
        }
    }

    pub fn get_next_action_username(&self) -> Option<String> {
        match self {
            PokerState::TakeAction(ref game) => game.get_next_action_username(),
            _ => None,
        }
    }

    pub fn get_views(&self) -> GameViews {
        match self {
            PokerState::Lobby(ref game) => game.get_views(),
            PokerState::SeatPlayers(ref game) => game.get_views(),
            PokerState::MoveButton(ref game) => game.get_views(),
            PokerState::CollectBlinds(ref game) => game.get_views(),
            PokerState::Deal(ref game) => game.get_views(),
            PokerState::TakeAction(ref game) => game.get_views(),
            PokerState::Flop(ref game) => game.get_views(),
            PokerState::Turn(ref game) => game.get_views(),
            PokerState::River(ref game) => game.get_views(),
            PokerState::ShowHands(ref game) => game.get_views(),
            PokerState::DistributePot(ref game) => game.get_views(),
            PokerState::RemovePlayers(ref game) => game.get_views(),
            PokerState::DivideDonations(ref game) => game.get_views(),
            PokerState::UpdateBlinds(ref game) => game.get_views(),
            PokerState::BootPlayers(ref game) => game.get_views(),
        }
    }

    pub fn init_start(&mut self, username: &str) -> Result<GameViews, UserError> {
        match self {
            PokerState::Lobby(ref mut game) => {
                if game.contains_waitlister(username) || game.contains_player(username) {
                    game.init_start()?;
                    Ok(game.get_views())
                } else {
                    Err(UserError::CannotStartGame)
                }
            }
            PokerState::SeatPlayers(_) => Err(UserError::GameAlreadyStarting),
            _ => Err(UserError::GameAlreadyInProgress),
        }
    }

    pub fn new() -> Self {
        let game = Game::<Lobby>::new();
        PokerState::Lobby(game)
    }

    pub fn show_hand(&mut self, username: &str) -> Result<GameViews, UserError> {
        match self {
            PokerState::ShowHands(ref mut game) => {
                game.show_hand(username)?;
                Ok(game.get_views())
            }
            _ => Err(UserError::CannotShowHand),
        }
    }

    pub fn step(self) -> Self {
        match self {
            PokerState::Lobby(game) => {
                if game.is_ready_to_start() {
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

    pub fn take_action(&mut self, username: &str, action: Action) -> Result<GameViews, UserError> {
        match self {
            PokerState::TakeAction(ref mut game)
                if !game.is_ready_for_next_phase() && game.is_turn(username) =>
            {
                game.act(action)?;
                Ok(game.get_views())
            }
            _ => Err(UserError::OutOfTurnAction),
        }
    }
}

macro_rules! impl_user_managers {
    ($($name:ident),+) => {
        impl PokerState {
            $(pub fn $name(&mut self, username: &str) -> Result<GameViews, UserError> {
                match self {
                    PokerState::Lobby(ref mut game) => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::SeatPlayers(ref mut game) => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::MoveButton(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::CollectBlinds(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::Deal(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::TakeAction(ref mut game) => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::Flop(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::Turn(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::River(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::ShowHands(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::DistributePot(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::RemovePlayers(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::DivideDonations(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::UpdateBlinds(ref mut game)  => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                    PokerState::BootPlayers(ref mut game) => {
                        game.$name(username)?;
                        Ok(game.get_views())
                    },
                }
            })*
        }
    }
}

impl_user_managers!(new_user, remove_user, spectate_user, waitlist_user);
