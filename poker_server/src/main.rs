use anyhow::Error;
use clap::{value_parser, Arg, Command};
use log::info;
use poker::{
    entities::Usd,
    server::{self, PokerConfig},
    GameSettings, DEFAULT_MAX_USERS, MAX_PLAYERS,
};

fn main() -> Result<(), Error> {
    let address = Arg::new("address")
        .help("Server socket bind address.")
        .default_value("127.0.0.1:6969")
        .long("address")
        .value_name("IP:PORT");

    let buy_in = Arg::new("buy_in")
        .help("New user starting money.")
        .default_value("200")
        .long("buy_in")
        .value_name("USD")
        .value_parser(value_parser!(Usd));

    let matches = Command::new("poker_server")
        .about("Host a centralized poker server over TCP.")
        .version("0.0.1")
        .arg(address)
        .arg(buy_in)
        .get_matches();

    let address = matches.get_one::<String>("address").unwrap();
    let buy_in = matches.get_one::<Usd>("buy_in").unwrap();

    let game_settings = GameSettings::new(MAX_PLAYERS, DEFAULT_MAX_USERS, *buy_in);
    let config: PokerConfig = game_settings.into();

    env_logger::init();
    info!("Starting at {address}.");
    server::run(address, config)?;

    Ok(())
}
