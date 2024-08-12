use anyhow::{bail, Error};
use mio::{
    event::Event,
    net::{TcpListener, TcpStream},
    Events, Interest, Poll, Registry, Token,
};
use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    io::{self},
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
use thiserror;

use crate::poker::{constants::MAX_USERS, entities::Action, game::UserError, PokerState};

use super::messages::{ClientCommand, ClientMessage, ServerMessage, ServerResponse, UserState};

pub const CLIENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_NETWORK_EVENTS_PER_USER: usize = 6;
pub const MAX_NETWORK_EVENTS: usize = MAX_NETWORK_EVENTS_PER_USER * MAX_USERS;
pub const SERVER: Token = Token(0);
pub const SERVER_POLL_TIMEOUT: Duration = Duration::from_secs(1);
pub const STATE_ACTION_TIMEOUT: Duration = Duration::from_secs(10);
pub const STATE_STEP_TIMEOUT: Duration = Duration::from_secs(5);

fn change_user_state(
    poker_state: &mut PokerState,
    username: &str,
    user_state: &UserState,
) -> Result<(), UserError> {
    match user_state {
        UserState::Play => poker_state.waitlist_user(username),
        UserState::Spectate => poker_state.spectate_user(username),
    }
}

pub fn run(addr: &str) -> Result<(), Error> {
    let addr = addr.parse()?;

    let (tx_client, rx_client): (Sender<ClientMessage>, Receiver<ClientMessage>) = channel();
    let (tx_server, rx_server): (Sender<ServerMessage>, Receiver<ServerMessage>) = channel();

    // Thread is where the actual networking happens for non-blocking IO.
    // A server is bound to the address and manages connections to clients.
    // Messages from the main thread are queued for each client/user
    // connection.
    thread::spawn(move || -> Result<(), Error> {
        let mut poll = Poll::new()?;
        let mut registry = poll.registry();
        let mut server = TcpListener::bind(addr)?;
        registry.register(&mut server, SERVER, Interest::READABLE)?;
        let mut events = Events::with_capacity(MAX_NETWORK_EVENTS);

        // TODO:
        // - Need some kind of token manager so we can reuse tokens and tokens
        // aren't just incremented indefinitely. BTreeHashMap good for keeping track of the
        // next token combined with a BTreeHashSet to keep track of next token
        // to be recycled.
        // - Need some way to track the time since a client initially connected
        // and the current time. If they don't send a `CONNECT` command with
        // their username, then we need to deregister them and drop the
        // connection.
        // - Need to read server messages from `rx_server` and send client messages
        // over `tx_client`.
        // - Need to map tokens to clients and usernames to tokens. The poker loop
        // will send messages using usernames as UIDs whereas the server uses tokens
        // as UIDs. Maybe some higher-level struct that combines the token manager,
        // the client time tracker, and this thing.
        loop {
            if let Err(error) = poll.poll(&mut events, Some(SERVER_POLL_TIMEOUT)) {
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                bail!(error);
            }

            for event in events.iter() {
                match event.token() {
                    SERVER => loop {
                        // Received an event for the TCP server socket, which
                        // indicates we can accept an connection.
                        let (mut connection, address) = match server.accept() {
                            Ok((connection, address)) => (connection, address),
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                                // If we get a `WouldBlock` error we know our
                                // listener has no more incoming connections queued,
                                // so we can return to polling and wait for some
                                // more.
                                break;
                            }
                            Err(error) => {
                                // If it was any other kind of error, something went
                                // wrong and we terminate with an error.
                                bail!(error);
                            }
                        };

                        let token = next(&mut unique_token);
                        registry.register(
                            &mut connection,
                            token,
                            Interest::READABLE | Interest::WRITABLE,
                        )?;

                        connections.insert(token, connection);
                    },
                }
            }
        }
    });

    let mut state = PokerState::new();
    loop {
        state = state.step();

        let views = state.get_views();
        let msg = ServerMessage::Views(views);
        tx_server.send(msg)?;

        let mut next_action_username = state.get_next_action_username();
        let mut timeout = STATE_STEP_TIMEOUT;
        'command: loop {
            // Check if it's a user's turn. If so, send them a turn signal
            // and increase the timeout to give them time to make their
            // decision. We also keep track of their username so we
            // can tell if they don't make a decision in time.
            match (state.get_next_action_username(), state.get_action_options()) {
                (Some(username), Some(action_options)) => {
                    // Check if the username from the last turn is the same as the
                    // username from this turn. If so, we need to check if there
                    // was a timeout.
                    if let Some(last_username) = next_action_username {
                        // If there's a timeout, then that means the user didn't
                        // make a decision, and they have to fold.
                        if timeout.as_secs() == 0 && username == last_username {
                            // Ack that they will fold (the poker state will
                            // fold for them).
                            let command = ClientCommand::TakeAction(Action::Fold);
                            let msg = ServerMessage::Ack(ClientMessage { username, command });
                            tx_server.send(msg)?;

                            // Force spectate them so they don't disrupt
                            // future games and ack it.
                            state.spectate_user(&username)?;
                            let command = ClientCommand::ChangeState(UserState::Spectate);
                            let msg = ServerMessage::Ack(ClientMessage { username, command });
                            tx_server.send(msg)?;

                            break 'command;
                        }
                    }
                    let msg = ServerMessage::Response {
                        username: username.clone(),
                        data: Box::new(ServerResponse::TurnSignal(action_options)),
                    };
                    tx_server.send(msg)?;
                    next_action_username = Some(username);
                    timeout = STATE_ACTION_TIMEOUT;
                }
                // If it's no one's turn and there's a timeout, then we must
                // break to update the poker state.
                _ => {
                    if timeout.as_secs() == 0 {
                        break 'command;
                    }
                }
            }

            while timeout.as_secs() > 0 {
                let start = Instant::now();
                if let Ok(msg) = rx_client.recv_timeout(timeout) {
                    let result = match msg.command {
                        ClientCommand::ChangeState(ref new_user_state) => {
                            change_user_state(&mut state, &msg.username, new_user_state)
                        }
                        ClientCommand::Connect => state.new_user(&msg.username),
                        ClientCommand::Leave => state.remove_user(&msg.username),
                        ClientCommand::ShowHand => state.show_hand(&msg.username),
                        ClientCommand::StartGame => state.init_start(&msg.username),
                        ClientCommand::TakeAction(ref action) => {
                            state.take_action(&msg.username, action.clone()).map(|()| {
                                timeout = Duration::ZERO;
                            })
                        }
                    };

                    // Get the result from a client's command. If their command
                    // is OK, ack the command to all clients so they know what
                    // happened. If their command is bad, send an error back to
                    // the commanding client.
                    match result {
                        Ok(()) => {
                            let msg = ServerMessage::Ack(msg);
                            tx_server.send(msg)?;

                            let msg = ServerMessage::Views(state.get_views());
                            tx_server.send(msg)?;
                        }
                        Err(error) => {
                            let msg = ServerMessage::Response {
                                username: msg.username,
                                data: Box::new(ServerResponse::Error(error)),
                            };
                            tx_server.send(msg)?;
                        }
                    }
                }
                timeout -= Instant::now() - start;
            }
        }
    }
}

