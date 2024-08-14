use anyhow::{bail, Error};
use mio::{
    net::{TcpListener, TcpStream},
    Events, Interest, Poll, Token,
};
use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    io::{self},
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use crate::poker::{constants::MAX_USERS, entities::Action, game::UserError, PokerState};

use super::{
    messages::{
        ClientCommand, ClientError, ClientMessage, ServerMessage, ServerResponse, UserState,
    },
    utils::{read_prefixed, write_prefixed},
};

pub const CLIENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_NETWORK_EVENTS_PER_USER: usize = 6;
pub const MAX_NETWORK_EVENTS: usize = MAX_NETWORK_EVENTS_PER_USER * MAX_USERS;
pub const SERVER: Token = Token(0);
pub const SERVER_POLL_TIMEOUT: Duration = Duration::from_secs(1);
pub const STATE_ACTION_TIMEOUT: Duration = Duration::from_secs(10);
pub const STATE_STEP_TIMEOUT: Duration = Duration::from_secs(5);

struct UnconfirmedClient {
    stream: TcpStream,
    t: Instant,
    timeout: Duration,
}

impl UnconfirmedClient {
    pub fn new(stream: TcpStream) -> Self {
        UnconfirmedClient {
            stream,
            t: Instant::now(),
            timeout: Duration::ZERO,
        }
    }
}

/// This manager enables a few mechanisms:
///
/// - Helps keep tokens bounded, recycling unused tokens for future
///   connections.
/// - Associates tokens and usernames with clients, making it easier
///   to read from and write to clients based on those attributes.
/// - Tracks client connection states, dividing clients that haven't
///   sent their username, clients that have sent their username but
///   their usernames haven't been confirmed by the poker game, and
///   clients that have sent their usernames and those usernames have
///   been confirmed by the poker game.
struct TokenManager {
    pub confirmed_tokens: BTreeMap<Token, TcpStream>,
    confirmed_usernames_to_tokens: HashMap<String, Token>,
    recycled_tokens: BTreeSet<Token>,
    tokens_to_usernames: BTreeMap<Token, String>,
    unconfirmed_tokens: BTreeMap<Token, UnconfirmedClient>,
    unconfirmed_usernames_to_tokens: HashMap<String, Token>,
}

impl TokenManager {
    pub fn associate_token_and_stream(&mut self, token: Token, stream: TcpStream) {
        let unconfirmed_client = UnconfirmedClient::new(stream);
        self.unconfirmed_tokens.insert(token, unconfirmed_client);
    }

    /// Associate a token with a username.
    pub fn associate_token_and_username(
        &mut self,
        token: Token,
        username: String,
    ) -> Result<(), ClientError> {
        if self.tokens_to_usernames.contains_key(&token)
            || self.unconfirmed_usernames_to_tokens.contains_key(&username)
            || self.confirmed_usernames_to_tokens.contains_key(&username)
        {
            Err(ClientError::AlreadyAssociated)
        } else if self.recycled_tokens.contains(&token) {
            Err(ClientError::Expired)
        } else {
            self.tokens_to_usernames.insert(token, username.clone());
            self.unconfirmed_usernames_to_tokens.insert(username, token);
            Ok(())
        }
    }

    pub fn confirm_username(&mut self, token: Token) -> Result<(), ClientError> {
        match self.tokens_to_usernames.get(&token) {
            Some(username) => match self.unconfirmed_usernames_to_tokens.remove_entry(username) {
                Some((username, token)) => match self.unconfirmed_tokens.remove(&token) {
                    Some(unconfirmed_client) => {
                        self.confirmed_tokens
                            .insert(token, unconfirmed_client.stream);
                        self.confirmed_usernames_to_tokens.insert(username, token);
                        Ok(())
                    }
                    None => unreachable!(
                        "An unconfirmed username always corresponds to an unconfirmed token."
                    ),
                },
                None => Err(ClientError::Unassociated),
            },
            None => Err(ClientError::Unassociated),
        }
    }

    pub fn get_confirmed_username_with_token(&self, token: &Token) -> Result<String, ClientError> {
        match (
            self.confirmed_tokens.contains_key(token),
            self.tokens_to_usernames.get(token),
        ) {
            (true, Some(username)) => Ok(username.clone()),
            _ => Err(ClientError::Unassociated),
        }
    }

