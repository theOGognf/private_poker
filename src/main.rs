mod poker;
use rand::thread_rng;
use rand::seq::SliceRandom;

fn main() {
    let mut deck = poker::new_deck();
    deck.shuffle(&mut thread_rng());
    print!("{deck:?}");
}
