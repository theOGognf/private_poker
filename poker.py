import bisect
from collections import Counter, defaultdict
from enum import Enum, IntEnum, auto
import time


class Rank(IntEnum):
    HIGH_CARD = auto()
    ONE_PAIR = auto()
    TWO_PAIR = auto()
    THREE_OF_A_KIND = auto()
    STRAIGHT = auto()
    FLUSH = auto()
    FULL_HOUSE = auto()
    FOUR_OF_A_KIND = auto()
    STRAIGHT_FLUSH = auto()


class Suit(Enum):
    CLUB = auto()
    SPADE = auto()
    DIAMOND = auto()
    HEART = auto()

def timeit(f):
    def inner(*args, **kwargs):
        start = time.monotonic()
        out = f(*args, **kwargs)
        print(time.monotonic() - start)
        return out
    return inner

@timeit
def sort(*cards: tuple[int, Suit]) -> list[tuple[Rank, int]]:
    # Used for tracking whether there's a flush.
    suit_counts = defaultdict(list)

    # Used for tracking whether there's a straight.
    straights = set()
    high = -1
    low = -1
    straight_count = 0
    prev_value_for_straight = -1

    # Used for tracking all other hands.
    sets = defaultdict(set)
    value_counts = Counter()

    # Loop through cards in hand assuming the hand is sorted
    # and that each ace appears in the hand twice (at the low
    # end and at the high end). We insort hands into a list of
    # hands to efficiently sort hands. The last hand is the hand
    # with the max value.
    hands = []
    for i, (value, suit) in enumerate(cards):
        value_counts[value] += 1

        # Don't care about high cards unless they're the last one
        # in the hand and there're no better hands.
        if i == (len(cards) - 1):
            if value_counts[value] == 1 and len(sets) == 0:
                sets[Rank.HIGH_CARD].add(value)

        if value_counts[value] == 2:
            sets[Rank.ONE_PAIR].add(value)

            # Check if a pair also occurs, then both pairs
            # make a two pair.
            if len(sets[Rank.ONE_PAIR]) >= 2:
                sets[Rank.TWO_PAIR].add(value)

            # Check if a three of a kind also occurs, then the pair
            # and three of a kind make a full house.
            if Rank.THREE_OF_A_KIND in sets and len(sets[Rank.THREE_OF_A_KIND]) == 1:
                three_of_a_kind_value, = sets[Rank.THREE_OF_A_KIND]
                sets[Rank.FULL_HOUSE].add(three_of_a_kind_value)

        elif value_counts[value] == 3:
            sets[Rank.ONE_PAIR].remove(value)
            sets[Rank.THREE_OF_A_KIND].add(value)

            # Check if a pair also occurs, then the three of a kind
            # and the pair make a full house.
            if Rank.ONE_PAIR in sets and len(sets[Rank.ONE_PAIR]) >= 1:
                sets[Rank.FULL_HOUSE].add(value)

            # Check if another three of a kind occurs, then both three
            # of kinds make a full house.
            if len(sets[Rank.THREE_OF_A_KIND]) == 2:
                sets[Rank.FULL_HOUSE].add(value)

        elif value_counts[value] == 4:
            sets[Rank.THREE_OF_A_KIND].remove(value)
            sets[Rank.FOUR_OF_A_KIND].add(value)

            # If a four of a kind appears, nothing else better can appear.
            # It's okay to break early.
            break

        # Keep a count of cards that're in sequential order, tracking the
        # max and min cards for a (potential) straight. These high/low cards
        # are compared against flush cards later. If there's a flush high/low
        # that overlaps with the straight high/low, then we know there's
        # a straight flush.
        if value == (prev_value_for_straight + 1):
            straight_count += 1
            high = value
        elif value == prev_value_for_straight:
            pass
        else:
            straight_count = 1
            high = value
            low = value

        prev_value_for_straight = value
        if straight_count >= 5:
            straights.add(tuple(range(low, high + 1)))

        # Keep a count of cards for each suit. If the suit count
        # reaches a flush, it's also compared against straights
        # for the straight flush potential. If there's no overlap with
        # a straight, then we know we at least have a flush.
        suit_counts[suit].append(value)
        if len(suit_counts[suit]) >= 5:
            flush_slice = tuple(suit_counts[suit][-5:])
            if flush_slice in straights:
                straights.remove(flush_slice)
                bisect.insort(hands, (Rank.STRAIGHT_FLUSH, flush_slice[-1]))
            else:
                bisect.insort(hands, (Rank.FLUSH, flush_slice[-1]))

    for straight in straights:
        bisect.insort(hands, (Rank.STRAIGHT, straight[-1]))

    # Only need the max hand from the sets for comparison since we
    # only care about the highest ranking hand.
    rank = max(sets)
    bisect.insort(hands, (rank, max(sets[rank])))

    return hands


