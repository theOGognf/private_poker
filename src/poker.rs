use std::{
    cmp::Ordering,
    collections::{BTreeSet, BinaryHeap, HashMap},
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Rank {
    HighCard,
    OnePair,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Suit {
    Club,
    Spade,
    Diamond,
    Heart,
    // Wild is used to initialize a deck of cards.
    // It is never used in practice, but it would be
    // a good choice for depicting a joker card.
    Wild,
}

/// A card is a tuple of a uInt8 value (ace=1u8 ... ace=14u8)
/// and a suit. A joker is depicted as 0u8.
type Card = (u8, Suit);

type Hand = (Rank, u8);

/// Get the indices corresponding to the winning hands from an array
/// of hands that were each created from `eval`.
/// 
/// # Examples
/// 
/// ```
/// let cards1 = [(4u8, Suit::Club), (11u8, Suit::Spade)];
/// let cards2 = [(4u8, Suit::Club), (12u8, Suit::Spade)];
/// let value1 = eval(&cards1);
/// let value2 = eval(&cards2);
/// assert_eq!(argmax(&[value1, value2]), vec![1])
/// ```
pub fn argmax(hands: &Vec<Hand>) -> Vec<usize> {
    let mut max: Hand = (Rank::HighCard, 0u8);
    let mut argmaxes: Vec<usize> = Vec::new();
    for (i, hand) in hands.iter().enumerate() {
        match hand.cmp(&max) {
            Ordering::Equal => argmaxes.push(i),
            Ordering::Greater => {
                argmaxes.clear();
                argmaxes.push(i);
                max = *hand;
            }
            _ => {}
        }
    }
    argmaxes
}

/// This function assumes the cards are already sorted in increasing order.
/// Group cards into hand rankings and insort them into a heap.
/// The max value in the heap is the best hand and is returned.
/// Multiple hands can then be compared, and the winning hand(s)
/// can be retrieved with the `argmax` function.
/// 
/// # Examples
/// 
/// ```
/// let cards = [(4u8, Suit::Club), (4u8, Suit::Heart), (11u8, Suit::Spade)];
/// let best_hand = eval(&cards).peek().unwrap();
/// assert_eq!(best_hand, (Rank::OnePair, 4u8))
/// ```
pub fn eval(cards: &Vec<Card>) -> Hand {
    // Mapping of suit to (sorted) cards within that suit.
    // Used for tracking whether there's a flush or straight flush.
    let mut values_per_suit: HashMap<Suit, Vec<u8>> = HashMap::new();

    // Used for tracking whether there's a straight.
    let mut straight_count: u8 = 0;
    let mut straight_prev_value: u8 = 0;

    // Mapping of rank to each value that meets that rank. Helps track
    // the highest value in each rank.
    let mut rank_to_values: HashMap<Rank, BTreeSet<u8>> = HashMap::new();
    // Count number of times a value appears. Helps track one pair,
    // two pair, etc.
    let mut value_counts: HashMap<u8, u8> = HashMap::new();

    // Loop through cards in hand assuming the hand is sorted
    // and that each ace appears in the hand twice (at the low
    // end with a value of 1 and at the high end with a value
    // of 14). We push hands into a binary heap so we can
    // easily get the best hand at the end.
    let mut hands: BinaryHeap<Hand> = BinaryHeap::new();
    for (card_idx, (value, suit)) in cards.iter().enumerate() {
        // Keep a count of cards for each suit. If the suit count
        // reaches a flush, it's also checked for a straight
        // for the straight flush potential.
        values_per_suit.entry(*suit).or_default().push(*value);
        let values_in_suit = values_per_suit.get(suit).unwrap();

        // Since aces appear in the cards twice, we need to make sure
        // they aren't counted twice for the flush. To get around this,
        // we just subtract one from the flush count in the case of the
        // high valued ace.
        let mut flush_count = values_in_suit.len();
        if *value == 14u8 {
            flush_count -= 1;
        }

        // A flush was found.
        if flush_count >= 5 {
            let maybe_straight_flush_start_idx = values_in_suit.len() - 5;
            let maybe_straight_flush_slice = &values_in_suit[maybe_straight_flush_start_idx..];
            let mut is_straight_flush = true;
            for flush_idx in 0..3 {
                if (maybe_straight_flush_slice[flush_idx] + 1)
                    != maybe_straight_flush_slice[flush_idx + 1]
                {
                    is_straight_flush = false;
                    break;
                }
            }

            if is_straight_flush {
                hands.push((Rank::StraightFlush, *value))
            } else {
                hands.push((Rank::Flush, *value))
            }
        }

        // Keep a count of cards that're in sequential order to check for
        // a straight. If the same value appears again, we can keep the
        // straight count the same and don't have to reset.
        if (straight_prev_value + 1) == *value {
            straight_count += 1;
        } else if straight_prev_value == *value {
        } else {
            straight_count = 1;
        }

        // A straight was found.
        straight_prev_value = *value;
        if straight_count >= 5 {
            let straight = (Rank::Straight, *value);
            // We don't need to push the straight into the heap if something
            // better was already found.
            let max_hand = hands.peek();
            if max_hand.is_none() || *max_hand.unwrap() < straight {
                hands.push(straight);
            }
        }

        // Now start checking for hands besides straights and flushes.
        let value_count = value_counts.entry(*value).or_insert(0);
        *value_count += 1;

        // Don't care about high cards unless they're the last one
        // in the hand and there're no better hands.
        if card_idx == (cards.len() - 1)
            && hands.is_empty()
            && rank_to_values.is_empty()
            && *value_count == 1u8
        {
            rank_to_values
                .entry(Rank::HighCard)
                .or_default()
                .insert(*value);
        }

        match *value_count {
            // Don't want to do anything since it's covered by the previous
            // statement.
            1 => {}

            2 => {
                let rank_values = rank_to_values.entry(Rank::OnePair).or_default();
                rank_values.insert(*value);

                // Check if a pair also occurs, then both pairs
                // make a two pair.
                if rank_values.len() >= 2 {
                    rank_to_values
                        .entry(Rank::TwoPair)
                        .or_default()
                        .insert(*value);
                }

                // Check if a three of a kind also occurs, then the pair
                // and three of a kind make a full house.
                if rank_to_values.contains_key(&Rank::ThreeOfAKind) {
                    let three_of_a_kinds = rank_to_values.get(&Rank::ThreeOfAKind).unwrap();
                    if three_of_a_kinds.len() == 1 {
                        let three_of_a_kind_value = *three_of_a_kinds.iter().next().unwrap();

                        rank_to_values
                            .entry(Rank::FullHouse)
                            .or_default()
                            .insert(three_of_a_kind_value);
                    }
                }
            }

            3 => {
                rank_to_values
                    .get_mut(&Rank::OnePair)
                    .unwrap()
                    .remove(value);
                rank_to_values
                    .entry(Rank::ThreeOfAKind)
                    .or_default()
                    .insert(*value);

                // Check if a pair also occurs, then the three of a kind
                // and the pair make a full house.
                if rank_to_values.contains_key(&Rank::OnePair)
                    && !rank_to_values.get(&Rank::OnePair).unwrap().is_empty()
                {
                    rank_to_values
                        .entry(Rank::FullHouse)
                        .or_default()
                        .insert(*value);
                }

                // Check if another three of a kind occurs, then both three
                // of a kinds make a full house.
                if rank_to_values.get(&Rank::ThreeOfAKind).unwrap().len() == 2 {
                    rank_to_values
                        .entry(Rank::FullHouse)
                        .or_default()
                        .insert(*value);
                }
            }

            4 => {
                rank_to_values
                    .get_mut(&Rank::ThreeOfAKind)
                    .unwrap()
                    .remove(value);
                rank_to_values
                    .entry(Rank::FourOfAKind)
                    .or_default()
                    .insert(*value);

                // You can't get a four of a kind and a straight flush
                // in the same round for any individual player.
                break;
            }

            _ => unreachable!("Cheater!"),
        }
    }
    // Only need the max hand from the sets for comparison since we
    // only care about the highest ranking hand.
    if !rank_to_values.is_empty() {
        let (rank, set) = rank_to_values.iter().max().unwrap();
        hands.push((*rank, *set.iter().next_back().unwrap()));
    }
    *hands.peek().unwrap()
}

/// Create a new, unshuffled deck of cards.
/// Shuffle the deck using `rand::shuffle`.
/// 
/// # Examples
/// 
/// ```
/// use rand::thread_rng;
/// use rand::seq::SliceRandom;
/// 
/// let mut deck = new_deck();
/// deck.shuffle(&mut thread_rng());
/// ```
pub fn new_deck() -> [Card; 52] {
    let mut deck: [Card; 52] = [(0u8, Suit::Wild); 52];
    for (i, value) in (1u8..14u8).enumerate() {
        for (j, suit) in [Suit::Club, Suit::Spade, Suit::Diamond, Suit::Heart].into_iter().enumerate() {
            deck[4 * i + j] = (value, suit);
        }
    }
    deck
}

#[cfg(test)]
mod tests {
    use super::{argmax, eval, Rank, Suit};

    macro_rules! sort_and_argmax_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (expected_value1, hand1, expected_value2, hand2, expected_winner) = $value;
                let value1 = eval(&hand1);
                let value2 = eval(&hand2);
                assert_eq!(expected_value1, value1.0);
                assert_eq!(expected_value2, value2.0);
                assert_eq!(expected_winner, argmax(&vec![value1, value2]));
            }
        )*
        }
    }

    sort_and_argmax_tests! {
        straight_loses_to_straight_flush: (Rank::Straight, vec![
            (4u8, Suit::Heart),
            (5u8, Suit::Heart),
            (6u8, Suit::Club),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
        ], Rank::StraightFlush, vec![
            (3u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ], vec![1]),
        flush_loses_to_straight_flush: (Rank::Flush, vec![
            (4u8, Suit::Heart),
            (5u8, Suit::Heart),
            (6u8, Suit::Club),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
            (9u8, Suit::Heart),
        ], Rank::StraightFlush, vec![
            (3u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
            (8u8, Suit::Diamond),
        ], vec![1]),
        high_card_wins_to_high_card: (Rank::HighCard, vec![
            (4u8, Suit::Club),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (10u8, Suit::Heart),
            (12u8, Suit::Spade),
        ], Rank::HighCard, vec![
            (3u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ], vec![0]),
        full_house_loses_to_full_house: (Rank::FullHouse, vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (4u8, Suit::Diamond),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (6u8, Suit::Club),
            (8u8, Suit::Diamond),
            (12u8, Suit::Spade),
        ], Rank::FullHouse, vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (4u8, Suit::Diamond),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (11u8, Suit::Spade),
        ], vec![0]),
        two_pair_beats_two_pair: (Rank::TwoPair, vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Club),
            (12u8, Suit::Spade),
        ], Rank::TwoPair, vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (11u8, Suit::Spade),
        ], vec![0]),
        four_of_a_kind_wins_to_two_pair: (Rank::FourOfAKind, vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (4u8, Suit::Diamond),
            (4u8, Suit::Spade),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Club),
            (12u8, Suit::Spade),
        ], Rank::TwoPair, vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (11u8, Suit::Spade),
        ], vec![0]),
        high_card_loses_to_one_pair: (Rank::HighCard, vec![
            (4u8, Suit::Club),
            (12u8, Suit::Spade),
        ], Rank::OnePair, vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (11u8, Suit::Spade),
        ], vec![1]),
    }
}
