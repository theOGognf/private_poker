use mio::net::TcpListener;

use std::{net::SocketAddr, thread, time::Duration};

use private_poker::{
    Client, UserError,
    game::{GameEvent, entities::Username},
    messages,
    server::{self, PokerConfig, ServerTimeouts},
};

fn get_random_open_port() -> u16 {
    let addr = "127.0.0.1:0".parse().unwrap();
    // Bind to port 0, which tells the OS to assign an available port
    let listener = TcpListener::bind(addr).unwrap();
    // Get the assigned port
    listener.local_addr().unwrap().port()
}

#[test]
fn already_associated_err() {
    let port = get_random_open_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    thread::spawn(move || server::run(addr.clone(), server::PokerConfig::default()));

    // Connect, make sure we're spectating.
    let username = Username::new("ognf");
    let (client, view) = Client::connect(username, &addr).unwrap();
    assert_eq!(view.spectators.len(), 1);
    assert_eq!(view.waitlist.len(), 0);
    assert!(view.spectators.contains(&client.username));

    // Try to connect, but the username is already taken.
    let username = Username::new("ognf");
    assert!(Client::connect(username, &addr).is_err());
}

#[test]
fn one_user_connects_to_lobby() {
    let port = get_random_open_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    thread::spawn(move || server::run(addr.clone(), server::PokerConfig::default()));

    // Connect, make sure we're spectating.
    let username = Username::new("ognf");
    let (mut client, view) = Client::connect(username, &addr).unwrap();
    assert_eq!(view.spectators.len(), 1);
    assert_eq!(view.waitlist.len(), 0);
    assert!(view.spectators.contains(&client.username));

    // Request to join players.
    client.change_state(messages::UserState::Play).unwrap();
    Client::recv_ack(&mut client.stream).unwrap();
    let view = Client::recv_view(&mut client.stream).unwrap();
    assert_eq!(view.spectators.len(), 0);
    assert_eq!(view.waitlist.len(), 1);
    assert!(!view.spectators.contains(&client.username));
    let event = Client::recv_event(&mut client.stream).unwrap();
    assert_eq!(event, GameEvent::Waitlisted(client.username.clone()));

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
    assert!(view.spectators.contains(&client.username));
    let event = Client::recv_event(&mut client.stream).unwrap();
    assert_eq!(event, GameEvent::Spectated(client.username.clone()));
}

#[test]
fn one_user_fails_to_connect_to_lobby() {
    let port = get_random_open_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let config: PokerConfig = ServerTimeouts {
        action: Duration::ZERO,
        connect: Duration::ZERO,
        poll: Duration::from_secs(5),
        step: Duration::from_secs(5),
    }
    .into();
    thread::spawn(move || server::run(addr.clone(), config));

    // Try to connect, but we won't be fast enough.
    let username = Username::new("ognf");
    assert!(Client::connect(username, &addr).is_err());
}