def argmax(*hands: list[tuple[Rank, int]]) -> list[int]:
    high_hand_indices = []
    high_hand = (-1, -1)
    for i, hand in enumerate(hands):
        if hand[-1] > high_hand:
            high_hand_indices = [i]
            high_hand = hand[-1]
        elif hand[-1] == high_hand:
            high_hand_indices.append(i)
    return high_hand_indices


h1 = sort(
    (4, Suit.HEART), (5, Suit.HEART), (6, Suit.CLUB), (7, Suit.HEART), (8, Suit.HEART)
)
h2 = sort(
    (3, Suit.DIAMOND),
    (4, Suit.DIAMOND),
    (5, Suit.DIAMOND),
    (6, Suit.DIAMOND),
    (7, Suit.DIAMOND),
)
print(h1)
print(h2)
print("-----")
assert tuple(argmax(h1, h2)) == (1,)

h1 = sort(
    (4, Suit.HEART),
    (5, Suit.HEART),
    (6, Suit.CLUB),
    (7, Suit.HEART),
    (8, Suit.HEART),
    (9, Suit.HEART),
)
h2 = sort(
    (3, Suit.DIAMOND),
    (4, Suit.DIAMOND),
    (5, Suit.DIAMOND),
    (6, Suit.DIAMOND),
    (7, Suit.DIAMOND),
    (8, Suit.DIAMOND),
)
print(h1)
print(h2)
print("-----")
assert tuple(argmax(h1, h2)) == (1,)

h1 = sort(
    (4, Suit.CLUB),
    (6, Suit.HEART),
    (8, Suit.DIAMOND),
    (10, Suit.HEART),
    (12, Suit.SPADE),
)
h2 = sort(
    (3, Suit.CLUB),
    (5, Suit.HEART),
    (7, Suit.DIAMOND),
    (9, Suit.HEART),
    (11, Suit.SPADE),
)
print(h1)
print(h2)
print("-----")
assert tuple(argmax(h1, h2)) == (0,)

h1 = sort(
    (4, Suit.CLUB),
    (4, Suit.HEART),
    (6, Suit.HEART),
    (6, Suit.DIAMOND),
    (6, Suit.CLUB),
    (12, Suit.SPADE),
)
h2 = sort(
    (3, Suit.CLUB),
    (5, Suit.HEART),
    (7, Suit.DIAMOND),
    (9, Suit.HEART),
    (11, Suit.SPADE),
)
print(h1)
print(h2)
print("-----")
assert tuple(argmax(h1, h2)) == (0,)

h1 = sort(
    (4, Suit.CLUB),
    (4, Suit.HEART),
    (4, Suit.DIAMOND),
    (6, Suit.HEART),
    (6, Suit.DIAMOND),
    (6, Suit.CLUB),
    (12, Suit.SPADE),
)
h2 = sort(
    (4, Suit.CLUB),
    (4, Suit.HEART),
    (4, Suit.DIAMOND),
    (6, Suit.HEART),
    (6, Suit.DIAMOND),
    (11, Suit.SPADE),
)
print(h1)
print(h2)
print("-----")
assert tuple(argmax(h1, h2)) == (0,)

h1 = sort(
    (4, Suit.CLUB),
    (4, Suit.HEART),
    (6, Suit.HEART),
    (12, Suit.CLUB),
    (12, Suit.SPADE),
)
h2 = sort(
    (4, Suit.CLUB),
    (4, Suit.HEART),
    (6, Suit.HEART),
    (6, Suit.DIAMOND),
    (11, Suit.SPADE),
)
print(h1)
print(h2)
print("-----")
assert tuple(argmax(h1, h2)) == (0,)

h1 = sort(
    (4, Suit.CLUB),
    (4, Suit.HEART),
    (4, Suit.DIAMOND),
    (4, Suit.SPADE),
    (12, Suit.SPADE),
    (12, Suit.DIAMOND),
    (12, Suit.HEART)
)
h2 = sort(
    (4, Suit.CLUB),
    (4, Suit.HEART),
    (6, Suit.HEART),
    (6, Suit.DIAMOND),
    (11, Suit.SPADE),
)
print(h1)
print(h2)
print("-----")
assert tuple(argmax(h1, h2)) == (0,)

h1 = sort(
    (4, Suit.CLUB),
    (12, Suit.HEART)
)
h2 = sort(
    (4, Suit.CLUB),
    (4, Suit.HEART),
    (11, Suit.SPADE),
)
print(h1)
print(h2)
print("-----")
assert tuple(argmax(h1, h2)) == (1,)