    pub fn get_mut_stream_with_token(
        &mut self,
        token: &Token,
    ) -> Result<&mut TcpStream, ClientError> {
        match (
            self.unconfirmed_tokens.get_mut(token),
            self.confirmed_tokens.get_mut(token),
        ) {
            (Some(unconfirmed_client), None) => Ok(&mut unconfirmed_client.stream),
            (None, Some(stream)) => Ok(stream),
            (None, None) => Err(ClientError::DoesNotExist),
            _ => unreachable!("A token must be either unconfirmed or confirmed."),
        }
    }

    pub fn get_token_with_username(&self, username: &str) -> Result<Token, ClientError> {
        match (
            self.unconfirmed_usernames_to_tokens.get(username),
            self.confirmed_usernames_to_tokens.get(username),
        ) {
            (Some(token), None) => Ok(*token),
            (None, Some(token)) => Ok(*token),
            _ => Err(ClientError::Unassociated),
        }
    }

    pub fn new() -> Self {
        Self {
            confirmed_tokens: BTreeMap::new(),
            confirmed_usernames_to_tokens: HashMap::new(),
            recycled_tokens: BTreeSet::new(),
            tokens_to_usernames: BTreeMap::new(),
            unconfirmed_tokens: BTreeMap::new(),
            unconfirmed_usernames_to_tokens: HashMap::new(),
        }
    }

    /// Create a new token and link it to the given stream.
    pub fn new_token(&mut self) -> Token {
        let token = match self.recycled_tokens.pop_last() {
            Some(token) => token,
            None => {
                let newest = match (
                    self.unconfirmed_tokens.last_key_value(),
                    self.confirmed_tokens.last_key_value(),
                ) {
                    (Some((unconfirmed, _)), Some((confirmed, _))) => max(unconfirmed, confirmed),
                    (Some((unconfirmed, _)), None) => unconfirmed,
                    (None, Some((verified, _))) => verified,
                    (None, None) => &SERVER,
                };
                Token(newest.0 + 1)
            }
        };
        token
    }

    pub fn recycle_token(&mut self, token: Token) -> Result<TcpStream, ClientError> {
        if let Some(username) = self.tokens_to_usernames.remove(&token) {
            self.unconfirmed_usernames_to_tokens.remove(&username);
            self.confirmed_usernames_to_tokens.remove(&username);
        }
        let stream = match (
            self.unconfirmed_tokens.remove(&token),
            self.confirmed_tokens.remove(&token),
        ) {
            (Some(unconfirmed), None) => unconfirmed.stream,
            (None, Some(stream)) => stream,
            (None, None) => return Err(ClientError::DoesNotExist),
            _ => unreachable!("A token must be either unconfirmed or confirmed."),
        };
        self.recycled_tokens.insert(token);
        Ok(stream)
    }

