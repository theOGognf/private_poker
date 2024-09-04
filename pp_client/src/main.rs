use anyhow::Error;

use clap::{Arg, Command};
use private_poker::Client;

mod app;

use app::App;

fn main() -> Result<(), Error> {
    let username = Arg::new("username")
        .help("Client username.")
        .required(true)
        .value_name("USERNAME");

    let addr = Arg::new("connect")
        .help("Server socket connection address.")
        .default_value("127.0.0.1:6969")
        .long("connect")
        .value_name("IP:PORT");

    let matches = Command::new("pp_client")
        .about("Connect to a centralized poker server over TCP.")
        .version("0.0.1")
        .arg(addr)
        .arg(username)
        .get_matches();

    let username = matches
        .get_one::<String>("username")
        .expect("Username is an invalid string.");

    let addr = matches
        .get_one::<String>("connect")
        .expect("Server address is an invalid string.");

    env_logger::builder().format_target(false).init();
    let (client, view) = Client::connect(username, addr)?;
    let terminal = ratatui::init();
    let app_result = App::new(username.clone(), addr.clone()).run(client, view, terminal);
    ratatui::restore();
    app_result
}
