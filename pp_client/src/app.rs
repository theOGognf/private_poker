use anyhow::{bail, Error};
use chrono::{DateTime, Utc};
use clap::{Arg, Command};
use mio::{Events, Interest, Poll, Waker};
use private_poker::{
    entities::{Action, Usd},
    game::GameView,
    messages::UserState,
    net::{
        messages::{ClientCommand, ClientMessage, ServerResponse},
        server::{DEFAULT_POLL_TIMEOUT, SERVER, WAKER},
        utils::{read_prefixed, write_prefixed},
    },
};
use ratatui::{
    self,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Margin, Position},
    style::{Style, Stylize},
    symbols::scrollbar,
    text::{Line, Text},
    widgets::{
        Block, List, ListDirection, ListItem, ListState, Paragraph, ScrollDirection, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    DefaultTerminal, Frame,
};
use std::{
    collections::{HashSet, VecDeque},
    io,
    net::TcpStream,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::Duration,
};

pub const MAX_LOG_RECORDS: usize = 1024;
pub const POLL_TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Clone)]
enum RecordKind {
    Alert,
    Error,
    Game,
    System,
    User,
}

#[derive(Clone)]
struct Record {
    datetime: DateTime<Utc>,
    kind: RecordKind,
    content: String,
}

impl Record {
    fn new(kind: RecordKind, content: String) -> Self {
        Self {
            datetime: Utc::now(),
            kind,
            content,
        }
    }
}

impl From<Record> for ListItem<'_> {
    fn from(val: Record) -> Self {
        let kind = match val.kind {
            RecordKind::Alert => format!("{:6}", "ALERT").light_magenta(),
            RecordKind::Error => format!("{:6}", "ERROR").light_red(),
            RecordKind::Game => format!("{:6}", "GAME").light_yellow(),
            RecordKind::System => format!("{:6}", "SYSTEM").light_blue(),
            RecordKind::User => format!("{:6}", "USER").light_green(),
        };

        let msg = vec![
            format!("[{} ", val.datetime.format("%Y-%m-%d %H:%M:%S")).into(),
            kind,
            format!("]: {}", val.content).into(),
        ];

        let content = Line::from(msg);
        ListItem::new(content)
    }
}

struct LogHandle {
    list_items: VecDeque<ListItem<'static>>,
    list_state: ListState,
    scroll_state: ScrollbarState,
}

impl LogHandle {
    pub fn clear(&mut self) {
        self.jump_to_last();
        self.scroll_state = self.scroll_state.content_length(0);
        self.list_items.clear();
    }

    pub fn jump_to_first(&mut self) {
        self.list_state.scroll_down_by(MAX_LOG_RECORDS as u16);
        self.scroll_state.first();
    }

    pub fn jump_to_last(&mut self) {
        self.list_state.scroll_up_by(MAX_LOG_RECORDS as u16);
        self.scroll_state.last();
    }

    pub fn move_down(&mut self) {
        self.list_state.scroll_up_by(1);
        if self.list_state.selected().is_some() {
            self.scroll_state.scroll(ScrollDirection::Forward);
        }
    }

    pub fn move_up(&mut self) {
        self.list_state.scroll_down_by(1);
        if self.list_state.selected().is_some() {
            self.scroll_state.scroll(ScrollDirection::Backward);
        }
    }

    pub fn new() -> Self {
        Self {
            list_items: VecDeque::with_capacity(MAX_LOG_RECORDS),
            list_state: ListState::default(),
            scroll_state: ScrollbarState::new(0),
        }
    }

    pub fn push(&mut self, item: ListItem<'static>) {
        if self.list_items.len() == MAX_LOG_RECORDS {
            self.list_items.pop_back();
        }
        self.list_items.push_front(item);
        self.scroll_state = self.scroll_state.content_length(self.list_items.len());
        self.move_down();
    }

