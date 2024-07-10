mod poker;
use std::time;

fn main() {
    let h1 = [
        (4u8, poker::Suit::Heart),
        (5u8, poker::Suit::Heart),
        (6u8, poker::Suit::Club),
        (7u8, poker::Suit::Heart),
        (8u8, poker::Suit::Heart),
    ];
    let h2 = [
        (3u8, poker::Suit::Diamond),
        (4u8, poker::Suit::Diamond),
        (5u8, poker::Suit::Diamond),
        (6u8, poker::Suit::Diamond),
        (7u8, poker::Suit::Diamond),
    ];

    let timer = time::Instant::now();
    let hands1 = poker::sort(&h1);
    let elapsed = timer.elapsed().as_micros();
    println!("{elapsed} us");
    println!("{hands1:?}");
    let hands2 = poker::sort(&h2);
    println!("{hands2:?}");
    let indices = poker::argmax(&[hands1, hands2]);
    println!("{indices:?}");

    assert_eq!(indices, vec![1]);
}
