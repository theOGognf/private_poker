use anyhow::Error;
use ctrlc::set_handler;
use pico_args::Arguments;
use std::sync::{Arc, Mutex};

mod app;
mod bot;
use app::App;
use bot::QLearning;

const HELP: &str = "\
Create poker bots and conect them to a private poker server over TCP

USAGE:
  pp_bots [OPTIONS]

OPTIONS:
  --connect IP:PORT     Server socket connection address  [default: 127.0.0.1:6969]
  --alpha   ALPHA       Bot Q-Learning rate               [default: 0.1]
  --gamma   GAMMA       Bot discount rate                 [default: 0.95]

FLAGS:
  -h, --help            Print help information
";

struct Args {
    addr: String,
    alpha: f32,
    gamma: f32,
}

fn main() -> Result<(), Error> {
    let mut pargs = Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let args = Args {
        addr: pargs
            .value_from_str("--connect")
            .unwrap_or("127.0.0.1:6969".into()),
        alpha: pargs.value_from_str("--alpha").unwrap_or("0.1".parse()?),
        gamma: pargs.value_from_str("--gamma").unwrap_or("0.95".parse()?),
    };

    // Catching signals for exit.
    set_handler(|| std::process::exit(0))?;

    let policy = Arc::new(Mutex::new(QLearning::new(args.alpha, args.gamma)));
    let terminal = ratatui::init();
    let app_result = App::new(args.addr, policy).run(terminal);
    ratatui::restore();
    app_result
}