    /// Push a string that has newlines so that each line is a separate
    /// item in the log. This makes it such that each item can be scrolled
    /// through independently and large strings can be rendered in parts
    /// if they can't fit within the whole terminal.
    pub fn push_multiline_string(&mut self, item: String) {
        for content in item.split('\n') {
            let line = Line::raw(content.to_string());
            self.push(line.into());
        }
    }
}

struct UserInput {
    /// Position of cursor in the input box.
    char_idx: usize,
    /// Current value of the input box.
    value: String,
}

impl UserInput {
    pub fn backspace(&mut self) {
        // Method "remove" is not used on the saved text for deleting the selected char.
        // Reason: Using remove on String works on bytes instead of the chars.
        // Using remove would require special care because of char boundaries.
        if self.char_idx != 0 {
            // Getting all characters before the selected character.
            let before_char_to_delete = self.value.chars().take(self.char_idx - 1);
            // Getting all characters after selected character.
            let after_char_to_delete = self.value.chars().skip(self.char_idx);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.value = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_left();
        }
    }

    /// Returns the byte index based on the character position.
    ///
    /// Since each character in a string can be contain multiple bytes, it's necessary to calculate
    /// the byte index based on the index of the character.
    fn byte_idx(&self) -> usize {
        self.value
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.char_idx)
            .unwrap_or(self.value.len())
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.value.chars().count())
    }

    pub fn delete(&mut self) {
        // Method "remove" is not used on the saved text for deleting the selected char.
        // Reason: Using remove on String works on bytes instead of the chars.
        // Using remove would require special care because of char boundaries.
        if self.char_idx != self.value.len() {
            // Getting all characters before the selected character.
            let before_char_to_delete = self.value.chars().take(self.char_idx);
            // Getting all characters after selected character.
            let after_char_to_delete = self.value.chars().skip(self.char_idx + 1);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.value = before_char_to_delete.chain(after_char_to_delete).collect();
        }
    }

    pub fn input(&mut self, new_char: char) {
        let idx = self.byte_idx();
        self.value.insert(idx, new_char);
        self.move_right();
    }

    pub fn jump_to_first(&mut self) {
        self.char_idx = 0;
    }

    pub fn jump_to_last(&mut self) {
        self.char_idx = self.value.len();
    }

    pub fn move_left(&mut self) {
        let cursor_moved_left = self.char_idx.saturating_sub(1);
        self.char_idx = self.clamp_cursor(cursor_moved_left);
    }

    pub fn move_right(&mut self) {
        let cursor_moved_right = self.char_idx.saturating_add(1);
        self.char_idx = self.clamp_cursor(cursor_moved_right);
    }

    pub fn new() -> Self {
        Self {
            char_idx: 0,
            value: String::new(),
        }
    }

    pub fn submit(&mut self) -> String {
        let input = self.value.clone();
        self.char_idx = 0;
        self.value.clear();
        input
    }
}

/// App holds the state of the application
pub struct App {
    username: String,
    addr: String,
    commands: Command,
    /// History of recorded messages
    log_handle: LogHandle,
    /// Current value of the input box
    user_input: UserInput,
}

