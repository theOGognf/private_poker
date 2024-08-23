use mio::net::TcpListener;

use std::thread;

use poker::{server, Client};

fn get_random_open_port() -> u16 {
    let addr = "127.0.0.1:0".parse().unwrap();
    // Bind to port 0, which tells the OS to assign an available port
    let listener = TcpListener::bind(addr).unwrap();
    // Get the assigned port
    listener.local_addr().unwrap().port()
}

#[test]
fn one_user() {
    let port = get_random_open_port();
    let addr = format!("127.0.0.1:{port}");
    thread::spawn(move || server::run(&addr, server::PokerConfig::default()));

    let addr = format!("127.0.0.1:{port}");
    let username = "ognf";
    let (client, view) = Client::connect(&addr, username).unwrap();
    assert_eq!(view.spectators.len(), 1);
    assert!(view.spectators.contains_key(&client.username));
}
