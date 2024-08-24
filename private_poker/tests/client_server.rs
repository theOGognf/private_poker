use mio::net::TcpListener;

use std::{thread, time::Duration};

use private_poker::{
    messages,
    server::{self, PokerConfig, ServerTimeouts},
    Client, UserError,
};

fn get_random_open_port() -> u16 {
    let addr = "127.0.0.1:0".parse().unwrap();
    // Bind to port 0, which tells the OS to assign an available port
    let listener = TcpListener::bind(addr).unwrap();
    // Get the assigned port
    listener.local_addr().unwrap().port()
}

#[test]
fn one_user_connects_to_lobby() {
    let port = get_random_open_port();
    let addr = format!("127.0.0.1:{port}");
    thread::spawn(move || server::run(&addr, server::PokerConfig::default()));

    // Connect, make sure we're spectating.
    let addr = format!("127.0.0.1:{port}");
    let username = "ognf";
    let (mut client, view) = Client::connect(&addr, username).unwrap();
    assert_eq!(view.spectators.len(), 1);
    assert_eq!(view.waitlist.len(), 0);
    assert!(view.spectators.contains_key(&client.username));

    // Request to join players.
    client.change_state(messages::UserState::Play).unwrap();
    Client::recv_ack(&mut client.stream).unwrap();
    let view = Client::recv_view(&mut client.stream).unwrap();
    assert_eq!(view.spectators.len(), 0);
    assert_eq!(view.waitlist.len(), 1);
    assert!(!view.spectators.contains_key(&client.username));

    // Prematurely start the game.
    client.start_game().unwrap();
    assert_eq!(
        Client::recv_user_error(&mut client.stream).unwrap(),
        UserError::NotEnoughPlayers
    );

    // Try to (illegally) show your hand.
    client.show_hand().unwrap();
    assert_eq!(
        Client::recv_user_error(&mut client.stream).unwrap(),
        UserError::CannotShowHand
    );

    // Go back to spectate.
    client.change_state(messages::UserState::Spectate).unwrap();
    Client::recv_ack(&mut client.stream).unwrap();
    let view = Client::recv_view(&mut client.stream).unwrap();
    assert_eq!(view.spectators.len(), 1);
    assert_eq!(view.waitlist.len(), 0);
    assert!(view.spectators.contains_key(&client.username));
}

#[test]
fn one_user_fails_to_connect_to_lobby() {
    let port = get_random_open_port();
    let addr = format!("127.0.0.1:{port}");
    let config: PokerConfig = ServerTimeouts {
        action: Duration::ZERO,
        connect: Duration::ZERO,
        poll: Duration::from_secs(5),
        step: Duration::from_secs(5),
    }
    .into();
    thread::spawn(move || server::run(&addr, config));

    // Try to connect, but we won't be fast enough.
    let addr = format!("127.0.0.1:{port}");
    let username = "ognf";
    assert!(Client::connect(&addr, username).is_err());
}
