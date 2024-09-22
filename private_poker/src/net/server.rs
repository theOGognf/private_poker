use anyhow::{bail, Error};
use log::{debug, error, info, warn};
use mio::{
    net::{TcpListener, TcpStream},
    Events, Interest, Poll, Token, Waker,
};
use serde::{Deserialize, Serialize};
use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    io,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use crate::{
    constants::MAX_USERNAME_LENGTH,
    game::{
        entities::{Action, GameView, Username},
        GameSettings, PokerState,
    },
};

use super::{
    messages::{ClientError, ClientMessage, ServerMessage, UserCommand, UserState},
    utils::{read_prefixed, write_prefixed},
};

pub const DEFAULT_ACTION_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const DEFAULT_POLL_TIMEOUT: Duration = Duration::from_secs(1);
pub const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_NETWORK_EVENTS_PER_USER: usize = 6;
pub const SERVER: Token = Token(0);
pub const WAKER: Token = Token(1);

/// A server message for communication between poker server threads. This
/// message is never sent directly to poker clients, but fields within the
/// underlying variants are.
#[derive(Debug, Deserialize, Serialize)]
enum ServerData {
    /// An acknowledgement of a client message, signaling that the client's
    /// command was successfully processed by the game thread.
    Ack(ClientMessage),
    /// A server message sent to a specific client.
    Response {
        username: Username,
        data: Box<ServerMessage>,
    },
    /// Game state represented as a string.
    Status(String),
    /// Mapping of usernames to their game views.
    Views(HashMap<Username, GameView>),
}

fn token_to_string(token: &Token) -> String {
    let id = token.0;
    format!("token({id})")
}

pub struct ServerTimeouts {
    pub action: Duration,
    pub connect: Duration,
    pub poll: Duration,
    pub step: Duration,
}

impl Default for ServerTimeouts {
    fn default() -> Self {
        Self {
            action: DEFAULT_ACTION_TIMEOUT,
            connect: DEFAULT_CONNECT_TIMEOUT,
            poll: DEFAULT_POLL_TIMEOUT,
            step: DEFAULT_STEP_TIMEOUT,
        }
    }
}

#[derive(Default)]
pub struct PokerConfig {
    pub game_settings: GameSettings,
    pub server_timeouts: ServerTimeouts,
}

impl From<GameSettings> for PokerConfig {
    fn from(value: GameSettings) -> Self {
        let server_timeouts = ServerTimeouts::default();
        Self {
            game_settings: value,
            server_timeouts,
        }
    }
}

impl From<ServerTimeouts> for PokerConfig {
    fn from(value: ServerTimeouts) -> Self {
        let game_config = GameSettings::default();
        Self {
            game_settings: game_config,
            server_timeouts: value,
        }
    }
}

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
    confirmed_usernames_to_tokens: HashMap<Username, Token>,
    recycled_tokens: BTreeSet<Token>,
    token_association_timeout: Duration,
    tokens_to_usernames: BTreeMap<Token, Username>,
    unconfirmed_tokens: BTreeMap<Token, UnconfirmedClient>,
    unconfirmed_usernames_to_tokens: HashMap<Username, Token>,
}

impl TokenManager {
    /// Associate a token with a TCP stream. Since tokens are usually registered
    /// with a poll, the typical workflow is:
    ///
    /// 1. Create a new token.
    /// 2. Register the token and stream with the poll.
    /// 3. Associate the token and stream with the token manager.
    ///
    /// This transfers ownership of the stream to the token manager, allowing
    /// deallocation of the stream wheenver the token is recycled.
    pub fn associate_token_and_stream(&mut self, token: Token, stream: TcpStream) {
        let unconfirmed_client = UnconfirmedClient::new(stream);
        self.unconfirmed_tokens.insert(token, unconfirmed_client);
    }

