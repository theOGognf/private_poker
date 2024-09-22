use anyhow::{bail, Error};
use chrono::{DateTime, Utc};
use clap::{Arg, Command};
use mio::{Events, Interest, Poll, Waker};
use private_poker::{
    entities::{Action, Card, GameView, Suit, Usd, User, Username},
    functional,
    messages::UserState,
    net::{
        messages::{ClientMessage, ServerMessage, UserCommand},
        server::{DEFAULT_POLL_TIMEOUT, SERVER, WAKER},
        utils::{read_prefixed, write_prefixed},
    },
};
use ratatui::{
    self,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Alignment, Constraint, Flex, Layout, Margin, Position},
    style::{Style, Stylize},
    symbols::scrollbar,
    text::{Line, Span, Text},
    widgets::{
        block, Block, Cell, Clear, List, ListDirection, ListItem, Padding, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, Table,
    },
    DefaultTerminal, Frame,
};
use std::{
    collections::{HashSet, VecDeque},
    io,
    net::TcpStream,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

mod widgets;

use widgets::{ScrollableList, UserInput};

pub const MAX_LOG_RECORDS: usize = 1024;
pub const POLL_TIMEOUT: Duration = Duration::from_millis(100);

fn blinds_to_string(view: &GameView) -> String {
    format!(" blinds: ${}/${}  ", view.big_blind, view.small_blind)
}

fn board_to_vec_of_spans(view: &GameView) -> Vec<Span<'_>> {
    let mut span = vec![" board: ".into()];
    // Player cards styled according to suit.
    for card_idx in 0..5 {
        let card_repr = match view.board.get(card_idx) {
            Some(card) => card_to_span(card),
            None => Span::raw(" ?/?"),
        };
        span.push(card_repr);
        span.push("  ".into());
    }
    span
}

fn card_to_span(card: &Card) -> Span<'_> {
    let Card(value, suit) = card;
    let value = match value {
        1 | 14 => "A",
        11 => "J",
        12 => "Q",
        13 => "K",
        v => &v.to_string(),
    };
    match suit {
        Suit::Club => format!("{value:>2}/c").light_green(),
        Suit::Diamond => format!("{value:>2}/d").light_blue(),
        Suit::Heart => format!("{value:>2}/h").light_red(),
        Suit::Spade => format!("{value:>2}/s").into(),
        Suit::Wild => format!("{value:>2}/w").light_magenta(),
    }
}

fn pot_to_string(view: &GameView) -> String {
    format!(" pot: {}  ", view.pot)
}

fn user_to_row(user: &User) -> Row {
    Row::new(vec![
        Cell::new(Text::from(user.name.clone()).alignment(Alignment::Left)),
        Cell::new(Text::from(format!("${}", user.money)).alignment(Alignment::Right)),
    ])
}

#[derive(Clone)]
enum RecordKind {
    Ack,
    Alert,
    Error,
    Game,
    You,
}

/// A timestamped terminal message with an importance label to help
/// direct user attention.
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
        let repr = match val.kind {
            RecordKind::Ack => "ACK".light_blue(),
            RecordKind::Alert => "ALERT".light_magenta(),
            RecordKind::Error => "ERROR".light_red(),
            RecordKind::Game => "GAME".light_yellow(),
            RecordKind::You => "YOU".light_green(),
        };

        let msg = vec![
            format!("[{} ", val.datetime.format("%H:%M:%S")).into(),
            Span::styled(format!("{repr:5}"), repr.style),
            format!("]: {}", val.content).into(),
        ];

        let content = Line::from(msg);
        ListItem::new(content)
    }
}

/// Provides turn time remaining warnings at specific intervals when it's
/// the player's turn.
struct TurnWarnings {
    t: Instant,
    idx: usize,
    warnings: [u8; 8],
}

impl TurnWarnings {
    /// Check for a new warning.
    fn check(&mut self) -> Option<u8> {
        if self.idx > 0 {
            let ceiling = self.warnings.last().expect("warnings immutable");
            let warning = self.warnings[self.idx - 1];
            let dt = Instant::now() - self.t;
            let remaining = ceiling.saturating_sub(dt.as_secs() as u8);
            if remaining <= warning {
                self.idx -= 1;
                return Some(warning);
            }
        }
        None
    }

    fn clear(&mut self) {
        self.idx = 0;
    }

