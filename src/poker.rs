use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet};

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

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum Suit {
    Club,
    Spade,
    Diamond,
    Heart,
}

type Card = (u8, Suit);

type Hand = (Rank, u8);

pub fn sort(cards: &[Card]) -> BinaryHeap<Hand> {
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
        values_per_suit.entry(*suit).or_insert(vec![]).push(*value);
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
            if max_hand.is_none() {
                hands.push(straight);
            } else {
                if *max_hand.unwrap() < straight {
                    hands.push(straight);
                }
            }
        }

        // Now start checking for hands besides straights and flushes.
        let value_count = value_counts.entry(*value).or_insert(0);
        *value_count += 1;

        // Don't care about high cards unless they're the last one
        // in the hand and there're no better hands.
        if card_idx == (cards.len() - 1) {
            if hands.len() == 0 && rank_to_values.len() == 0 && *value_count == 1u8 {
                rank_to_values
                    .entry(Rank::HighCard)
                    .or_insert(BTreeSet::new())
                    .insert(*value);
            }
        }

        match *value_count {
            // Don't want to do anything since it's covered by the previous
            // statement.
            1 => {}

            2 => {
                let rank_values = rank_to_values
                    .entry(Rank::OnePair)
                    .or_insert(BTreeSet::new());
                rank_values.insert(*value);

                // Check if a pair also occurs, then both pairs
                // make a two pair.
                if rank_values.len() >= 2 {
                    rank_to_values
                        .entry(Rank::TwoPair)
                        .or_insert(BTreeSet::new())
                        .insert(*value);
                }

                // Check if a three of a kind also occurs, then the pair
                // and three of a kind make a full house.
                if rank_to_values.contains_key(&Rank::ThreeOfAKind) {
                    let three_of_a_kinds = rank_to_values.get(&Rank::ThreeOfAKind).unwrap();
                    if three_of_a_kinds.len() == 1 {
                        let three_of_a_kind_value = three_of_a_kinds.iter().next().unwrap().clone();

                        rank_to_values
                            .entry(Rank::FullHouse)
                            .or_insert(BTreeSet::new())
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
                    .or_insert(BTreeSet::new())
                    .insert(*value);

                // Check if a pair also occurs, then the three of a kind
                // and the pair make a full house.
                if rank_to_values.contains_key(&Rank::OnePair)
                    && rank_to_values.get(&Rank::OnePair).unwrap().len() >= 1
                {
                    rank_to_values
                        .entry(Rank::FullHouse)
                        .or_insert(BTreeSet::new())
                        .insert(*value);
                }

                // Check if another three of a kind occurs, then both three
                // of a kinds make a full house.
                if rank_to_values.get(&Rank::ThreeOfAKind).unwrap().len() == 2 {
                    rank_to_values
                        .entry(Rank::FullHouse)
                        .or_insert(BTreeSet::new())
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
                    .or_insert(BTreeSet::new())
                    .insert(*value);

                // You can't get a four of a kind and a straight flush
                // in the same round for any individual player.
                break;
            }

            _ => unreachable!("Cheater!"),
        }

        // Only need the max hand from the sets for comparison since we
        // only care about the highest ranking hand.
        if rank_to_values.len() >= 1 {
            let (rank, set) = rank_to_values.iter().max().unwrap();
            hands.push((*rank, *set.iter().next_back().unwrap()));
        }
    }
    return hands;
}

///
///
///
///
pub fn argmax(hands: &[BinaryHeap<Hand>]) -> Vec<usize> {
    let mut max_hand: Hand = (Rank::HighCard, 0u8);
    let mut max_hand_indices: Vec<usize> = Vec::new();
    for (i, hand) in hands.iter().enumerate() {
        let high_hand = *hand.iter().next().unwrap();
        if high_hand > max_hand {
            max_hand_indices.clear();
            max_hand_indices.push(i);
            max_hand = high_hand;
        } else if high_hand == max_hand {
            max_hand_indices.push(i)
        }
    }
    return max_hand_indices;
}
