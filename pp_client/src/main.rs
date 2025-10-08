//! A low-level TCP poker client and TUI built with [`ratatui`].
//!
//! The client runs with two threads; one for managing the TCP connection
//! and exchanging data, and another for updating the TUI at fixed
//! intervals and in response to user commands.
//!
//! [`ratatui`]: https://github.com/ratatui/ratatui

use std::net::SocketAddr;

use anyhow::Error;

use pico_args::Arguments;
use private_poker::{Client, entities::Username};

mod app;
use app::App;

const HELP: &str = "\
Connect to a private poker server over TCP

USAGE:
  pp_client [OPTIONS] USERNAME

OPTIONS:
  --connect IP:PORT     Server socket connection address  [default: 127.0.0.1:6969]

FLAGS:
  -h, --help            Print help information
";

struct Args {
    addr: SocketAddr,
    username: Username,
}

fn main() -> Result<(), Error> {
    let mut pargs = Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{HELP}");
        std::process::exit(0);
    }

    let args = Args {
        addr: pargs
            .value_from_str("--connect")
            .unwrap_or("127.0.0.1:6969".parse()?),
        username: pargs.free_from_str().unwrap_or(whoami::username()).into(),
    };

    // Doesn't make sense to use the complexity of non-blocking IO
    // for connecting to the poker server, so we try to connect with
    // a blocking client instead. The client is then eventually
    // converted to a non-blocking stream and polled for events.
    let (client, view) = Client::connect(args.username, &args.addr)?;
    let Client { username, stream } = client;
    let terminal = ratatui::init();
    let app_result = App::new(args.addr, username).run(stream, view, terminal);
    ratatui::restore();
    app_result
}
