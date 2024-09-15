// Don't want too many people waiting to play the game.
pub const MAX_PLAYERS: usize = 10;
pub const DEFAULT_MAX_USERS: usize = MAX_PLAYERS + 6;
// In the wild case that players have monotonically increasing
// stacks and they all go all-in.
pub const MAX_POTS: usize = MAX_PLAYERS / 2 + 1;
pub const MAX_USERNAME_LENGTH: usize = 16;
