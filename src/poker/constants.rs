// Don't want too many people waiting to play the game.
pub const MAX_PLAYERS: usize = 12;
pub const MAX_USERS: usize = MAX_PLAYERS + 6;
// In the wild case that players have monotonically increasing
// stacks and they all go all-in.
pub const MAX_POTS: usize = MAX_PLAYERS / 3;
// Technically a hand can only consist of 7 cards, but we treat aces
// as two separate cards (1u8 and 14u8).
pub const MAX_CARDS: usize = 11;
