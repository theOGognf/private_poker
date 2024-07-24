pub mod functional;
pub mod game;

use game::{
    BootPlayers, CollectBlinds, Deal, DivideDonations, Flop, Game, MoveButton, RemovePlayers,
    River, SeatPlayers, Showdown, TakeAction, Turn, UpdateBlinds,
};

/// The poker finite state machine. State transitions are defined in
/// `PokerState::step`.
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

pub struct Poker {
    pub state: PokerState,
}

impl Poker {
    pub fn new() -> Self {
        let game = Game::<SeatPlayers>::new();
        let state = PokerState::SeatPlayers(game);
        Poker { state }
    }
}
