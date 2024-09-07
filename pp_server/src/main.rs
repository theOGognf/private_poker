use anyhow::Error;
use clap::{value_parser, Arg, Command};
use log::info;
use private_poker::{
    entities::Usd,
    server::{self, PokerConfig},
    GameSettings, DEFAULT_MAX_USERS, MAX_PLAYERS,
};

fn main() -> Result<(), Error> {
    let addr = Arg::new("bind")
        .help("Server socket bind address.")
        .default_value("127.0.0.1:6969")
        .long("bind")
        .value_name("IP:PORT");

    let buy_in = Arg::new("buy_in")
        .help("New user starting money.")
        .default_value("200")
        .long("buy_in")
        .value_name("USD")
        .value_parser(value_parser!(Usd));

    let matches = Command::new("pp_server")
        .about("Host a centralized poker server over TCP.")
        .version("0.0.1")
        .arg(addr)
        .arg(buy_in)
        .get_matches();

    let addr = matches
        .get_one::<String>("bind")
        .expect("Server address is an invalid string.");
    let buy_in = matches
        .get_one::<Usd>("buy_in")
        .expect("Buy-in is an invalid integer.");

    let game_settings = GameSettings::new(MAX_PLAYERS, DEFAULT_MAX_USERS, *buy_in);
    let config: PokerConfig = game_settings.into();

    env_logger::builder().format_target(false).init();
    info!("Starting at {addr}");
    server::run(addr, config)?;

    Ok(())
}
