//! A low-level TCP poker server.
//!
//! The server runs with two threads; one for managing TCP connections
//! and exchanging data, and another for updating the poker game state
//! at fixed intervals and in response to user commands.

use anyhow::Error;
use ctrlc::set_handler;
use log::info;
use pico_args::Arguments;
use private_poker::{
    DEFAULT_MAX_USERS, GameSettings, MAX_PLAYERS,
    entities::Usd,
    server::{self, PokerConfig},
};

const HELP: &str = "\
Run a private poker server

USAGE:
  pp_server [OPTIONS]

OPTIONS:
  --bind    IP:PORT     Server socket bind address  [default: 127.0.0.1:6969]
  --buy_in  USD         New user starting money     [default: 200]

FLAGS:
  -h, --help            Print help information
";

struct Args {
    bind: String,
    buy_in: Usd,
}

fn main() -> Result<(), Error> {
    let mut pargs = Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{HELP}");
        std::process::exit(0);
    }

    let args = Args {
        bind: pargs
            .value_from_str("--bind")
            .unwrap_or("127.0.0.1:6969".into()),
        buy_in: pargs.value_from_str("--buy_in").unwrap_or(200),
    };

    let game_settings = GameSettings::new(MAX_PLAYERS, DEFAULT_MAX_USERS, args.buy_in);
    let config: PokerConfig = game_settings.into();

    // Catching signals for exit.
    set_handler(|| std::process::exit(0))?;

    env_logger::builder().format_target(false).init();
    info!("starting at {}", args.bind);
    server::run(&args.bind, config)?;

    Ok(())
}