    fn new() -> Self {
        Self {
            t: Instant::now(),
            idx: 0,
            warnings: [1, 2, 3, 4, 5, 10, 20, 30],
        }
    }

    fn reset(&mut self) {
        self.t = Instant::now();
        self.idx = self.warnings.len();
    }
}

/// App holds the application state.
pub struct App {
    username: Username,
    addr: String,
    commands: Command,
    /// Help menu
    help_menu_text: String,
    /// Whether to display the help menu window
    show_help_menu: bool,
    /// History of recorded messages
    log_handle: ScrollableList,
    /// Current value of the input box
    user_input: UserInput,
}

impl App {
    fn handle_command(
        &mut self,
        user_input: &str,
        action_options: &HashSet<Action>,
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
                                    command: UserCommand::TakeAction(action.clone()),
                                };
                                tx_client.send(msg)?;
                                waker.wake()?;
                            } else {
                                let record =
                                    Record::new(RecordKind::Error, "can't all-in now".to_string());
                                self.log_handle.push(record.into());
                            }
                        }
                        "call" => {
                            // Actions use their variant for comparisons,
                            // so we don't need to provide the correct call
                            // amount to see if it exists within the action
                            // options.
                            if let Some(action) = action_options.get(&Action::Call(0)) {
                                let msg = ClientMessage {
                                    username: self.username.to_string(),
                                    command: UserCommand::TakeAction(action.clone()),
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
                                    command: UserCommand::TakeAction(action.clone()),
                                };
                                tx_client.send(msg)?;
                                waker.wake()?;
                            } else {
                                let record =
                                    Record::new(RecordKind::Error, "can't check now".to_string());
                                self.log_handle.push(record.into());
                            }
                        }
                        "fold" => {
                            if let Some(action) = action_options.get(&Action::Fold) {
                                let msg = ClientMessage {
                                    username: self.username.clone(),
                                    command: UserCommand::TakeAction(action.clone()),
                                };
                                tx_client.send(msg)?;
                                waker.wake()?;
                            } else {
                                let record =
                                    Record::new(RecordKind::Error, "can't fold now".to_string());
                                self.log_handle.push(record.into());
                            }
                        }
                        "play" => {
                            let msg = ClientMessage {
                                username: self.username.clone(),
                                command: UserCommand::ChangeState(UserState::Play),
                            };
                            tx_client.send(msg)?;
                            waker.wake()?;
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
                                                command: UserCommand::TakeAction(action),
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
                                command: UserCommand::ShowHand,
                            };
                            tx_client.send(msg)?;
                            waker.wake()?;
                        }
                        "spectate" => {
                            let msg = ClientMessage {
                                username: self.username.clone(),
                                command: UserCommand::ChangeState(UserState::Spectate),
                            };
                            tx_client.send(msg)?;
                            waker.wake()?;
                        }
                        "start" => {
                            let msg = ClientMessage {
                                username: self.username.clone(),
                                command: UserCommand::StartGame,
                            };
                            tx_client.send(msg)?;
                            waker.wake()?;
                        }
                        _ => unreachable!("always a subcommand"),
                    }
                }
            }
            Err(_) => {
                let record = Record::new(
                    RecordKind::Error,
                    format!("unrecognized command: {user_input}"),
                );
                self.log_handle.push(record.into());
            }
        }
        Ok(())
    }

    pub fn new(username: Username, addr: String) -> Self {
        let all_in = Command::new("all-in").about("Go all-in, betting all your money on the hand.");
        let call = Command::new("call").about("Match the investment required to stay in the hand.");
        let check =
            Command::new("check").about("Check, voting to move to the next card reveal(s).");
        let fold = Command::new("fold").about("Fold, forfeiting your hand.");
        let play = Command::new("play").about("Join the playing waitlist.");
        let raise_about = [
            "Raise the investment required to stay in the hand. Entering without a value",
            "defaults to the min raise amount. Entering AMOUNT will raise by AMOUNT, but",
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
        let start =
            Command::new("start").about("Start the game. Requires 2+ players or waitlisters.");
        let usage = "Enter commands to interact with the poker server.";
        let commands = Command::new("poker")
            .disable_help_flag(true)
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .next_line_help(true)
            .no_binary_name(true)
            .override_usage(usage)
            .subcommand(all_in)
            .subcommand(call)
            .subcommand(check)
            .subcommand(fold)
            .subcommand(play)
            .subcommand(raise)
            .subcommand(show)
            .subcommand(spectate)
            .subcommand(start);
        let help_menu_text = commands.clone().render_help().to_string();
        Self {
            username,
            addr,
            commands,
            help_menu_text,
            show_help_menu: false,
            log_handle: ScrollableList::new(MAX_LOG_RECORDS),
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
        let (tx_server, rx_server): (Sender<ServerMessage>, Receiver<ServerMessage>) = channel();

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
                                    match read_prefixed::<ServerMessage, mio::net::TcpStream>(
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

        let mut action_options = HashSet::new();
        let mut turn_warnings = TurnWarnings::new();
        loop {
            terminal.draw(|frame| self.draw(&view, frame))?;

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
                                    let record = Record::new(RecordKind::You, user_input.clone());
                                    self.log_handle.push(record.into());
                                    self.handle_command(
                                        &user_input,
                                        &action_options,
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
                                KeyCode::Tab => self.show_help_menu = !self.show_help_menu,
                                KeyCode::Esc => return Ok(()),
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                }
            }

            if let Ok(msg) = rx_server.try_recv() {
                match msg {
                    ServerMessage::Ack(msg) => {
                        if msg.username == self.username {
                            match msg.command {
                                // Our action was acknowledged, so we don't need warnings anymore.
                                UserCommand::TakeAction(_) => {
                                    turn_warnings.clear();
                                }
                                // Our action timed-out and so the server booted us; let's exit.
                                UserCommand::Leave => return Ok(()),
                                _ => {}
                            }
                        }
                        let record = Record::new(RecordKind::Ack, msg.to_string());
                        self.log_handle.push(record.into());
                    }
                    ServerMessage::ClientError(error) => {
                        let record = Record::new(RecordKind::Error, error.to_string());
                        self.log_handle.push(record.into());
                    }
                    ServerMessage::GameView(new_view) => view = new_view,
                    ServerMessage::Status(msg) => {
                        let record = Record::new(RecordKind::Game, msg);
                        self.log_handle.push(record.into());
                    }
                    ServerMessage::TurnSignal(new_action_options) => {
                        action_options = new_action_options;
                        turn_warnings.reset();
                        let record = Record::new(RecordKind::Alert, "it's your turn!".to_string());
                        self.log_handle.push(record.into());
                    }
                    ServerMessage::UserError(error) => {
                        let record = Record::new(RecordKind::Error, error.to_string());
                        self.log_handle.push(record.into());
                    }
                };
            }

            // Signal how much time is left to the user at specific intervals.
            if let Some(warning) = turn_warnings.check() {
                let record = Record::new(RecordKind::Alert, format!("{warning:>2} second(s) left"));
                self.log_handle.push(record.into());
            }
        }
    }

    fn draw(&mut self, view: &GameView, frame: &mut Frame) {
        let window = Layout::vertical([
            Constraint::Min(6),
            Constraint::Length(3),
            Constraint::Length(1),
        ]);
        let [top_area, user_input_area, help_area] = window.areas(frame.area());
        let [view_area, log_area] =
            Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(top_area);
        let [lobby_area, table_area] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                .areas(view_area);
        let [spectator_area, waitlister_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(lobby_area);

        // Render spectators area.
        let mut spectators = Vec::from_iter(view.spectators.values());
        spectators.sort_unstable();
        let spectators = Table::new(
            spectators.iter().map(|user| user_to_row(user)),
            [Constraint::Percentage(50), Constraint::Percentage(50)],
        )
        .block(
            Block::bordered()
                .padding(Padding::uniform(1))
                .title(" spectators  "),
        );
        frame.render_widget(spectators, spectator_area);

        // Render waitlisters area.
        let waitlisters = Table::new(
            view.waitlist.iter().map(|user| user_to_row(user)),
            [Constraint::Percentage(50), Constraint::Percentage(50)],
        )
        .block(
            Block::bordered()
                .padding(Padding::uniform(1))
                .title(" waitlisters  "),
        );
        frame.render_widget(waitlisters, waitlister_area);

        // Render table area.
        let table = Table::new(
            view.players.iter().enumerate().map(|(player_idx, player)| {
                // Indicator if it's the player's move.
                let move_repr = match view.next_action_idx {
                    Some(next_action_idx) if player_idx == next_action_idx => "→",
                    _ => " ",
                };
                let move_repr = Text::from(move_repr);

                // Indicator for what blind each player pays.
                let button_repr = if player_idx == view.big_blind_idx {
                    "BB"
                } else if player_idx == view.small_blind_idx {
                    "SB"
                } else {
                    "  "
                };
                let button_repr = Text::from(button_repr);

                // Username column.
                let username_repr = player.user.name.clone();
                let username_repr = Text::from(username_repr);

                // Money column.
                let money_repr = format!("${}", player.user.money);
                let money_repr = Text::from(money_repr);

                // State column.
                let state_repr = player.state.to_string();
                let state_repr = Text::from(state_repr);

                // This is the final row representation for the table entry.
                let mut row = vec![
                    Cell::new(move_repr.alignment(Alignment::Center)),
                    Cell::new(button_repr.alignment(Alignment::Left)),
                    Cell::new(username_repr.alignment(Alignment::Left)),
                    Cell::new(money_repr.alignment(Alignment::Right)),
                    Cell::new(state_repr.alignment(Alignment::Center)),
                ];

                // Player cards styled according to suit.
                for card_idx in 0..2 {
                    let card_repr = match player.cards.get(card_idx) {
                        Some(card) => Text::from(card_to_span(card)),
                        None => Text::from(" ?/?"),
                    };
                    let card_cell = Cell::new(card_repr.alignment(Alignment::Right));
                    row.push(card_cell);
                }

                // Player's highest subhand displayed.
                let hand_repr = if player.cards.is_empty() {
                    "??"
                } else {
                    let mut cards = view.board.clone();
                    cards.extend(player.cards.clone());
                    functional::prepare_hand(&mut cards);
                    let hand = functional::eval(&cards);
                    if let Some(subhand) = hand.first() {
                        &subhand.rank.to_string()
                    } else {
                        "??"
                    }
                };
                let hand_repr = format!("({hand_repr})");
                let hand_repr = Text::from(hand_repr).alignment(Alignment::Right);
                let hand_cell = Cell::new(hand_repr);
                row.push(hand_cell);

                Row::new(row)
            }),
            [
                Constraint::Max(3),
                Constraint::Fill(1),
                Constraint::Fill(2),
                Constraint::Fill(2),
                Constraint::Fill(2),
                Constraint::Fill(1),
                Constraint::Fill(1),
                Constraint::Fill(1),
            ],
        )
        .block(
            block::Block::bordered()
                .padding(Padding::uniform(1))
                .title(
                    block::Title::from(board_to_vec_of_spans(view))
                        .position(block::Position::Top)
                        .alignment(Alignment::Left),
                )
                .title(
                    block::Title::from(blinds_to_string(view))
                        .position(block::Position::Bottom)
                        .alignment(Alignment::Right),
                )
                .title(
                    block::Title::from(pot_to_string(view))
                        .position(block::Position::Bottom)
                        .alignment(Alignment::Left),
                ),
        );
        frame.render_widget(table, table_area);

        // Render log window.
        let log_records = self.log_handle.list_items.clone();
        let log_records = List::new(log_records)
            .direction(ListDirection::BottomToTop)
            .block(block::Block::bordered().title(" history  "));
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
            .block(block::Block::bordered().title(format!(" {username}@{addr}  ").light_green()));
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
            "press ".into(),
            "Tab".bold(),
            " to view help, press ".into(),
            "Enter".bold(),
            " to record a command, or press ".into(),
            "Esc".bold(),
            " to exit".into(),
        ];
        let help_style = Style::default();
        let help_message = Text::from(Line::from(help_message)).patch_style(help_style);
        let help_message = Paragraph::new(help_message);
        frame.render_widget(help_message, help_area);

        // Render the help menu.
        if self.show_help_menu {
            let vertical = Layout::vertical([Constraint::Max(25)]).flex(Flex::Center);
            let horizontal = Layout::horizontal([Constraint::Max(95)]).flex(Flex::Center);
            let [help_menu_area] = vertical.areas(frame.area());
            let [help_menu_area] = horizontal.areas(help_menu_area);
            frame.render_widget(Clear, help_menu_area); // clears out the background

            // Render help text.
            let help_text = Paragraph::new(self.help_menu_text.clone())
                .style(Style::default())
                .block(block::Block::bordered().padding(Padding::uniform(1)));
            frame.render_widget(help_text, help_menu_area);
        }
    }
}