impl App {
    fn handle_command(
        &mut self,
        user_input: &str,
        action_options: &HashSet<Action>,
        view: &GameView,
        tx_client: &Sender<ClientMessage>,
        waker: &Waker,
    ) -> Result<(), Error> {
        let cmd = user_input.split(' ');
        match self.commands.clone().try_get_matches_from(cmd) {
            Ok(matches) => {
                if let Some(cmd) = matches.subcommand_name() {
                    match cmd {
                        "all-in" => {
                            if let Some(action) = action_options.get(&Action::AllIn) {
                                let msg = ClientMessage {
                                    username: self.username.to_string(),
                                    command: ClientCommand::TakeAction(action.clone()),
                                };
                                tx_client.send(msg)?;
                                waker.wake()?;
                            } else {
                                let record =
                                    Record::new(RecordKind::Error, "can't all-in now".to_string());
                                self.log_handle.push(record.into());
                            }
                        }
                        "board" => {
                            let content = view.board_to_string();
                            let line = Line::raw(content);
                            self.log_handle.push(line.into());
                        }
                        "call" => {
                            // Actions use their variant for comparisons,
                            // so we don't need to provide the correct call
                            // amount to see if it exists within the action
                            // options.
                            if let Some(action) = action_options.get(&Action::Call(0)) {
                                let msg = ClientMessage {
                                    username: self.username.to_string(),
                                    command: ClientCommand::TakeAction(action.clone()),
                                };
                                tx_client.send(msg)?;
                                waker.wake()?;
                            } else {
                                let record =
                                    Record::new(RecordKind::Error, "can't call now".to_string());
                                self.log_handle.push(record.into());
                            }
                        }
                        "check" => {
                            if let Some(action) = action_options.get(&Action::Check) {
                                let msg = ClientMessage {
                                    username: self.username.to_string(),
                                    command: ClientCommand::TakeAction(action.clone()),
                                };
                                tx_client.send(msg)?;
                                waker.wake()?;
                            } else {
                                let record =
                                    Record::new(RecordKind::Error, "can't check now".to_string());
                                self.log_handle.push(record.into());
                            }
                        }
                        "clear" => self.log_handle.clear(),
                        "exit" => bail!("exit"),
                        "fold" => {
                            if let Some(action) = action_options.get(&Action::Fold) {
                                let msg = ClientMessage {
                                    username: self.username.clone(),
                                    command: ClientCommand::TakeAction(action.clone()),
                                };
                                tx_client.send(msg)?;
                                waker.wake()?;
                            } else {
                                let record =
                                    Record::new(RecordKind::Error, "can't fold now".to_string());
                                self.log_handle.push(record.into());
                            }
                        }
                        "game" => {
                            let content = view.to_string();
                            self.log_handle.push_multiline_string(content);
                        }
                        "play" => {
                            let msg = ClientMessage {
                                username: self.username.clone(),
                                command: ClientCommand::ChangeState(UserState::Play),
                            };
                            tx_client.send(msg)?;
                            waker.wake()?;
                        }
                        "players" => {
                            let content = view.players_to_string();
                            self.log_handle.push_multiline_string(content);
                        }
                        "pots" => {
                            let content = view.pots_to_string();
                            self.log_handle.push_multiline_string(content);
                        }
                        "raise" => {
                            // Actions use their variant for comparisons,
                            // so we don't need to provide the correct raise
                            // amount to see if it exists within the action
                            // options.
                            if let Some(action) = action_options.get(&Action::Raise(0)) {
                                match matches.subcommand_matches("raise") {
                                    Some(matches) => match matches.get_one::<String>("amount") {
                                        Some(amount) => {
                                            let action = if let Ok(amount) = amount.parse::<Usd>() {
                                                Action::Raise(amount)
                                            } else {
                                                action.clone()
                                            };
                                            let msg = ClientMessage {
                                                username: self.username.to_string(),
                                                command: ClientCommand::TakeAction(action),
                                            };
                                            tx_client.send(msg)?;
                                            waker.wake()?;
                                        }
                                        None => unreachable!("always matches"),
                                    },
                                    None => {
                                        unreachable!("always matches")
                                    }
                                }
                            } else {
                                let record =
                                    Record::new(RecordKind::Error, "can't raise now".to_string());
                                self.log_handle.push(record.into());
                            }
                        }
                        "show" => {
                            let msg = ClientMessage {
                                username: self.username.clone(),
                                command: ClientCommand::ShowHand,
                            };
                            tx_client.send(msg)?;
                            waker.wake()?;
                        }
                        "spectate" => {
                            let msg = ClientMessage {
                                username: self.username.clone(),
                                command: ClientCommand::ChangeState(UserState::Spectate),
                            };
                            tx_client.send(msg)?;
                            waker.wake()?;
                        }
                        "start" => {
                            let msg = ClientMessage {
                                username: self.username.clone(),
                                command: ClientCommand::StartGame,
                            };
                            tx_client.send(msg)?;
                            waker.wake()?;
                        }
                        "table" => {
                            let content = view.table_to_string();
                            self.log_handle.push_multiline_string(content);
                        }
                        _ => unreachable!("always a subcommand"),
                    }
                }
            }
            Err(_) => match user_input {
                "help" => {
                    let help = self.commands.render_help().to_string();
                    self.log_handle.push_multiline_string(help);
                }
                invalid => {
                    let record = Record::new(
                        RecordKind::Error,
                        format!("unrecognized command: {invalid}"),
                    );
                    self.log_handle.push(record.into());
                }
            },
        }
        Ok(())
    }

