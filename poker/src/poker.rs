mod constants;
pub mod entities;
pub mod functional;
pub mod game;

use std::collections::HashSet;

use entities::Action;
use game::{
    BootPlayers, CollectBlinds, Deal, DistributePot, DivideDonations, Flop, Game, GameSettings,
    GameViews, Lobby, MoveButton, RemovePlayers, River, SeatPlayers, ShowHands, TakeAction, Turn,
    UpdateBlinds, UserError,
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

impl Default for PokerState {
    fn default() -> Self {
        Self::new()
    }
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

    pub fn init_start(&mut self, username: &str) -> Result<(), UserError> {
        match self {
            PokerState::Lobby(ref mut game) => {
                if game.contains_waitlister(username) || game.contains_player(username) {
                    game.init_start()?;
                    Ok(())
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

    pub fn show_hand(&mut self, username: &str) -> Result<(), UserError> {
        match self {
            PokerState::DistributePot(ref mut game) => {
                game.show_hand(username)?;
                Ok(())
            }
            PokerState::ShowHands(ref mut game) => {
                game.show_hand(username)?;
                Ok(())
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
                if game.is_ready_for_next_phase() {
                    match game.get_num_community_cards() {
                        0 => PokerState::Flop(game.into()),
                        3 => PokerState::Turn(game.into()),
                        4 => PokerState::River(game.into()),
                        5 => PokerState::ShowHands(game.into()),
                        _ => unreachable!(
                            "There can only be 0, 3, 4, or 5 community cards on the board at a time."
                        ),
                    }
                } else {
                    match game.act(Action::Fold) {
                        Err(_) => unreachable!("Force folding is OK."),
                        _ => PokerState::TakeAction(game),
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
            PokerState::ShowHands(game) => PokerState::DistributePot(game.into()),
            PokerState::DistributePot(game) => {
                if game.get_num_pots() >= 2 {
                    PokerState::ShowHands(game.into())
                } else {
                    PokerState::RemovePlayers(game.into())
                }
            }
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
                    PokerState::Lobby(ref mut game) => {
                        game.$name(username)?;
                    },
                    PokerState::SeatPlayers(ref mut game) => {
                        game.$name(username)?;
                    },
                    PokerState::MoveButton(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::CollectBlinds(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::Deal(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::TakeAction(ref mut game) => {
                        game.$name(username)?;
                    },
                    PokerState::Flop(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::Turn(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::River(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::ShowHands(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::DistributePot(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::RemovePlayers(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::DivideDonations(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::UpdateBlinds(ref mut game)  => {
                        game.$name(username)?;
                    },
                    PokerState::BootPlayers(ref mut game) => {
                        game.$name(username)?;
                    },
                }
                Ok(())
            })*
        }
    }
}

impl_user_managers!(new_user, remove_user, spectate_user, waitlist_user);

impl From<GameSettings> for PokerState {
    fn from(value: GameSettings) -> Self {
        let game: Game<Lobby> = value.into();
        PokerState::Lobby(game)
    }
}

#[cfg(test)]
mod tests {
    use crate::poker::{entities::Action, game::UserError};

    use super::PokerState;

    fn init_state() -> PokerState {
        let mut state = PokerState::new();
        for i in 0..3 {
            let username = i.to_string();
            state.new_user(&username).unwrap();
            state.waitlist_user(&username).unwrap();
        }
        state
    }

    #[test]
    fn cant_start_game() {
        let mut state = init_state();
        assert_eq!(state.init_start("0"), Ok(()));
        // At SeatPlayers.
        state = state.step();
        assert_eq!(state.init_start("0"), Err(UserError::GameAlreadyStarting));
        assert_eq!(state.remove_user("1"), Ok(()));
        assert_eq!(state.remove_user("2"), Ok(()));
        // Should be back at Lobby.
        state = state.step();
        assert_eq!(state.init_start("0"), Err(UserError::NotEnoughPlayers));
    }

    #[test]
    fn early_showdown_1_winner_2_early_folds() {
        let mut state = init_state();
        assert_eq!(state.init_start("0"), Ok(()));
        // SeatPlayers
        state = state.step();
        // MoveButton
        state = state.step();
        // CollectBlinds
        state = state.step();
        // Deal
        state = state.step();
        // TakeAction
        state = state.step();
        // 1st fold
        state = state.step();
        // 2nd fold
        state = state.step();
        // Flop
        state = state.step();
        // Turn
        state = state.step();
        // River
        state = state.step();
        // ShowHands
        state = state.step();
        // DistributePot
        state = state.step();
        // RemovePlayers
        state = state.step();
        // DivideDonations
        state = state.step();
        // UpdateBlinds
        state = state.step();
        // BootPlayers
        state = state.step();
        // Lobby
        state = state.step();
        assert_eq!(state.init_start("0"), Ok(()));
    }

    #[test]
    fn early_showdown_1_winner_2_folds() {
        let mut state = init_state();
        assert_eq!(state.init_start("0"), Ok(()));
        // SeatPlayers
        state = state.step();
        // MoveButton
        state = state.step();
        // CollectBlinds
        state = state.step();
        // Deal
        state = state.step();
        // TakeAction
        state = state.step();
        // All-in
        assert_eq!(state.take_action("0", Action::AllIn), Ok(()));
        // 1st fold
        state = state.step();
        // 2nd fold
        state = state.step();
        // Flop
        state = state.step();
        // Turn
        state = state.step();
        // River
        state = state.step();
        // ShowHands
        state = state.step();
        // DistributePot
        state = state.step();
        // RemovePlayers
        state = state.step();
        // DivideDonations
        state = state.step();
        // UpdateBlinds
        state = state.step();
        // BootPlayers
        state = state.step();
        // Lobby
        state = state.step();
        assert_eq!(state.init_start("0"), Ok(()));
    }

    #[test]
    fn early_showdown_1_winner_2_late_folds() {
        let mut state = init_state();
        assert_eq!(state.init_start("0"), Ok(()));
        // SeatPlayers
        state = state.step();
        // MoveButton
        state = state.step();
        // CollectBlinds
        state = state.step();
        // Deal
        state = state.step();
        // TakeAction
        state = state.step();
        // Call
        assert_eq!(state.take_action("0", Action::Call(10)), Ok(()));
        // Check
        assert_eq!(state.take_action("1", Action::Call(5)), Ok(()));
        // Check
        assert_eq!(state.take_action("2", Action::Check), Ok(()));
        // Flop
        state = state.step();
        // TakeAction
        state = state.step();
        // Check
        assert_eq!(state.take_action("0", Action::Check), Ok(()));
        // Check
        assert_eq!(state.take_action("1", Action::Check), Ok(()));
        // Check
        assert_eq!(state.take_action("2", Action::Check), Ok(()));
        // Turn
        state = state.step();
        // TakeAction
        state = state.step();
        // Check
        assert_eq!(state.take_action("0", Action::Check), Ok(()));
        // Check
        assert_eq!(state.take_action("1", Action::Check), Ok(()));
        // Check
        assert_eq!(state.take_action("2", Action::Check), Ok(()));
        // River
        state = state.step();
        // TakeAction
        state = state.step();
        // Check
        assert_eq!(state.take_action("0", Action::AllIn), Ok(()));
        // Check
        assert_eq!(state.take_action("1", Action::Fold), Ok(()));
        // Check
        assert_eq!(state.take_action("2", Action::Fold), Ok(()));
        // ShowHands
        state = state.step();
        // DistributePot
        state = state.step();
        // RemovePlayers
        state = state.step();
        // DivideDonations
        state = state.step();
        // UpdateBlinds
        state = state.step();
        // BootPlayers
        state = state.step();
        // Lobby
        state = state.step();
        assert_eq!(state.init_start("0"), Ok(()));
    }
}
