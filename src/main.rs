// FIXME: Temporary warning suppression during early development.
#![allow(dead_code)]

mod poker;

fn main() {
    let mut game = poker::Poker::new();
    game.state = game.state.step();
}
