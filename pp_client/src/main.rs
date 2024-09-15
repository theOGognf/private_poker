use anyhow::Error;

use clap::{Arg, Command};
use private_poker::{constants::MAX_USERNAME_LENGTH, Client};

mod app;
use app::App;

fn main() -> Result<(), Error> {
    let username = Arg::new("username")
        .help("client username")
        .required(true)
        .value_name("USERNAME");

    let addr = Arg::new("connect")
        .help("server socket connection address")
        .default_value("127.0.0.1:6969")
        .long("connect")
        .value_name("IP:PORT");

    let matches = Command::new("pp_client")
        .about("connect to a centralized poker server over TCP")
        .version("0.0.1")
        .arg(addr)
        .arg(username)
        .get_matches();

    let mut username = matches
        .get_one::<String>("username")
        .expect("username is an invalid string")
        .to_string();
    username.truncate(MAX_USERNAME_LENGTH);

    let addr = matches
        .get_one::<String>("connect")
        .expect("server address is an invalid string");

    // Doesn't make sense to use the complexity of non-blocking IO
    // for connecting to the poker server, so we try to connect with
    // a blocking client instead. The client is then eventually
    // converted to a non-blocking stream and polled for events.
    let (client, view) = Client::connect(&username, addr)?;
    let Client {
        username,
        addr,
        stream,
    } = client;
    let terminal = ratatui::init();
    let app_result = App::new(username, addr).run(stream, view, terminal);
    ratatui::restore();
    app_result
}
