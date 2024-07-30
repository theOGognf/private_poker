use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

use super::entities::{Card, Rank, SubHand, Suit};

/// Get the indices corresponding to the winning hands from an array
/// of hands that were each created from `eval`.
///
/// # Examples
///
/// ```
/// let cards1 = [(4u8, Suit::Club), (11u8, Suit::Spade)];
/// let cards2 = [(4u8, Suit::Club), (12u8, Suit::Spade)];
/// let hand1 = eval(&cards1);
/// let hand2 = eval(&cards2);
/// assert_eq!(argmax(&[hand1, hand2]), vec![1])
/// ```
pub fn argmax(hands: &[Vec<SubHand>]) -> Vec<usize> {
    let mut max = vec![SubHand {
        rank: Rank::HighCard,
        cards: vec![0u8],
    }];
    let mut argmaxes: Vec<usize> = Vec::new();
    for (i, hand) in hands.iter().enumerate() {
        match hand.cmp(&max) {
            Ordering::Equal => argmaxes.push(i),
            Ordering::Greater => {
                argmaxes.clear();
                argmaxes.push(i);
                max.clone_from(hand);
            }
            _ => {}
        }
    }
    argmaxes
}

/// Evaluate any number of cards, returning the best (up to) 5-card hand.
///
/// This function assumes the cards are already sorted in increasing order.
/// Cards are grouped into hand rankings and insorted into a heap.
/// The top subhands are created from the heap and compose a hand.
/// Multiple hands can then be compared, and the winning hand(s)
/// can be retrieved with the `argmax` function.
///
/// # Examples
///
/// ```
/// let cards = [(4u8, Suit::Club), (4u8, Suit::Heart), (11u8, Suit::Spade)];
/// let best_subhand = eval(&cards)[0];
/// assert_eq!(best_subhand.rank, Rank::OnePair)
/// ```
pub fn eval(cards: &[Card]) -> Vec<SubHand> {
    // Mapping of suit to (sorted) cards within that suit.
    // Used for tracking whether there's a flush or straight flush.
    let mut values_per_suit: HashMap<Suit, Vec<u8>> = HashMap::new();

    // Used for tracking whether there's a straight.
    let mut straight_count: u8 = 0;
    let mut straight_prev_value: u8 = 0;

    // Mapping of rank to each subhand for that rank. Helps track
    // the highest subhand in each rank.
    let mut subhands_per_rank: BTreeMap<Rank, BTreeSet<SubHand>> = BTreeMap::new();
    // Count number of times a card value appears. Helps track one pair,
    // two pair, etc.
    let mut value_counts: HashMap<u8, u8> = HashMap::new();

    // Loop through cards in hand assuming the hand is sorted
    // and that each ace appears in the hand twice (at the low
    // end with a value of 1 and at the high end with a value
    // of 14). We push hands into a binary heap so we can
    // easily get the best hand at the end.
    let mut hands: BinaryHeap<SubHand> = BinaryHeap::new();
    for (value, suit) in cards.iter() {
        // Keep a count of cards for each suit. If the suit count
        // reaches a flush, it's also checked for a straight
        // for the straight flush potential.
        let values_in_suit = values_per_suit.entry(*suit).or_default();
        values_in_suit.push(*value);

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
            let maybe_straight_flush_cards = &values_in_suit[maybe_straight_flush_start_idx..];
            let mut is_straight_flush = true;
            for flush_idx in 0..4 {
                if (maybe_straight_flush_cards[flush_idx] + 1)
                    != maybe_straight_flush_cards[flush_idx + 1]
                {
                    is_straight_flush = false;
                    break;
                }
            }

            if is_straight_flush {
                hands.push(SubHand {
                    rank: Rank::StraightFlush,
                    cards: Vec::from(maybe_straight_flush_cards),
                })
            } else {
                hands.push(SubHand {
                    rank: Rank::Flush,
                    cards: Vec::from(maybe_straight_flush_cards),
                })
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
            let straight_subhand = SubHand {
                rank: Rank::Straight,
                cards: (*value - 4..*value).rev().collect(),
            };
            // We don't need to push the straight into the heap if something
            // better was already found.
            let best_subhand = hands.peek();
            match best_subhand {
                None => hands.push(straight_subhand),
                Some(subhand) => {
                    if *subhand < straight_subhand {
                        hands.push(straight_subhand);
                    }
                }
            }
        }

        // Now start checking for hands besides straights and flushes.
        let value_count = value_counts.entry(*value).or_insert(0);
        *value_count += 1;

        match *value_count {
            1 => {
                let high_card_subhand = SubHand {
                    rank: Rank::HighCard,
                    cards: vec![*value],
                };
                subhands_per_rank
                    .entry(Rank::HighCard)
                    .or_default()
                    .insert(high_card_subhand);
            }

            2 => {
                let one_pair_subhand = SubHand {
                    rank: Rank::OnePair,
                    cards: vec![*value; 2],
                };
                let one_pairs = subhands_per_rank.entry(Rank::OnePair).or_default();
                one_pairs.insert(one_pair_subhand);

                // Check if a pair also occurs, then both pairs
                // make a two pair.
                if let Some(next_best_one_pair) = one_pairs.iter().nth_back(1) {
                    let mut two_pair_cards = vec![*value; 2];
                    two_pair_cards.extend(next_best_one_pair.cards.clone());
                    let two_pair_subhand = SubHand {
                        rank: Rank::TwoPair,
                        cards: two_pair_cards,
                    };
                    subhands_per_rank
                        .entry(Rank::TwoPair)
                        .or_default()
                        .insert(two_pair_subhand);
                }

                // Check if a three of a kind also occurs, then the pair
                // and three of a kind make a full house.
                if let Some(three_of_a_kinds) = subhands_per_rank.get(&Rank::ThreeOfAKind) {
                    if let Some(best_three_of_a_kind) = three_of_a_kinds.iter().next() {
                        let mut full_house_cards = best_three_of_a_kind.cards.clone();
                        full_house_cards.extend(vec![*value; 2]);
                        let full_house_subhand = SubHand {
                            rank: Rank::FullHouse,
                            cards: full_house_cards,
                        };

                        subhands_per_rank
                            .entry(Rank::FullHouse)
                            .or_default()
                            .insert(full_house_subhand);
                    }
                }
            }

            3 => {
                let one_pair_subhand = SubHand {
                    rank: Rank::OnePair,
                    cards: vec![*value; 2],
                };
                let three_of_a_kind_subhand = SubHand {
                    rank: Rank::ThreeOfAKind,
                    cards: vec![*value; 3],
                };
                subhands_per_rank
                    .get_mut(&Rank::OnePair)
                    .unwrap()
                    .remove(&one_pair_subhand);
                subhands_per_rank
                    .entry(Rank::ThreeOfAKind)
                    .or_default()
                    .insert(three_of_a_kind_subhand);

                // Check if a pair also occurs, then the three of a kind
                // and the pair make a full house.
                if let Some(one_pairs) = subhands_per_rank.get(&Rank::OnePair) {
                    if let Some(best_one_pair) = one_pairs.iter().next_back() {
                        let mut full_house_cards = vec![*value; 3];
                        full_house_cards.extend(best_one_pair.cards.clone());
                        let full_house_subhand = SubHand {
                            rank: Rank::FullHouse,
                            cards: full_house_cards,
                        };
                        subhands_per_rank
                            .entry(Rank::FullHouse)
                            .or_default()
                            .insert(full_house_subhand);
                    }
                }

                // Check if another three of a kind occurs, then both three
                // of a kinds make a full house.
                if let Some(three_of_a_kinds) = subhands_per_rank.get(&Rank::ThreeOfAKind) {
                    if let Some(next_best_three_of_a_kind) = three_of_a_kinds.iter().nth_back(1) {
                        let mut full_house_cards = vec![*value; 3];
                        full_house_cards.extend(vec![next_best_three_of_a_kind.cards[0]; 2]);
                        let full_house_subhand = SubHand {
                            rank: Rank::FullHouse,
                            cards: full_house_cards,
                        };
                        subhands_per_rank
                            .entry(Rank::FullHouse)
                            .or_default()
                            .insert(full_house_subhand);
                    }
                }
            }

            4 => {
                let three_of_a_kind_subhand = SubHand {
                    rank: Rank::ThreeOfAKind,
                    cards: vec![*value; 3],
                };
                let four_of_a_kind_subhand = SubHand {
                    rank: Rank::FourOfAKind,
                    cards: vec![*value; 4],
                };
                subhands_per_rank
                    .get_mut(&Rank::ThreeOfAKind)
                    .unwrap()
                    .remove(&three_of_a_kind_subhand);
                subhands_per_rank
                    .entry(Rank::FourOfAKind)
                    .or_default()
                    .insert(four_of_a_kind_subhand);

                // You can't get a four of a kind and a straight flush
                // in the same round for any individual player.
                break;
            }

            _ => unreachable!("Cheater!"),
        }
    }

    // Move subhands according to rank to the temporary hands heap.
    // Can only keep the best subhand for each except for high cards.
    // There can be up to 5 high cards in the final hand.
    while let Some((rank, mut subhands)) = subhands_per_rank.pop_last() {
        if let Some(best_subhand) = hands.peek() {
            if best_subhand.rank >= Rank::Straight {
                break;
            }
        }
        if rank == Rank::HighCard {
            while let Some(best_subhand) = subhands.pop_last() {
                hands.push(best_subhand);
            }
        } else if let Some(best_subhand) = subhands.pop_last() {
            hands.push(best_subhand);
        }
    }

    // Now convert the binary heap to a vector containing the best
    // hand. Do this by popping from the binary heap until we get
    // the 5 best cards in our hand to construct the best hand.
    let mut cards_in_hand: HashSet<u8> = HashSet::with_capacity(5);
    let mut num_cards: usize = 0;
    let mut hand: Vec<SubHand> = Vec::with_capacity(5);
    while let Some(subhand) = hands.pop() {
        if hand.is_empty()
            || (subhand.rank == Rank::HighCard && !cards_in_hand.contains(&subhand.cards[0]))
        {
            num_cards += subhand.cards.len();
            cards_in_hand.extend(subhand.cards.clone());
            hand.push(subhand);
        }
        if let Some(best_subhand) = hand.first() {
            if best_subhand.rank >= Rank::Straight || num_cards >= 5 {
                break;
            }
        }
    }
    hand
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
        for (j, suit) in [Suit::Club, Suit::Spade, Suit::Diamond, Suit::Heart]
            .into_iter()
            .enumerate()
        {
            deck[4 * i + j] = (value, suit);
        }
    }
    deck
}

