use anyhow::{bail, Error};
use chrono::{DateTime, Utc};
use mio::{Events, Interest, Poll, Waker};
use private_poker::{
    entities::Action,
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
    collections::VecDeque,
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
    Error,
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
            RecordKind::Error => format!("{:6}", "ERROR").light_red(),
            RecordKind::System => format!("{:6}", "SYSTEM").light_yellow(),
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
    /// History of recorded messages
    log_handle: LogHandle,
    /// Current value of the input box
    user_input: UserInput,
}

impl App {
    pub fn new(username: String, addr: String) -> Self {
        Self {
            username,
            addr,
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
                                    let cmd = self.user_input.submit();
                                    let record = Record::new(RecordKind::User, cmd.clone());
                                    self.log_handle.push(record.into());
                                    match cmd.as_str() {
                                        // "call" => {
                                        //     let msg = ClientMessage {
                                        //         username: self.username.to_string(),
                                        //         command: ClientCommand::TakeAction(Action::Call(())),
                                        //     };
                                        // }
                                        "clear" => self.log_handle.clear(),
                                        "exit" => return Ok(()),
                                        "fold" => {
                                            let msg = ClientMessage {
                                                username: self.username.clone(),
                                                command: ClientCommand::TakeAction(Action::Fold),
                                            };
                                            tx_client.send(msg)?;
                                            waker.wake()?;
                                        }
                                        "play" => {
                                            let msg = ClientMessage {
                                                username: self.username.clone(),
                                                command: ClientCommand::ChangeState(
                                                    UserState::Play,
                                                ),
                                            };
                                            tx_client.send(msg)?;
                                            waker.wake()?;
                                        }
                                        "show hand" => {
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
                                                command: ClientCommand::ChangeState(
                                                    UserState::Spectate,
                                                ),
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
                                        cmd => {
                                            let content = match cmd {
                                                "board" => view.board_to_string(),
                                                "game" => view.to_string(),
                                                "players" => view.players_to_string(),
                                                "pots" => view.pots_to_string(),
                                                "table" => view.table_to_string(),
                                                cmd => format!("unrecognized command: {cmd}"),
                                            };
                                            let text = Text::raw(content);
                                            self.log_handle.push(text.into());
                                        }
                                    }
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
                        let record = Record::new(RecordKind::System, msg);
                        self.log_handle.push(record.into());
                    }
                    ServerResponse::TurnSignal(_) => {
                        let record = Record::new(RecordKind::System, "it's your turn!".to_string());
                        self.log_handle.push(record.into());
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
        let (help_message, help_style) = (
            vec![
                "Press ".into(),
                "Enter".bold(),
                " to record a command, enter ".into(),
                "help".bold(),
                " to view commands,".into(),
                " or enter ".into(),
                "exit".bold(),
                " to exit.".into(),
            ],
            Style::default(),
        );
        let help_message = Text::from(Line::from(help_message)).patch_style(help_style);
        let help_message = Paragraph::new(help_message);
        frame.render_widget(help_message, help_area);
    }
}
