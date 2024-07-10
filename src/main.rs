mod poker;

fn main() {
    let deck = poker::new_deck();
    print!("{deck:?}");
}