#[cfg(test)]
mod tests {
    use super::{argmax, eval, Card, Rank, Suit};

    struct TestHand {
        expected_rank: Rank,
        cards: Vec<Card>,
    }

    macro_rules! eval_and_argmax_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (test_hand1, test_hand2, expected_winner) = $value;
                let hand1 = eval(&test_hand1.cards);
                let hand2 = eval(&test_hand2.cards);
                assert_eq!(test_hand1.expected_rank, hand1[0].rank);
                assert_eq!(test_hand2.expected_rank, hand2[0].rank);
                assert_eq!(expected_winner, argmax(&[hand1, hand2]));
            }
        )*
        }
    }

    eval_and_argmax_tests! {
        straight_flush_wins_to_flush: (TestHand{expected_rank: Rank::StraightFlush, cards: vec![
            (1u8, Suit::Heart),
            (5u8, Suit::Heart),
            (6u8, Suit::Heart),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
            (9u8, Suit::Heart),
            (14u8, Suit::Heart),
        ]}, TestHand{expected_rank: Rank::Flush, cards: vec![
            (2u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ]}, vec![0]),
        straight_loses_to_straight_flush: (TestHand{expected_rank: Rank::Straight, cards: vec![
            (4u8, Suit::Heart),
            (5u8, Suit::Heart),
            (6u8, Suit::Club),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
        ]}, TestHand{expected_rank: Rank::StraightFlush, cards: vec![
            (3u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ]}, vec![1]),
        straight_wins_to_high_card: (TestHand{expected_rank: Rank::Straight, cards: vec![
            (4u8, Suit::Heart),
            (5u8, Suit::Heart),
            (6u8, Suit::Club),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
        ]}, TestHand{expected_rank: Rank::HighCard, cards: vec![
            (1u8, Suit::Diamond),
            (5u8, Suit::Heart),
            (6u8, Suit::Heart),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
            (10u8, Suit::Diamond),
        ]}, vec![0]),
        flush_loses_to_straight_flush: (TestHand{expected_rank: Rank::Flush, cards: vec![
            (4u8, Suit::Heart),
            (5u8, Suit::Heart),
            (6u8, Suit::Club),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
            (9u8, Suit::Heart),
        ]}, TestHand{expected_rank: Rank::StraightFlush, cards: vec![
            (3u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
            (8u8, Suit::Diamond),
        ]}, vec![1]),
        flush_loses_to_flush: (TestHand{expected_rank: Rank::Flush, cards: vec![
            (2u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
            (8u8, Suit::Diamond),
        ]}, TestHand{expected_rank: Rank::Flush, cards: vec![
            (3u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
            (8u8, Suit::Diamond),
        ]}, vec![1]),
        high_card_loses_to_high_card: (TestHand{expected_rank: Rank::HighCard, cards: vec![
            (3u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ]}, TestHand{expected_rank: Rank::HighCard, cards: vec![
            (4u8, Suit::Club),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (10u8, Suit::Heart),
            (12u8, Suit::Spade),
        ]}, vec![1]),
        high_card_wins_to_high_card: (TestHand{expected_rank: Rank::HighCard, cards: vec![
            (4u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ]}, TestHand{expected_rank: Rank::HighCard, cards: vec![
            (3u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ]}, vec![0]),
        high_card_ties_with_high_card: (TestHand{expected_rank: Rank::HighCard, cards: vec![
            (4u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ]}, TestHand{expected_rank: Rank::HighCard, cards: vec![
            (4u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ]}, vec![0, 1]),
        full_house_loses_to_full_house: (TestHand{expected_rank: Rank::FullHouse, cards: vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (4u8, Suit::Diamond),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (6u8, Suit::Club),
            (8u8, Suit::Diamond),
            (12u8, Suit::Spade),
        ]}, TestHand{expected_rank: Rank::FullHouse, cards: vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (4u8, Suit::Diamond),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (11u8, Suit::Spade),
        ]}, vec![0]),
        two_pair_wins_to_two_pair: (TestHand{expected_rank: Rank::TwoPair, cards: vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Club),
            (12u8, Suit::Spade),
        ]}, TestHand{expected_rank: Rank::TwoPair, cards: vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (11u8, Suit::Spade),
        ]}, vec![0]),
        one_pair_wins_to_one_pair: (TestHand{expected_rank: Rank::OnePair, cards: vec![
            (4u8, Suit::Club),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Club),
            (12u8, Suit::Spade),
        ]}, TestHand{expected_rank: Rank::OnePair, cards: vec![
            (3u8, Suit::Club),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Heart),
            (12u8, Suit::Diamond),
        ]}, vec![0]),
        four_of_a_kind_wins_to_two_pair: (TestHand{expected_rank: Rank::FourOfAKind, cards: vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (4u8, Suit::Diamond),
            (4u8, Suit::Spade),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Club),
            (12u8, Suit::Spade),
        ]}, TestHand{expected_rank: Rank::TwoPair, cards: vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (11u8, Suit::Spade),
        ]}, vec![0]),
        high_card_loses_to_one_pair: (TestHand{expected_rank: Rank::HighCard, cards: vec![
            (4u8, Suit::Club),
            (12u8, Suit::Spade),
        ]}, TestHand{expected_rank: Rank::OnePair, cards: vec![
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (11u8, Suit::Spade),
        ]}, vec![1]),
        three_of_a_kind_loses_to_three_of_a_kind: (TestHand{expected_rank: Rank::ThreeOfAKind, cards: vec![
            (6u8, Suit::Heart),
            (14u8, Suit::Spade),
            (14u8, Suit::Diamond),
            (14u8, Suit::Heart),
        ]}, TestHand{expected_rank: Rank::ThreeOfAKind, cards: vec![
            (7u8, Suit::Heart),
            (14u8, Suit::Spade),
            (14u8, Suit::Diamond),
            (14u8, Suit::Heart),
        ]}, vec![1]),
    }
}