    /// Associate a token with a username. This should be called in response
    /// to a client declaring a username. This will catch cases where the username
    /// is already taken, and cases where the client took too long to declare
    /// a username after its connection has already been accepted by the server.
    pub fn associate_token_and_username(
        &mut self,
        token: Token,
        username: Username,
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

    /// Confirm a token's declared username. This acknowledges that the poker
    /// game accepted their username and relieves the token from potential
    /// expiration.
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
                        "an unconfirmed username always corresponds to an unconfirmed token"
                    ),
                },
                None => Err(ClientError::Unassociated),
            },
            None => Err(ClientError::Unassociated),
        }
    }

    pub fn get_confirmed_username_with_token(
        &self,
        token: &Token,
    ) -> Result<Username, ClientError> {
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
            _ => unreachable!("a token must be either unconfirmed or confirmed"),
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

    pub fn new(token_association_timeout: Duration) -> Self {
        Self {
            confirmed_tokens: BTreeMap::new(),
            confirmed_usernames_to_tokens: HashMap::new(),
            recycled_tokens: BTreeSet::new(),
            token_association_timeout,
            tokens_to_usernames: BTreeMap::new(),
            unconfirmed_tokens: BTreeMap::new(),
            unconfirmed_usernames_to_tokens: HashMap::new(),
        }
    }

    /// Create a new token.
    pub fn new_token(&mut self) -> Token {
        let token = match self.recycled_tokens.pop_first() {
            Some(token) => token,
            None => {
                let newest = match (
                    self.unconfirmed_tokens.last_key_value(),
                    self.confirmed_tokens.last_key_value(),
                ) {
                    (Some((unconfirmed, _)), Some((confirmed, _))) => max(unconfirmed, confirmed),
                    (Some((unconfirmed, _)), None) => unconfirmed,
                    (None, Some((verified, _))) => verified,
                    (None, None) => &WAKER,
                };
                Token(newest.0 + 1)
            }
        };
        token
    }

    /// Recycle tokens that've gone stale because the client has yet
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
            if unknown_client.timeout >= self.token_association_timeout {
                tokens_to_recycle.push_back(*token);
            }
        }
        let mut recyclables = VecDeque::new();
        for token in tokens_to_recycle {
            match self.unconfirmed_tokens.remove(&token) {
                Some(unconfirmed_client) => {
                    recyclables.push_back((token, unconfirmed_client.stream))
                }
                None => unreachable!("an unassociated token is always unconfirmed"),
            }
            self.recycled_tokens.insert(token);
        }
        recyclables
    }

    /// Manually recycle an individual token. Should be used when a client is dropped,
    /// unfaithful, or when a user leaves the game.
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
            _ => unreachable!("a token must be either unconfirmed or confirmed"),
        };
        self.recycled_tokens.insert(token);
        Ok(stream)
    }
}

