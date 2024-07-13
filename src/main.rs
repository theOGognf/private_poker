mod game;
mod poker;
use ::std::time;

fn main() {
    let start = time::Instant::now();
    let cards1 = [
        (1u8, poker::Suit::Heart),
        (5u8, poker::Suit::Heart),
        (6u8, poker::Suit::Heart),
        (7u8, poker::Suit::Heart),
        (8u8, poker::Suit::Heart),
        (14u8, poker::Suit::Heart),
    ];
    let cards2 = [
        (2u8, poker::Suit::Diamond),
        (4u8, poker::Suit::Diamond),
        (5u8, poker::Suit::Diamond),
        (6u8, poker::Suit::Diamond),
        (7u8, poker::Suit::Diamond),
    ];
    let hand1 = poker::eval(&cards1);
    let hand2 = poker::eval(&cards2);
    let winners = poker::argmax(&[hand1, hand2]);
    let duration = start.elapsed().as_nanos();
    println!("{duration}");
}
