use anyhow::Error;

use pico_args::Arguments;
use private_poker::{constants::MAX_USER_INPUT_LENGTH, entities::Username};

mod bots;
use bots::run;

const HELP: &str = "\
Create a poker bot and conect it to a private poker server over TCP

USAGE:
  pp_bot [OPTIONS] USERNAME

OPTIONS:
  --connect IP:PORT     Server socket connection address  [default: 127.0.0.1:6969]

FLAGS:
  -h, --help            Print help information
";

struct Args {
    username: Username,
    addr: String,
}

fn main() -> Result<(), Error> {
    let mut pargs = Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let mut args = Args {
        addr: pargs
            .value_from_str("--connect")
            .unwrap_or("127.0.0.1:6969".into()),
        username: pargs.free_from_str().unwrap_or("bot".to_string()),
    };
    args.username.truncate(MAX_USER_INPUT_LENGTH);

    run(&args.username, &args.addr)?;
    Ok(())
}
