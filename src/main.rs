mod poker;
use rand::seq::SliceRandom;
use rand::thread_rng;

fn main() {
    let mut deck = poker::new_deck();
    deck.shuffle(&mut thread_rng());
    print!("{deck:?}");
}
// spectators: Vec<poker::Player>;
// pots: Vec<Vec<(poker::Player, poker::PlayerState)>>;