    /// Remove tokens that've gone stale because the client has yet
    /// to associate a username with itself before the association timeout.
    pub fn recycle_expired_tokens(&mut self) -> VecDeque<(Token, TcpStream)> {
        let mut tokens_to_recycle = VecDeque::new();
        for (token, unknown_client) in self
            .unconfirmed_tokens
            .iter_mut()
            .filter(|(token, _)| !self.tokens_to_usernames.contains_key(token))
        {
            let t = Instant::now();
            let dt = t - unknown_client.t;
            unknown_client.t = t;
            unknown_client.timeout += dt;
            if unknown_client.timeout >= CLIENT_CONNECT_TIMEOUT {
                tokens_to_recycle.push_back(*token);
            }
        }
        let mut recyclables = VecDeque::new();
        for token in tokens_to_recycle {
            match self.unconfirmed_tokens.remove(&token) {
                Some(unconfirmed_client) => {
                    recyclables.push_back((token, unconfirmed_client.stream))
                }
                None => unreachable!("An unassociated token is always unconfirmed."),
            }
            self.recycled_tokens.insert(token);
        }
        recyclables
    }
}

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

    // This thread is where the actual networking happens for non-blocking IO.
    // A server is bound to the address and manages connections to clients.
    // Messages from the main thread are queued for each client/user
    // connection.
    thread::spawn(move || -> Result<(), Error> {
        let mut events = Events::with_capacity(MAX_NETWORK_EVENTS);
        let mut messages_to_process: HashMap<Token, VecDeque<ClientMessage>> = HashMap::new();
        let mut messages_to_write: HashMap<Token, VecDeque<ServerResponse>> = HashMap::new();
        let mut poll = Poll::new()?;
        let mut server = TcpListener::bind(addr)?;
        let mut token_manager = TokenManager::new();
        let mut tokens_to_remove: HashSet<Token> = HashSet::new();
        poll.registry()
            .register(&mut server, SERVER, Interest::READABLE)?;

        loop {
            if let Err(error) = poll.poll(&mut events, Some(SERVER_POLL_TIMEOUT)) {
                match error.kind() {
                    io::ErrorKind::Interrupted => continue,
                    _ => bail!(error),
                }
            }

            for event in events.iter() {
                // Drain server messages received from the parent thread so
                // they can be relayed to the respective clients.
                while let Ok(msg) = rx_server.try_recv() {
                    match msg {
                        // Acks are effectively successful responses to client
                        // messages and are relayed to all clients.
                        ServerMessage::Ack(msg) => {
                            // We only need to check this connect edge case because all other
                            // client commands can only go through to the parent thread if the
                            // client's username has already been confirmed by the parent
                            // thread.
                            if msg.command == ClientCommand::Connect {
                                let disconnected = token_manager
                                    .get_token_with_username(&msg.username)
                                    .map_or(true, |token| {
                                        token_manager.confirm_username(token).is_err()
                                    });
                                // The client disconnected before the server could confirm their
                                // username even though the username was OK. A bit of an edge case,
                                // we need to notify the main thread that they disconnected. We'll
                                // still send out the acknowledgement to other clients saying that
                                // they were able to connect briefly.
                                if disconnected {
                                    let msg = ClientMessage {
                                        username: msg.username.clone(),
                                        command: ClientCommand::Leave,
                                    };
                                    tx_client.send(msg)?;
                                }
                            }
                            for token in token_manager.confirmed_tokens.keys() {
                                let msg = ServerResponse::Ack(msg.clone());
                                messages_to_write.entry(*token).or_default().push_back(msg);
                            }
                        }
                        // A response goes to a single client. We can safely ignore cases where a
                        // client no longer exists to receive a response because the response
                        // is meant just for the client.
                        ServerMessage::Response { username, data } => {
                            if let Ok(token) = token_manager.get_token_with_username(&username) {
                                messages_to_write.entry(token).or_default().push_back(*data);
                            }
                        }
                        // Views go to all clients. We can safely ignore cases where a client
                        // no longer exists to receive a view because the view is specific
                        // to the client.
                        ServerMessage::Views(views) => {
                            for (username, view) in views {
                                if let Ok(token) = token_manager.get_token_with_username(&username)
                                {
                                    let msg = ServerResponse::GameView(view);
                                    messages_to_write.entry(token).or_default().push_back(msg);
                                }
                            }
                        }
                    }
                }

                match event.token() {
                    SERVER => loop {
                        // Received an event for the TCP server socket, which
                        // indicates we can accept a connection.
                        let mut stream = match server.accept() {
                            Ok((stream, _)) => stream,
                            Err(error) => {
                                match error.kind() {
                                    // If we get a `WouldBlock` error we know our
                                    // listener has no more incoming connections queued,
                                    // so we can return to polling and wait for some
                                    // more.
                                    io::ErrorKind::WouldBlock => break,
                                    // If it was any other kind of error, something went
                                    // wrong and we should terminate.
                                    _ => bail!(error),
                                }
                            }
                        };

                        let token = token_manager.new_token();
                        poll.registry().register(
                            &mut stream,
                            token,
                            Interest::READABLE | Interest::WRITABLE,
                        )?;
                        token_manager.associate_token_and_stream(token, stream);
                    },
                    token => {
                        // Maybe received an event for a TCP connection.
                        if let Ok(stream) = token_manager.get_mut_stream_with_token(&token) {
                            if event.is_writable() {
                                if let Some(messages) = messages_to_write.get_mut(&token) {
                                    while let Some(msg) = messages.pop_front() {
                                        match write_prefixed::<ServerResponse, TcpStream>(
                                            stream, &msg,
                                        ) {
                                            Ok(_) => {
                                                // Client errors are strict and result in the removal of a connection.
                                                if let ServerResponse::ClientError(_) = msg {
                                                    tokens_to_remove.insert(token);
                                                }
                                            }
                                            Err(error) => {
                                                // The message couldn't be sent, so we need to push it back
                                                // onto the queue so we don't accidentally forget about it.
                                                // This is an expensive and infrequent operation.
                                                messages.push_front(msg);
                                                match error.kind() {
                                                    // `write_prefixed` uses `write_all` under the hood, so we know
                                                    // that an Eof error means the connection was dropped.
                                                    io::ErrorKind::UnexpectedEof => {
                                                        tokens_to_remove.insert(token);
                                                    }
                                                    // Would block "errors" are the OS's way of saying that the
                                                    // connection is not actually ready to perform this I/O operation.
                                                    io::ErrorKind::WouldBlock => {}
                                                    // Other errors we'll consider fatal.
                                                    _ => bail!(error),
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }

                                if event.is_readable() {
                                    // We can (maybe) read from the connection.
                                    loop {
                                        match read_prefixed::<ClientMessage, TcpStream>(stream) {
                                            Ok(msg) => {
                                                messages_to_process
                                                    .entry(token)
                                                    .or_default()
                                                    .push_back(msg);
                                            }
                                            Err(error) => {
                                                match error.kind() {
                                                    // `read_prefixed` uses `read_exact` under the hood, so we know
                                                    // that an Eof error means the connection was dropped.
                                                    io::ErrorKind::UnexpectedEof => {
                                                        tokens_to_remove.insert(token);
                                                    }
                                                    // Would block "errors" are the OS's way of saying that the
                                                    // connection is not actually ready to perform this I/O operation.
                                                    io::ErrorKind::WouldBlock => {}
                                                    // Other errors we'll consider fatal.
                                                    _ => bail!(error),
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // Process all the messages received from the clients.
                        for (token, msgs) in messages_to_process.drain() {
                            for msg in msgs {
                                let result = match msg.command {
                                    // Check if the client wasn't able to associate its token with a username
                                    // in time, or if that username is already taken.
                                    ClientCommand::Connect => token_manager
                                        .associate_token_and_username(token, msg.username.clone()),
                                    // Check if the client is being faithful and sending messages with
                                    // the correct username.
                                    _ => match token_manager.get_token_with_username(&msg.username)
                                    {
                                        Ok(associated_token) => {
                                            if token == associated_token {
                                                Ok(())
                                            } else {
                                                Err(ClientError::Unassociated)
                                            }
                                        }
                                        Err(error) => Err(error),
                                    },
                                };
                                match result {
                                    Ok(_) => tx_client.send(msg)?,
                                    Err(error) => {
                                        let msg = ServerResponse::ClientError(error);
                                        messages_to_write.entry(token).or_default().push_back(msg);
                                    }
                                }
                            }
                        }

                        // Recycle all tokens that need to be remove, deregistering their streams
                        // with the poll.
                        for token in tokens_to_remove.drain() {
                            if let Ok(username) =
                                token_manager.get_confirmed_username_with_token(&token)
                            {
                                let msg = ClientMessage {
                                    username,
                                    command: ClientCommand::Leave,
                                };
                                tx_client.send(msg)?;
                            }
                            messages_to_write.remove(&token);
                            if let Ok(mut stream) = token_manager.recycle_token(token) {
                                poll.registry().deregister(&mut stream)?;
                            }
                        }
                        for (token, mut stream) in token_manager.recycle_expired_tokens() {
                            messages_to_write.remove(&token);
                            poll.registry().deregister(&mut stream)?;
                        }
                    }
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
                            let msg = ServerMessage::Ack(ClientMessage {
                                username: username.clone(),
                                command,
                            });
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
                                data: Box::new(ServerResponse::UserError(error)),
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
