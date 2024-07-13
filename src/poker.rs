use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Rank {
    HighCard,
    // Only keep high cards.
    OnePair,
    // Only keep high cards.
    TwoPair,
    // Only keep high cards.
    ThreeOfAKind,
    // Don't keep anything else.
    Straight,
    // Don't keep anything else.
    Flush,
    // Don't keep anything else.
    FullHouse,
    // Don't keep anything else.
    FourOfAKind,
    // Don't keep anything else.
    StraightFlush,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Suit {
    Club,
    Spade,
    Diamond,
    Heart,
    // Wild is used to initialize a deck of cards.
    // Might be a good choice for a joker's suit.
    Wild,
}

/// A card is a tuple of a uInt8 value (ace=1u8 ... ace=14u8)
/// and a suit. A joker is depicted as 0u8.
pub type Card = (u8, Suit);

#[derive(Clone, Ord, Eq, PartialEq, PartialOrd)]
pub struct SubHand {
    rank: Rank,
    cards: Vec<u8>,
}

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
pub fn eval(cards: &[Card]) -> Vec<SubHand> {
    // Mapping of suit to (sorted) cards within that suit.
    // Used for tracking whether there's a flush or straight flush.
    let mut values_per_suit: HashMap<Suit, Vec<u8>> = HashMap::new();

    // Used for tracking whether there's a straight.
    let mut straight_count: u8 = 0;
    let mut straight_prev_value: u8 = 0;

    // Mapping of rank to each value that meets that rank. Helps track
    // the highest value in each rank.
    let mut subhands_per_rank: BTreeMap<Rank, BTreeSet<SubHand>> = BTreeMap::new();
    // Count number of times a value appears. Helps track one pair,
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
            let maybe_straight_flush_cards = &values_in_suit[maybe_straight_flush_start_idx..];
            let mut is_straight_flush = true;
            for flush_idx in 0..3 {
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
            let max_hand = hands.peek();
            if max_hand.is_none() || *max_hand.unwrap() < straight_subhand {
                hands.push(straight_subhand);
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
                let subhand = subhands_per_rank.entry(Rank::OnePair).or_default();
                subhand.insert(one_pair_subhand);

                // Check if a pair also occurs, then both pairs
                // make a two pair.
                if subhand.len() >= 2 {
                    let one_pairs = subhands_per_rank.get(&Rank::OnePair).unwrap();
                    let mut two_pair_cards = vec![*value; 2];
                    two_pair_cards.extend(one_pairs.iter().nth_back(1).unwrap().cards.clone());
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
                let three_of_a_kinds = subhands_per_rank.get(&Rank::ThreeOfAKind);
                if three_of_a_kinds.is_some_and(|s| s.len() == 1) {
                    let mut full_house_cards = three_of_a_kinds
                        .unwrap()
                        .iter()
                        .next()
                        .unwrap()
                        .cards
                        .clone();
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
                let one_pairs = subhands_per_rank.get(&Rank::OnePair);
                if one_pairs.is_some_and(|s| !s.is_empty()) {
                    let mut full_house_cards = vec![*value; 3];
                    full_house_cards
                        .extend(one_pairs.unwrap().iter().next_back().unwrap().cards.clone());
                    let full_house_subhand = SubHand {
                        rank: Rank::FullHouse,
                        cards: full_house_cards,
                    };
                    subhands_per_rank
                        .entry(Rank::FullHouse)
                        .or_default()
                        .insert(full_house_subhand);
                }

                // Check if another three of a kind occurs, then both three
                // of a kinds make a full house.
                let three_of_a_kinds = subhands_per_rank.get(&Rank::ThreeOfAKind);
                if three_of_a_kinds.is_some_and(|s| s.len() == 2) {
                    let other_three_of_a_kind_cards = three_of_a_kinds
                        .unwrap()
                        .iter()
                        .nth_back(1)
                        .unwrap()
                        .cards
                        .clone();
                    let mut full_house_cards = vec![*value; 3];
                    full_house_cards.extend(vec![other_three_of_a_kind_cards[0]; 2]);
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
    while hands.is_empty()
        || (hands.peek().unwrap().rank < Rank::Straight && !subhands_per_rank.is_empty())
    {
        let (rank, subhands) = &mut subhands_per_rank.pop_last().unwrap();
        if *rank == Rank::HighCard {
            while !subhands.is_empty() {
                let best_subhand = subhands.pop_last().unwrap();
                hands.push(best_subhand);
            }
        } else if !subhands.is_empty() {
            let best_subhand = subhands.pop_last().unwrap();
            hands.push(best_subhand);
        }
    }

    // Now convert the binary heap to a vector containing the best
    // hand. Do this by popping from the binary heap until we get
    // the 5 best cards in our hand to construct the best hand.
    let mut cards_in_hand: HashSet<u8> = HashSet::with_capacity(5);
    let mut num_cards: usize = 0;
    let mut hand: Vec<SubHand> = Vec::with_capacity(5);

    // Manually do the first iteration to fill the max subhand.
    let subhand = hands.pop().unwrap();
    num_cards += subhand.cards.len();
    cards_in_hand.extend(subhand.cards.clone());
    hand.push(subhand);
    while !hands.is_empty() && hand[0].rank < Rank::Straight && num_cards < 5 {
        let subhand = hands.pop().unwrap();
        if subhand.rank == Rank::HighCard && !cards_in_hand.contains(&subhand.cards[0]) {
            cards_in_hand.insert(subhand.cards[0]);
            num_cards += 1;
            hand.push(subhand);
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
    use super::{argmax, eval, Rank, Suit};

    macro_rules! eval_and_argmax_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (expected_subhand1, cards1, expected_subhand2, cards2, expected_winner) = $value;
                let hand1 = eval(&cards1);
                let hand2 = eval(&cards2);
                assert_eq!(expected_subhand1, hand1[0].rank);
                assert_eq!(expected_subhand2, hand2[0].rank);
                assert_eq!(expected_winner, argmax(&[hand1, hand2]));
            }
        )*
        }
    }

    eval_and_argmax_tests! {
        straightflush_wins_to_flush: (Rank::StraightFlush, [
            (1u8, Suit::Heart),
            (5u8, Suit::Heart),
            (6u8, Suit::Heart),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
            (14u8, Suit::Heart),
        ], Rank::Flush, [
            (2u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ], vec![0]),
        straight_loses_to_straight_flush: (Rank::Straight, [
            (4u8, Suit::Heart),
            (5u8, Suit::Heart),
            (6u8, Suit::Club),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
        ], Rank::StraightFlush, [
            (3u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ], vec![1]),
        flush_loses_to_straight_flush: (Rank::Flush, [
            (4u8, Suit::Heart),
            (5u8, Suit::Heart),
            (6u8, Suit::Club),
            (7u8, Suit::Heart),
            (8u8, Suit::Heart),
            (9u8, Suit::Heart),
        ], Rank::StraightFlush, [
            (3u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
            (8u8, Suit::Diamond),
        ], vec![1]),
        flush_loses_to_flush: (Rank::Flush, [
            (1u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ], Rank::Flush, [
            (2u8, Suit::Diamond),
            (4u8, Suit::Diamond),
            (5u8, Suit::Diamond),
            (6u8, Suit::Diamond),
            (7u8, Suit::Diamond),
        ], vec![1]),
        high_card_loses_to_high_card: (Rank::HighCard, [
            (3u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ], Rank::HighCard, [
            (4u8, Suit::Club),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (10u8, Suit::Heart),
            (12u8, Suit::Spade),
        ], vec![1]),
        high_card_wins_to_high_card: (Rank::HighCard, [
            (4u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ], Rank::HighCard, [
            (3u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ], vec![0]),
        high_card_ties_with_high_card: (Rank::HighCard, [
            (4u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ], Rank::HighCard, [
            (4u8, Suit::Club),
            (5u8, Suit::Heart),
            (7u8, Suit::Diamond),
            (9u8, Suit::Heart),
            (11u8, Suit::Spade),
        ], vec![0, 1]),
        full_house_loses_to_full_house: (Rank::FullHouse, [
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (4u8, Suit::Diamond),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (6u8, Suit::Club),
            (8u8, Suit::Diamond),
            (12u8, Suit::Spade),
        ], Rank::FullHouse, [
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (4u8, Suit::Diamond),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (11u8, Suit::Spade),
        ], vec![0]),
        two_pair_wins_to_two_pair: (Rank::TwoPair, [
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Club),
            (12u8, Suit::Spade),
        ], Rank::TwoPair, [
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (11u8, Suit::Spade),
        ], vec![0]),
        one_pair_wins_to_one_pair: (Rank::OnePair, [
            (4u8, Suit::Club),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Club),
            (12u8, Suit::Spade),
        ], Rank::OnePair, [
            (3u8, Suit::Club),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Heart),
            (12u8, Suit::Diamond),
        ], vec![0]),
        four_of_a_kind_wins_to_two_pair: (Rank::FourOfAKind, [
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (4u8, Suit::Diamond),
            (4u8, Suit::Spade),
            (6u8, Suit::Heart),
            (8u8, Suit::Diamond),
            (12u8, Suit::Club),
            (12u8, Suit::Spade),
        ], Rank::TwoPair, [
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (6u8, Suit::Heart),
            (6u8, Suit::Diamond),
            (11u8, Suit::Spade),
        ], vec![0]),
        high_card_loses_to_one_pair: (Rank::HighCard, [
            (4u8, Suit::Club),
            (12u8, Suit::Spade),
        ], Rank::OnePair, [
            (4u8, Suit::Club),
            (4u8, Suit::Heart),
            (11u8, Suit::Spade),
        ], vec![1]),
    }
}