    pub fn new(username: String, addr: String) -> Self {
        let all_in = Command::new("all-in").about("Go all-in, betting all your money on the hand.");
        let board = Command::new("board").about("Display community cards.");
        let call = Command::new("call").about("Match the investment required to stay in the hand.");
        let check =
            Command::new("check").about("Check, voting to move to the next card reveal(s).");
        let clear = Command::new("clear").about("Clear command outputs.");
        let exit = Command::new("exit").about("Exit the poker client.");
        let fold = Command::new("fold").about("Fold, forfeiting your hand.");
        let game = Command::new("game").about("Display all game entities.");
        let play = Command::new("play").about("Join the playing waitlist.");
        let players = Command::new("players")
            .about("Display all players (and their hands if they're showing).");
        let pots =
            Command::new("pots").about("Display the pots and the investments players have made.");
        let raise_about = [
            "Raise the investment required to stay in the hand. Entering `raise` without a value",
            "defaults to the min raise amount. Entering `raise AMOUNT` will raise by AMOUNT, but",
            "AMOUNT must be >= the min raise.",
        ]
        .join("\n");
        let raise = Command::new("raise").about(raise_about).arg(
            Arg::new("amount")
                .help("Raise amount.")
                .default_value("")
                .value_name("AMOUNT"),
        );
        let show = Command::new("show").about("Show your hand. Only possible during the showdown.");
        let spectate = Command::new("spectate").about(
            "Join spectators. If you're a player, you won't spectate until the game is over.",
        );
        let start = Command::new("start").about("Start the game.");
        let table = Command::new("table")
            .about("Display all entities at the table (cards, pots, and players).");
        let usage = [
            "Enter any of the following to interact with the poker server or render game states.\n",
            "The typical flow is:",
            "- Two or more users prepare to play with `play`",
            "- A player starts the game with `start`",
            "- Users view the game state with `game` and `table`",
            "- Players make actions with `all-in`, `call`, `check`, `fold`, and `raise`",
            "- Players show hands with `show`",
            "- Users spectate with `spectate` or leave with `exit`",
        ]
        .join("\n");
        let commands = Command::new("poker")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .next_line_help(true)
            .no_binary_name(true)
            .override_usage(usage)
            .subcommand(all_in)
            .subcommand(board)
            .subcommand(call)
            .subcommand(check)
            .subcommand(clear)
            .subcommand(exit)
            .subcommand(fold)
            .subcommand(game)
            .subcommand(play)
            .subcommand(players)
            .subcommand(pots)
            .subcommand(raise)
            .subcommand(show)
            .subcommand(spectate)
            .subcommand(start)
            .subcommand(table);
        Self {
            username,
            addr,
            commands,
            log_handle: LogHandle::new(),
            user_input: UserInput::new(),
        }
    }

