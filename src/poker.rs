use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet};

#[derive(Clone, Copy, Eq, Hash, Ord, PartialOrd, PartialEq)]
enum Rank {
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
enum Suit {
    Club,
    Spade,
    Diamond,
    Heart,
}

type Hand = (Rank, u8);

// TODO: Need to make sure aces aren't counted twice for flushes.
fn sort(cards: &[(u8, Suit)]) -> BinaryHeap<Hand> {
    // Mapping of suit to (sorted) cards within that suit.
    // Used for tracking whether there's a flush or straight flush.
    let mut values_per_suit: HashMap<Suit, Vec<u8>> = HashMap::new();

    // Used for tracking whether there's a straight.
    let mut straight_maxes: HashSet<u8> = HashSet::new();
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
    // end and at the high end). We insort hands into a binary heap
    // so we can easily get the best hand at the end.
    let mut hands: BinaryHeap<Hand> = BinaryHeap::new();
    for (i, (value, suit)) in cards.iter().enumerate() {
        *value_counts.entry(*value).or_insert(0) += 1;

        let count = *value_counts.get(value).unwrap();

        // Don't care about high cards unless they're the last one
        // in the hand and there're no better hands.
        if i == (cards.len() - 1) {
            if count == 1u8 && rank_to_values.len() == 0 {}
        }

        match count {
            2 => {
                rank_to_values
                    .entry(Rank::OnePair)
                    .or_insert(BTreeSet::new())
                    .insert(*value);

                // Check if a pair also occurs, then both pairs
                // make a two pair.
                if rank_to_values.get(&Rank::OnePair).unwrap().len() >= 2 {
                    rank_to_values
                        .entry(Rank::TwoPair)
                        .or_insert(BTreeSet::new())
                        .insert(*value);
                }

                // Check if a three of a kind also occurs, then the pair
                // and three of a kind make a full house.
                if rank_to_values.contains_key(&Rank::ThreeOfAKind)
                    && rank_to_values.get(&Rank::ThreeOfAKind).unwrap().len() == 1
                {
                    let three_of_a_kind_value = rank_to_values
                        .get(&Rank::ThreeOfAKind)
                        .unwrap()
                        .iter()
                        .next()
                        .unwrap()
                        .clone();
                    rank_to_values
                        .entry(Rank::FullHouse)
                        .or_insert(BTreeSet::new())
                        .insert(three_of_a_kind_value);
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

                // If a four of a kind appears, nothing else better can appear.
                // It's okay to break early.
                break;
            }

            _ => unreachable!("Only four suits per value."),
        }

        // Keep a count of cards that're in sequential order, tracking the
        // max and min cards for a (potential) straight. These high/low cards
        // are compared against flush cards later. If there's a flush high/low
        // that overlaps with the straight high/low, then we know there's
        // a straight flush.
        if *value == (straight_prev_value + 1) {
            straight_count += 1;
        } else if *value == straight_prev_value {
        } else {
            straight_count = 1;
        }

        straight_prev_value = *value;
        if straight_count >= 5 {
            straight_maxes.insert(*value);
        }

        // Keep a count of cards for each suit. If the suit count
        // reaches a flush, it's also compared against straights
        // for the straight flush potential. If there's no overlap with
        // a straight, then we know we at least have a flush.
        values_per_suit.entry(*suit).or_insert(vec![]).push(*value);
        let values_with_suit = values_per_suit.get(suit).unwrap();
        if values_with_suit.len() >= 5 {
            let maybe_flush_start_idx = values_with_suit.len() - 6;
            let maybe_flush_slice = &values_with_suit[maybe_flush_start_idx..];
            let mut is_straight_flush = true;
            for flush_idx in 0..3 {
                if maybe_flush_slice[flush_idx] != (maybe_flush_slice[flush_idx] + 1) {
                    is_straight_flush = false;
                    break;
                }
            }

            if is_straight_flush {
                let straight_max_value = maybe_flush_slice.last().unwrap();
                straight_maxes.remove(&straight_max_value);
                hands.push((Rank::StraightFlush, *value))
            } else {
                hands.push((Rank::Flush, *value))
            }
        }

        for straight in straight_maxes.iter() {
            hands.push((Rank::Straight, *straight))
        }

        // Only need the max hand from the sets for comparison since we
        // only care about the highest ranking hand.
        let (rank, set) = rank_to_values.iter().max().unwrap();
        hands.push((*rank, *set.iter().next_back().unwrap()));
    }
    return hands;
}

fn argmax() {}
