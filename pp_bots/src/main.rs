use anyhow::Error;
use ctrlc::set_handler;
use log::info;
use pico_args::Arguments;
use std::{
    sync::{Arc, Mutex},
    thread,
};

mod bot;
use bot::{Bot, QLearning};

const HELP: &str = "\
Create poker bots and conect them to a private poker server over TCP

USAGE:
  pp_bots [OPTIONS] BOTNAME1 BOTNAME2 ...

OPTIONS:
  --connect IP:PORT     Server socket connection address  [default: 127.0.0.1:6969]
  --alpha   ALPHA       Q-Learning rate                   [default: 0.1]
  --gamma   GAMMA       Discount rate                     [default: 0.95]

FLAGS:
  -h, --help            Print help information
";

struct Args {
    addr: String,
    alpha: f32,
    gamma: f32,
    botnames: Vec<String>,
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
        botnames: pargs
            .finish()
            .iter()
            .map(|s| s.to_str().unwrap().to_string())
            .collect(),
    };

    if args.botnames.is_empty() {
        println!("no botnames provided");
        std::process::exit(0);
    }
    // Catching signals for exit.
    set_handler(|| std::process::exit(0))?;

    let policy = Arc::new(Mutex::new(QLearning::new(args.alpha, args.gamma)));
    let workers: Vec<_> = args
        .botnames
        .into_iter()
        .map(|botname| {
            thread::spawn({
                let addr = args.addr.clone();
                let policy = policy.clone();
                move || -> Result<(), Error> {
                    let mut env = Bot::new(&botname, &addr)?;
                    loop {
                        let (mut state1, mut masks1) = env.reset()?;
                        loop {
                            let action = {
                                let mut policy = policy.lock().expect("sample lock");
                                info!("{botname} sampling");
                                policy.sample(state1.clone(), masks1.clone())
                            };
                            let (state2, masks2, reward, done) = env.step(action.clone())?;
                            if done {
                                let mut policy = policy.lock().expect("done lock");
                                info!("{botname} done update");
                                policy.update_done(state1.clone(), action.clone(), reward);
                                break;
                            }
                            {
                                let mut policy = policy.lock().expect("step lock");
                                info!("{botname} step update");
                                policy.update_step(
                                    state1.clone(),
                                    action.clone(),
                                    reward,
                                    state2.clone(),
                                    masks2.clone(),
                                );
                            }
                            state1.clone_from(&state2);
                            masks1.clone_from(&masks2);
                        }
                    }
                }
            })
        })
        .collect();

    for worker in workers {
        let _ = worker.join();
    }

    Ok(())
}