#[derive(Debug, Eq, thiserror::Error, PartialEq)]
enum TokenError {
    #[error("Token already associated.")]
    AlreadyAssociated,
    #[error("Token does not exist.")]
    DoesNotExist,
    #[error("Token expired.")]
    Expired,
}

struct UnknownClient {
    stream: TcpStream,
    timeout: Duration,
}

impl UnknownClient {
    pub fn new(stream: TcpStream) -> Self {
        UnknownClient {
            stream,
            timeout: Duration::ZERO,
        }
    }
}

struct TokenManager {
    recycled_tokens: BTreeSet<Token>,
    t: Instant,
    tokens_to_usernames: BTreeMap<Token, String>,
    unverified_tokens: BTreeMap<Token, UnknownClient>,
    verified_usernames: HashMap<String, TcpStream>,
    verified_usernames_to_tokens: HashMap<String, Token>,
}

impl TokenManager {
    /// Associate a token with a username.
    pub fn associate_token(&mut self, token: Token, username: String) -> Result<(), TokenError> {
        if self.tokens_to_usernames.contains_key(&token) {
            Err(TokenError::AlreadyAssociated)
        } else if !self.unverified_tokens.contains_key(&token) {
            Err(TokenError::DoesNotExist)
        } else if self.recycled_tokens.contains(&token) {
            Err(TokenError::Expired)
        } else {
            self.tokens_to_usernames.insert(token, username);
            Ok(())
        }
    }

    /// Remove tokens that've gone stale because the client has yet
    /// to associate a username with itself before the association timeout.
    fn filter_unassociated_tokens(&mut self) {
        let dt = Instant::now() - self.t;
        let mut tokens_to_recycle = VecDeque::new();
        for (token, unknown_client) in self
            .unverified_tokens
            .iter_mut()
            .filter(|(token, _)| !self.tokens_to_usernames.contains_key(token))
        {
            unknown_client.timeout += dt;
            if unknown_client.timeout >= CLIENT_CONNECT_TIMEOUT {
                tokens_to_recycle.push_back(*token);
            }
        }
        while let Some(token) = tokens_to_recycle.pop_front() {
            self.unverified_tokens.remove(&token);
            self.recycled_tokens.insert(token);
        }
        self.t = Instant::now();
    }

    pub fn get_stream_from_token(&self) -> &TcpStream {}

    pub fn get_stream_from_username(&self) -> &TcpStream {}

    /// Create a new token and link it to the given stream.
    pub fn new_token(&mut self, stream: TcpStream) -> Token {
        self.filter_unassociated_tokens();
        let token = match self.recycled_tokens.pop_last() {
            Some(token) => token,
            None => {
                let largest = match (
                    self.unverified_tokens.last_key_value(),
                    self.tokens_to_usernames.last_key_value(),
                ) {
                    (Some((reserved, _)), Some((verified, _))) => max(reserved, verified).0,
                    (Some((reserved, _)), None) => reserved.0,
                    (None, Some((verified, _))) => verified.0,
                    (None, None) => SERVER.0,
                };
                Token(largest + 1)
            }
        };
        let unknown_client = UnknownClient::new(stream);
        self.unverified_tokens.insert(token, unknown_client);
        token
    }

    pub fn recycle_token(&mut self, token: Token) -> TcpStream {
        self.filter_unassociated_tokens();
        self.tokens_to_usernames.remove(&token);
        self.recycled_tokens.insert(token);
    }

    pub fn verify_token(&mut self, token: Token) {
        self.filter_unassociated_tokens();
        self.unverified_tokens.remove(&token);
        self.verified_usernames.insert(username.clone(), stream);
        self.tokens_to_usernames.insert(token, username);
    }
}