/// Run the poker server in two separate threads. The parent thread manages
/// the poker game state while the child thread manages non-blocking networking
/// IO.
pub fn run(addr: &str, config: PokerConfig) -> Result<(), Error> {
    let addr = addr.parse()?;
    let max_network_events = MAX_NETWORK_EVENTS_PER_USER * config.game_settings.max_users;

    let (tx_client, rx_client): (Sender<ClientMessage>, Receiver<ClientMessage>) = channel();
    let (tx_server, rx_server): (Sender<ServerData>, Receiver<ServerData>) = channel();

    let mut poll = Poll::new()?;
    let waker = Waker::new(poll.registry(), WAKER)?;

    // This thread is where the actual networking happens for non-blocking IO.
    // A server is bound to the address and manages connections to clients.
    // Messages from the main thread are queued for each client/user
    // connection.
    thread::spawn(move || -> Result<(), Error> {
        let mut events = Events::with_capacity(max_network_events);
        let mut messages_to_process: HashMap<Token, VecDeque<ClientMessage>> = HashMap::new();
        let mut messages_to_write: HashMap<Token, VecDeque<ServerMessage>> = HashMap::new();
        let mut server = TcpListener::bind(addr)?;
        let mut token_manager = TokenManager::new(config.server_timeouts.connect);
        let mut tokens_to_remove: HashSet<Token> = HashSet::new();
        let mut tokens_to_reregister: HashSet<Token> = HashSet::new();
        poll.registry()
            .register(&mut server, SERVER, Interest::READABLE)?;

        loop {
            if let Err(error) = poll.poll(&mut events, Some(config.server_timeouts.poll)) {
                match error.kind() {
                    io::ErrorKind::Interrupted => continue,
                    _ => bail!(error),
                }
            }

            for event in events.iter() {
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
                        poll.registry()
                            .register(&mut stream, token, Interest::READABLE)?;
                        token_manager.associate_token_and_stream(token, stream);
                        let repr = token_to_string(&token);
                        debug!("accepted new connection with {repr}");
                    },
                    WAKER => {
                        // Drain server messages received from the parent thread so
                        // they can be relayed to the respective clients.
                        while let Ok(msg) = rx_server.try_recv() {
                            match msg {
                                // Acks are effectively successful responses to client
                                // messages and are relayed to all clients.
                                ServerData::Ack(msg) => {
                                    // We only need to check this connect edge case because all other
                                    // client commands can only go through to the parent thread if the
                                    // client's username has already been confirmed by the parent
                                    // thread.
                                    if msg.command == UserCommand::Connect {
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
                                                command: UserCommand::Leave,
                                            };
                                            tx_client.send(msg)?;
                                        }
                                    }
                                    for token in token_manager.confirmed_tokens.keys() {
                                        let msg = ServerMessage::Ack(msg.clone());
                                        messages_to_write.entry(*token).or_default().push_back(msg);
                                        tokens_to_reregister.insert(*token);
                                    }
                                }
                                // A response goes to a single client. We can safely ignore cases where a
                                // client no longer exists to receive a response because the response
                                // is meant just for the client.
                                ServerData::Response { username, data } => {
                                    if let Ok(token) =
                                        token_manager.get_token_with_username(&username)
                                    {
                                        messages_to_write
                                            .entry(token)
                                            .or_default()
                                            .push_back(*data);
                                        tokens_to_reregister.insert(token);
                                    }
                                }
                                // Server status is a game status update to all clients.
                                ServerData::Status(msg) => {
                                    for token in token_manager.confirmed_tokens.keys() {
                                        let msg = ServerMessage::Status(msg.clone());
                                        messages_to_write.entry(*token).or_default().push_back(msg);
                                        tokens_to_reregister.insert(*token);
                                    }
                                }
                                // Views go to all clients. We can safely ignore cases where a client
                                // no longer exists to receive a view because the view is specific
                                // to the client.
                                ServerData::Views(views) => {
                                    for (username, view) in views {
                                        if let Ok(token) =
                                            token_manager.get_token_with_username(&username)
                                        {
                                            let msg = ServerMessage::GameView(view);
                                            messages_to_write
                                                .entry(token)
                                                .or_default()
                                                .push_back(msg);
                                            tokens_to_reregister.insert(token);
                                        }
                                    }
                                }
                            }
                        }
                        for token in tokens_to_reregister.drain() {
                            if let Ok(stream) = token_manager.get_mut_stream_with_token(&token) {
                                poll.registry().reregister(
                                    stream,
                                    token,
                                    Interest::READABLE | Interest::WRITABLE,
                                )?;
                            }
                        }
                    }
                    // Only care about events associated with clients that are
                    // still valid.
                    token if !tokens_to_remove.contains(&token) => {
                        // Maybe received an event for a TCP connection.
                        if let Ok(stream) = token_manager.get_mut_stream_with_token(&token) {
                            if event.is_writable() {
                                if let Some(messages) = messages_to_write.get_mut(&token) {
                                    // Need to handle the case where there's an unresponsive or
                                    // misbehaving client that doesn't let us write messages to
                                    // them. If their message queue reaches a certain size, queue
                                    // them for removal.
                                    if messages.len() >= max_network_events {
                                        let repr = token_to_string(&token);
                                        error!(
                                            "{repr} has not been receiving and will be removed."
                                        );
                                        tokens_to_remove.insert(token);
                                        continue;
                                    }
                                    while let Some(msg) = messages.pop_front() {
                                        match write_prefixed::<ServerMessage, TcpStream>(
                                            stream, &msg,
                                        ) {
                                            Ok(_) => {
                                                // Client errors are strict and result in the removal of a connection.
                                                if let ServerMessage::ClientError(_) = msg {
                                                    let repr = token_to_string(&token);
                                                    debug!("{repr}: {msg}");
                                                    tokens_to_remove.insert(token);
                                                    break;
                                                }
                                            }
                                            Err(error) => {
                                                match error.kind() {
                                                    // `write_prefixed` uses `write_all` under the hood, so we know
                                                    // that if any of these occur, then the connection was probably
                                                    // dropped at some point.
                                                    io::ErrorKind::BrokenPipe
                                                    | io::ErrorKind::ConnectionAborted
                                                    | io::ErrorKind::ConnectionReset
                                                    | io::ErrorKind::TimedOut
                                                    | io::ErrorKind::UnexpectedEof => {
                                                        let repr = token_to_string(&token);
                                                        debug!("{repr} connection dropped");
                                                        tokens_to_remove.insert(token);
                                                    }
                                                    // Would block "errors" are the OS's way of saying that the
                                                    // connection is not actually ready to perform this I/O operation.
                                                    io::ErrorKind::WouldBlock => {
                                                        // The message couldn't be sent, so we need to push it back
                                                        // onto the queue so we don't accidentally forget about it.
                                                        messages.push_front(msg);
                                                    }
                                                    // Retry writing in the case that the full message couldn't
                                                    // be written. This should be infrequent.
                                                    io::ErrorKind::WriteZero => {
                                                        let repr = token_to_string(&token);
                                                        debug!("{repr} got a zero write, but will retry");
                                                        messages.push_front(msg);
                                                        continue;
                                                    }
                                                    // Other errors we'll consider fatal.
                                                    _ => bail!(error),
                                                }
                                                poll.registry().reregister(
                                                    stream,
                                                    token,
                                                    Interest::READABLE,
                                                )?;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }

                            if event.is_readable() {
                                // We can (maybe) read from the connection.
                                loop {
                                    match read_prefixed::<ClientMessage, TcpStream>(stream) {
                                        Ok(mut msg) => {
                                            msg.username.truncate(MAX_USERNAME_LENGTH);
                                            let messages =
                                                messages_to_process.entry(token).or_default();
                                            messages.push_back(msg);
                                            if messages.len() >= MAX_NETWORK_EVENTS_PER_USER {
                                                let repr = token_to_string(&token);
                                                error!(
                                                    "{repr} has been spamming and will be removed."
                                                );
                                                tokens_to_remove.insert(token);
                                                break;
                                            }
                                        }
                                        Err(error) => {
                                            match error.kind() {
                                                // `read_prefixed` uses `read_exact` under the hood, so we know
                                                // that an Eof error means the connection was dropped.
                                                io::ErrorKind::BrokenPipe
                                                | io::ErrorKind::ConnectionAborted
                                                | io::ErrorKind::ConnectionReset
                                                | io::ErrorKind::InvalidData
                                                | io::ErrorKind::TimedOut
                                                | io::ErrorKind::UnexpectedEof => {
                                                    let repr = token_to_string(&token);
                                                    debug!("{repr}'s connection dropped");
                                                    tokens_to_remove.insert(token);
                                                }
                                                // Would block "errors" are the OS's way of saying that the
                                                // connection is not actually ready to perform this I/O operation.
                                                io::ErrorKind::WouldBlock => {}
                                                // Other errors we'll consider fatal.
                                                _ => {
                                                    bail!(error)
                                                }
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // The client is already queued for removal and so this event
                    // will be ignored.
                    _ => {}
                }
            }

            // Process all the messages received from the clients.
            for (token, msgs) in messages_to_process
                .drain()
                .filter(|(t, _)| !tokens_to_remove.contains(t))
            {
                for msg in msgs {
                    let result = match msg.command {
                        // Check if the client wasn't able to associate its token with a username
                        // in time, or if that username is already taken.
                        UserCommand::Connect => {
                            token_manager.associate_token_and_username(token, msg.username.clone())
                        }
                        // Check if the client is being faithful and sending messages with
                        // the correct username.
                        _ => match token_manager.get_token_with_username(&msg.username) {
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
                    let repr = token_to_string(&token);
                    match result {
                        Ok(_) => {
                            debug!("{repr}: {msg}");
                            tx_client.send(msg)?
                        }
                        Err(error) => {
                            debug!("{repr}: {error}");
                            let msg = ServerMessage::ClientError(error);
                            messages_to_write.entry(token).or_default().push_back(msg);
                        }
                    }
                }
            }

            // Recycle all tokens that need to be removed, deregistering their streams
            // with the poll.
            for token in tokens_to_remove.drain() {
                let repr = token_to_string(&token);
                debug!("{repr} is being removed");
                if let Ok(username) = token_manager.get_confirmed_username_with_token(&token) {
                    let msg = ClientMessage {
                        username,
                        command: UserCommand::Leave,
                    };
                    tx_client.send(msg)?;
                }
                messages_to_write.remove(&token);
                if let Ok(mut stream) = token_manager.recycle_token(token) {
                    poll.registry().deregister(&mut stream)?;
                }
            }
            for (token, mut stream) in token_manager.recycle_expired_tokens() {
                let repr = token_to_string(&token);
                debug!("{repr} expired");
                messages_to_write.remove(&token);
                poll.registry().deregister(&mut stream)?;
            }
        }
    });

    let mut state: PokerState = config.game_settings.into();
    let mut status = state.to_string();
    loop {
        // Order is kind of key here. We get the status string before
        // we step so we can inform users what's happening rather than
        // what's going to happen in the future. This allows faster
        // feedback from a user's perspective.
        let repr = state.to_string();
        // Only send new statuses to clients to avoid spam.
        if status != repr {
            info!("{repr}");
            status = repr;
            let msg = ServerData::Status(status.clone());
            tx_server.send(msg)?;
            waker.wake()?;
        }
        state = state.step();

        let views = state.get_views();
        let msg = ServerData::Views(views);
        tx_server.send(msg)?;
        waker.wake()?;

        let mut next_action_username = state.get_next_action_username();
        let mut timeout = config.server_timeouts.step;
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
                    if let Some(ref last_username) = next_action_username {
                        // If there's a timeout, then that means the user didn't
                        // make a decision in time, and they have to fold.
                        if timeout.as_secs() == 0 && &username == last_username {
                            // Ack that they will fold (the poker state will
                            // fold for them).
                            warn!("{username} ran out of time and will be forced to fold");
                            let command = UserCommand::TakeAction(Action::Fold);
                            let msg = ServerData::Ack(ClientMessage {
                                username: username.clone(),
                                command,
                            });
                            tx_server.send(msg)?;
                            waker.wake()?;

                            // Force remove them so they don't disrupt
                            // future games and ack it.
                            warn!("{username} will be removed at the end of the game");
                            state.remove_user(&username)?;
                            let command = UserCommand::Leave;
                            let msg = ServerData::Ack(ClientMessage { username, command });
                            tx_server.send(msg)?;
                            waker.wake()?;

                            break 'command;
                        } else {
                            // Let all users know whose turn it is.
                            let turn_signal = ServerMessage::TurnSignal(action_options);
                            let status =
                                format!("it's {username}'s turn and they can {turn_signal}");
                            let msg = ServerData::Status(status.clone());
                            tx_server.send(msg)?;
                            waker.wake()?;

                            // Let player know it's their turn.
                            info!("{status}");
                            let msg = ServerData::Response {
                                username: username.clone(),
                                data: Box::new(turn_signal),
                            };
                            tx_server.send(msg)?;
                            waker.wake()?;

                            next_action_username = Some(username);
                            timeout = config.server_timeouts.action;
                        }
                    }
                }
                // If it's no one's turn and there's a timeout, then we must
                // break to update the poker state.
                _ => {
                    if timeout.as_secs() == 0 {
                        break 'command;
                    }
                }
            }

            // Use the timeout duration to process events from the server's
            // IO thread.
            while timeout.as_secs() > 0 {
                let start = Instant::now();
                if let Ok(mut msg) = rx_client.recv_timeout(timeout) {
                    let result = match msg.command {
                        UserCommand::ChangeState(ref new_user_state) => match new_user_state {
                            UserState::Play => state.waitlist_user(&msg.username),
                            UserState::Spectate => state.spectate_user(&msg.username),
                        },
                        UserCommand::Connect => state.new_user(&msg.username),
                        UserCommand::Leave => state.remove_user(&msg.username),
                        UserCommand::ShowHand => state.show_hand(&msg.username),
                        UserCommand::StartGame => state.init_start(&msg.username),
                        UserCommand::TakeAction(ref mut action) => state
                            .take_action(&msg.username, action.clone())
                            .map(|new_action| {
                                timeout = Duration::ZERO;
                                *action = new_action;
                            }),
                    };

                    // Get the result from a client's command. If their command
                    // is OK, ack the command to all clients so they know what
                    // happened. If their command is bad, send an error back to
                    // the commanding client.
                    match result {
                        Ok(()) => {
                            info!("{msg}");
                            let msg = ServerData::Ack(msg);
                            tx_server.send(msg)?;
                            waker.wake()?;

                            let msg = ServerData::Views(state.get_views());
                            tx_server.send(msg)?;
                            waker.wake()?;
                        }
                        Err(error) => {
                            error!("{error}: {msg}");
                            let msg = ServerData::Response {
                                username: msg.username,
                                data: Box::new(ServerMessage::UserError(error)),
                            };
                            tx_server.send(msg)?;
                            waker.wake()?;
                        }
                    }
                }
                timeout = timeout.saturating_sub(Instant::now() - start);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mio::{
        net::{TcpListener, TcpStream},
        Token,
    };

    use crate::net::messages::ClientError;

    use super::TokenManager;

    fn get_random_open_port() -> u16 {
        let addr = "127.0.0.1:0".parse().unwrap();
        // Bind to port 0, which tells the OS to assign an available port
        let listener = TcpListener::bind(addr).unwrap();
        // Get the assigned port
        listener.local_addr().unwrap().port()
    }

    fn get_server() -> TcpListener {
        let port = get_random_open_port();
        let addr = format!("127.0.0.1:{port}").parse().unwrap();
        TcpListener::bind(addr).unwrap()
    }

    fn get_stream(listener: &TcpListener) -> TcpStream {
        let port = listener.local_addr().unwrap().port();
        let addr = format!("127.0.0.1:{port}").parse().unwrap();
        TcpStream::connect(addr).unwrap();
        let (stream, _) = listener.accept().unwrap();
        stream
    }

    #[test]
    fn confirm_username() {
        let server = get_server();
        let stream = get_stream(&server);
        let mut token_manager = TokenManager::new(Duration::ZERO);

        let token = token_manager.new_token();
        token_manager.associate_token_and_stream(token, stream);

        let username = "ognf".to_string();
        assert_eq!(
            token_manager.get_token_with_username(&username),
            Err(ClientError::Unassociated)
        );
        assert_eq!(
            token_manager.associate_token_and_username(token, username.clone()),
            Ok(())
        );
        assert_eq!(token_manager.get_token_with_username(&username), Ok(token));

        assert_eq!(token_manager.confirm_username(token), Ok(()));
        assert_eq!(
            token_manager.get_confirmed_username_with_token(&token),
            Ok(username.clone())
        );
        assert_eq!(token_manager.get_token_with_username(&username), Ok(token));
    }

    #[test]
    fn confirm_username_recycled_token() {
        let server = get_server();
        let stream = get_stream(&server);
        let mut token_manager = TokenManager::new(Duration::ZERO);

        let token = token_manager.new_token();
        token_manager.associate_token_and_stream(token, stream);
        token_manager.recycle_expired_tokens();

        let username = "ognf".to_string();
        assert_eq!(
            token_manager.get_token_with_username(&username),
            Err(ClientError::Unassociated)
        );
        assert_eq!(
            token_manager.associate_token_and_username(token, username),
            Err(ClientError::Expired)
        );
    }

    #[test]
    fn recycle_expired_tokens() {
        let server = get_server();
        let stream1 = get_stream(&server);
        let stream2 = get_stream(&server);
        let stream3 = get_stream(&server);
        let stream4 = get_stream(&server);
        let mut token_manager = TokenManager::new(Duration::ZERO);

        // Create a couple of tokens and immediately recycle them.
        let token1 = token_manager.new_token();
        token_manager.associate_token_and_stream(token1, stream1);
        let token2 = token_manager.new_token();
        token_manager.associate_token_and_stream(token2, stream2);
        token_manager.recycle_expired_tokens();

        // Tokens are immediately resused.
        let token3 = token_manager.new_token();
        token_manager.associate_token_and_stream(token1, stream3);
        let token4 = token_manager.new_token();
        token_manager.associate_token_and_stream(token2, stream4);
        assert_eq!(token1, Token(2));
        assert_eq!(token1, token3);
        assert_eq!(token2, Token(3));
        assert_eq!(token2, token4);
    }

    #[test]
    fn recycle_token() {
        let server = get_server();
        let stream1 = get_stream(&server);
        let stream2 = get_stream(&server);
        let mut token_manager = TokenManager::new(Duration::ZERO);

        let token1 = token_manager.new_token();
        token_manager.associate_token_and_stream(token1, stream1);
        let token2 = token_manager.new_token();
        token_manager.associate_token_and_stream(token2, stream2);

        let username = "ognf".to_string();
        assert_eq!(
            token_manager.associate_token_and_username(token1, username.clone()),
            Ok(())
        );
        assert_eq!(
            token_manager.associate_token_and_username(token2, username.clone()),
            Err(ClientError::AlreadyAssociated)
        );
        assert!(token_manager.recycle_token(token1).is_ok());
        assert_eq!(
            token_manager.associate_token_and_username(token2, username),
            Ok(())
        );
        assert_eq!(token1, token_manager.new_token());
    }
}