    pub fn run(
        mut self,
        stream: TcpStream,
        mut view: GameView,
        mut terminal: DefaultTerminal,
    ) -> Result<(), Error> {
        let (tx_client, rx_client): (Sender<ClientMessage>, Receiver<ClientMessage>) = channel();
        let (tx_server, rx_server): (Sender<ServerResponse>, Receiver<ServerResponse>) = channel();

        let mut poll = Poll::new()?;
        let waker = Waker::new(poll.registry(), WAKER)?;

        // This thread is where the actual client-server networking happens for
        // non-blocking IO. Some non-blocking IO between client threads is also
        // managed by this thread. The UI thread sends client command messages
        // to this thread; those messages are eventually written to the server.
        thread::spawn(move || -> Result<(), Error> {
            let mut events = Events::with_capacity(64);
            let mut messages_to_write: VecDeque<ClientMessage> = VecDeque::new();
            stream.set_nonblocking(true)?;
            let mut stream = mio::net::TcpStream::from_std(stream);
            poll.registry()
                .register(&mut stream, SERVER, Interest::READABLE)?;

            loop {
                if let Err(error) = poll.poll(&mut events, Some(DEFAULT_POLL_TIMEOUT)) {
                    match error.kind() {
                        io::ErrorKind::Interrupted => continue,
                        _ => bail!(error),
                    }
                }

                for event in events.iter() {
                    match event.token() {
                        SERVER => {
                            if event.is_writable() && !messages_to_write.is_empty() {
                                while let Some(msg) = messages_to_write.pop_front() {
                                    if let Err(error) =
                                        write_prefixed::<ClientMessage, mio::net::TcpStream>(
                                            &mut stream,
                                            &msg,
                                        )
                                    {
                                        match error.kind() {
                                            // `write_prefixed` uses `write_all` under the hood, so we know
                                            // that if any of these occur, then the connection was probably
                                            // dropped at some point.
                                            io::ErrorKind::BrokenPipe
                                            | io::ErrorKind::ConnectionAborted
                                            | io::ErrorKind::ConnectionReset
                                            | io::ErrorKind::TimedOut
                                            | io::ErrorKind::UnexpectedEof => {
                                                bail!("connection dropped");
                                            }
                                            // Would block "errors" are the OS's way of saying that the
                                            // connection is not actually ready to perform this I/O operation.
                                            io::ErrorKind::WouldBlock => {
                                                // The message couldn't be sent, so we need to push it back
                                                // onto the queue so we don't accidentally forget about it.
                                                messages_to_write.push_front(msg);
                                            }
                                            // Retry writing in the case that the full message couldn't
                                            // be written. This should be infrequent.
                                            io::ErrorKind::WriteZero => {
                                                messages_to_write.push_front(msg);
                                                continue;
                                            }
                                            // Other errors we'll consider fatal.
                                            _ => bail!(error),
                                        }
                                        poll.registry().reregister(
                                            &mut stream,
                                            SERVER,
                                            Interest::READABLE,
                                        )?;
                                        break;
                                    }
                                }
                            }

                            if event.is_readable() {
                                // We can (maybe) read from the connection.
                                loop {
                                    match read_prefixed::<ServerResponse, mio::net::TcpStream>(
                                        &mut stream,
                                    ) {
                                        Ok(msg) => {
                                            tx_server.send(msg)?;
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
                                                    bail!("connection dropped");
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
                        WAKER => {
                            while let Ok(msg) = rx_client.try_recv() {
                                messages_to_write.push_back(msg);
                                poll.registry().reregister(
                                    &mut stream,
                                    SERVER,
                                    Interest::READABLE | Interest::WRITABLE,
                                )?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        let mut action_options: HashSet<Action> = HashSet::new();
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            if event::poll(POLL_TIMEOUT)? {
                if let Event::Key(KeyEvent {
                    code,
                    modifiers,
                    kind,
                    ..
                }) = event::read()?
                {
                    if kind == KeyEventKind::Press {
                        match modifiers {
                            KeyModifiers::CONTROL => match code {
                                KeyCode::Home => self.log_handle.jump_to_first(),
                                KeyCode::End => self.log_handle.jump_to_last(),
                                _ => {}
                            },
                            KeyModifiers::NONE => match code {
                                KeyCode::Enter => {
                                    let user_input = self.user_input.submit();
                                    let record = Record::new(RecordKind::User, user_input.clone());
                                    self.log_handle.push(record.into());
                                    self.handle_command(
                                        &user_input,
                                        &action_options,
                                        &view,
                                        &tx_client,
                                        &waker,
                                    )?;
                                }
                                KeyCode::Char(to_insert) => self.user_input.input(to_insert),
                                KeyCode::Backspace => self.user_input.backspace(),
                                KeyCode::Delete => self.user_input.delete(),
                                KeyCode::Left => self.user_input.move_left(),
                                KeyCode::Right => self.user_input.move_right(),
                                KeyCode::Up => self.log_handle.move_up(),
                                KeyCode::Down => self.log_handle.move_down(),
                                KeyCode::Home => self.user_input.jump_to_first(),
                                KeyCode::End => self.user_input.jump_to_last(),
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                }
            }

            if let Ok(msg) = rx_server.try_recv() {
                match msg {
                    ServerResponse::Ack(msg) => {
                        let record = Record::new(RecordKind::System, msg.to_string());
                        self.log_handle.push(record.into());
                    }
                    ServerResponse::ClientError(error) => {
                        let record = Record::new(RecordKind::Error, error.to_string());
                        self.log_handle.push(record.into());
                    }
                    ServerResponse::GameView(new_view) => view = new_view,
                    ServerResponse::Status(msg) => {
                        let record = Record::new(RecordKind::Game, msg);
                        self.log_handle.push(record.into());
                    }
                    ServerResponse::TurnSignal(new_action_options) => {
                        action_options = new_action_options;
                        let record = Record::new(RecordKind::Alert, "it's your turn!".to_string());
                        self.log_handle.push(record.into());

                        // Push the table state to the terminal so the player
                        // doesn't have to enter a new command.
                        let content = view.table_to_string();
                        self.log_handle.push_multiline_string(content);
                    }
                    ServerResponse::UserError(error) => {
                        let record = Record::new(RecordKind::Error, error.to_string());
                        self.log_handle.push(record.into());
                    }
                };
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let vertical = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ]);
        let [log_area, user_input_area, help_area] = vertical.areas(frame.area());

        // Render log window.
        let log_records = self.log_handle.list_items.clone();
        let log_records = List::new(log_records)
            .direction(ListDirection::BottomToTop)
            .block(Block::bordered());
        frame.render_stateful_widget(log_records, log_area, &mut self.log_handle.list_state);

        // Render log window scrollbar.
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .symbols(scrollbar::VERTICAL)
                .begin_symbol(None)
                .end_symbol(None),
            log_area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.log_handle.scroll_state,
        );

        // Render user input area.
        let username = self.username.clone();
        let addr = self.addr.clone();
        let user_input = Paragraph::new(self.user_input.value.as_str())
            .style(Style::default())
            .block(Block::bordered().title(format!("{username}@{addr}").light_green()));
        frame.render_widget(user_input, user_input_area);
        frame.set_cursor_position(Position::new(
            // Draw the cursor at the current position in the input field.
            // This position is can be controlled via the left and right arrow key
            user_input_area.x + self.user_input.char_idx as u16 + 1,
            // Move one line down, from the border to the input line
            user_input_area.y + 1,
        ));

        // Render user input help message.
        let help_message = vec![
            "Press ".into(),
            "Enter".bold(),
            " to record a command, enter ".into(),
            "help".bold(),
            " to view commands,".into(),
            " or enter ".into(),
            "exit".bold(),
            " to exit.".into(),
        ];
        let help_style = Style::default();
        let help_message = Text::from(Line::from(help_message)).patch_style(help_style);
        let help_message = Paragraph::new(help_message);
        frame.render_widget(help_message, help_area);
    }
}
