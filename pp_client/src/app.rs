use anyhow::{bail, Error};
use chrono::{DateTime, Utc};
use mio::{Events, Interest, Poll, Waker};
use private_poker::{
    entities::{
        Action, ActionChoice, ActionChoices, Card, GameView, Suit, Usd, User, Username, Vote,
    },
    functional,
    messages::UserState,
    net::{
        messages::{ClientMessage, ServerMessage, UserCommand},
        server::{DEFAULT_POLL_TIMEOUT, SERVER, WAKER},
        utils::{read_prefixed, write_prefixed},
    },
};
use ratatui::{
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

const HELP: &str = "\
all-in
        Go all-in, betting all your money on the hand.                                 
call
        Match the investment required to stay in the hand.                             
check
        Check, voting to move to the next card reveal(s).                              
fold
        Fold, forfeiting your hand.                                                    
play
        Join the playing waitlist.                                                     
raise
        Raise the investment required to stay in the hand. Entering without a value    
        defaults to the min raise amount. Entering AMOUNT will raise by AMOUNT, but    
        AMOUNT must be >= the min raise.                                               
show
        Show your hand. Only possible during the showdown.                             
spectate
        Join spectators. If you're a player, you won't spectate until the game is over.
start
        Start the game. Requires 2+ players or waitlisters.
vote kick USER
        Vote to kick a user from the game. The vote will pass when a majority is
        reached.
vote reset
        Vote to reset game money. Entering without a value defaults to voting to
        reset everyone's money. Entering USER will vote to reset that specific
        user's money.
";
const INVALID_RAISE_MESSAGE: &str = "invalid raise amount";
const MAX_LOG_RECORDS: usize = 1024;
const POLL_TIMEOUT: Duration = Duration::from_millis(100);
const UNRECOGNIZED_COMMAND_MESSAGE: &str = "unrecognized command";

fn blinds_to_string(view: &GameView) -> String {
    format!(" blinds: ${}/{}  ", view.blinds.big, view.blinds.small)
}

fn board_to_vec_of_spans(view: &GameView) -> Vec<Span<'_>> {
    if view.board.is_empty() {
        return vec![];
    }
    std::iter::once(" board: ".into())
        .chain(
            view.board
                .iter()
                .flat_map(|card| vec![card_to_span(card), "  ".into()]),
        )
        .collect()
}

fn card_to_span(card: &Card) -> Span<'_> {
    let Card(value, suit) = card;
    let value = match value {
        1 | 14 => "A".to_string(),
        11 => "J".to_string(),
        12 => "Q".to_string(),
        13 => "K".to_string(),
        v => v.to_string(),
    };
    let padded_value = format!("{value:>2}");
    match suit {
        Suit::Club => format!("{padded_value}/c").light_green(),
        Suit::Diamond => format!("{padded_value}/d").light_blue(),
        Suit::Heart => format!("{padded_value}/h").light_red(),
        Suit::Spade => format!("{padded_value}/s").into(),
        Suit::Wild => format!("{padded_value}/w").light_magenta(),
    }
}

fn pot_to_string(view: &GameView) -> String {
    format!(" pot: {}  ", view.pot)
}

fn user_to_row(username: &str, user: &User) -> Row<'static> {
    let mut row = Row::new(vec![
        Cell::new(Text::from(user.name.clone()).alignment(Alignment::Left)),
        Cell::new(Text::from(format!("${}", user.money)).alignment(Alignment::Right)),
    ]);

    if username == user.name {
        row = row.bold().white();
    }

    row
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
            let ceiling = self.warnings.last().expect("warnings should be immutable");
            let warning = self.warnings[self.idx - 1];
            let dt = self.t.elapsed();
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
    /// Whether to display the help menu window
    show_help_menu: bool,
    /// Helps scroll through the help menu window if the terminal is small
    help_handle: ScrollableList,
    /// History of recorded messages
    log_handle: ScrollableList,
    /// Current value of the input box
    user_input: UserInput,
}

impl App {
    fn handle_command(
        &mut self,
        user_input: &str,
        action_choices: &ActionChoices,
        tx_client: &Sender<ClientMessage>,
        waker: &Waker,
    ) -> Result<(), Error> {
        let result = match user_input.trim() {
            "all-in" => Ok(UserCommand::TakeAction(Action::AllIn)),
            "call" => Ok(UserCommand::TakeAction(Action::Call)),
            "check" => Ok(UserCommand::TakeAction(Action::Check)),
            "fold" => Ok(UserCommand::TakeAction(Action::Fold)),
            "play" => Ok(UserCommand::ChangeState(UserState::Play)),
            "show" => Ok(UserCommand::ShowHand),
            "spectate" => Ok(UserCommand::ChangeState(UserState::Spectate)),
            "start" => Ok(UserCommand::StartGame),
            other => {
                let other: Vec<&str> = other.split_ascii_whitespace().collect();
                match other.first() {
                    Some(&"raise") => {
                        let result =
                            match (action_choices.get(&ActionChoice::Raise(0)), other.get(1)) {
                                // Raise with a specific amount.
                                (None | Some(_), Some(value)) => match value.parse::<Usd>() {
                                    Ok(amount) => Ok(Action::Raise(amount)),
                                    Err(_) => Err(INVALID_RAISE_MESSAGE.to_string()),
                                },
                                // Valid raise without specified amount defaults to the default raise.
                                (Some(action_choice), None) => Ok(action_choice.clone().into()),
                                // Invalid action.
                                (None, ..) => Err(INVALID_RAISE_MESSAGE.to_string()),
                            };
                        result.map(UserCommand::TakeAction)
                    }
                    Some(&"vote") => match (other.get(1), other.get(2)) {
                        (Some(&"kick"), Some(username)) => {
                            Ok(UserCommand::CastVote(Vote::Kick(username.to_string())))
                        }
                        (Some(&"reset"), Some(username)) => Ok(UserCommand::CastVote(Vote::Reset(
                            Some(username.to_string()),
                        ))),
                        (Some(&"reset"), None) => Ok(UserCommand::CastVote(Vote::Reset(None))),
                        _ => Err(UNRECOGNIZED_COMMAND_MESSAGE.to_string()),
                    },
                    _ => Err(UNRECOGNIZED_COMMAND_MESSAGE.to_string()),
                }
            }
        };
        match result {
            Ok(command) => {
                let msg = ClientMessage {
                    username: self.username.to_string(),
                    command,
                };
                tx_client.send(msg)?;
                waker.wake()?;
            }
            Err(message) => {
                let record = Record::new(RecordKind::Error, message);
                self.log_handle.push(record.into());
            }
        }
        Ok(())
    }

    pub fn new(username: Username, addr: String) -> Result<Self, Error> {
        // Fill help menu with help text lines. Also add some whitespace as
        // a jank way to add padding on the top and bottom.
        let mut help_handle = ScrollableList::new(MAX_LOG_RECORDS);
        help_handle.push("".into());
        for line in HELP.lines() {
            help_handle.push(line.into());
        }
        help_handle.push("".into());
        help_handle.jump_to_first();
        Ok(Self {
            username,
            addr,
            show_help_menu: false,
            help_handle,
            log_handle: ScrollableList::new(MAX_LOG_RECORDS),
            user_input: UserInput::new(),
        })
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

                for event in &events {
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

        let mut action_choices = HashSet::new();
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
                                        &action_choices,
                                        &tx_client,
                                        &waker,
                                    )?;
                                }
                                KeyCode::Char(to_insert) => self.user_input.input(to_insert),
                                KeyCode::Backspace => self.user_input.backspace(),
                                KeyCode::Delete => self.user_input.delete(),
                                KeyCode::Left => self.user_input.move_left(),
                                KeyCode::Right => self.user_input.move_right(),
                                KeyCode::Up => {
                                    if self.show_help_menu {
                                        self.help_handle.move_up()
                                    } else {
                                        self.log_handle.move_up()
                                    }
                                }
                                KeyCode::Down => {
                                    if self.show_help_menu {
                                        self.help_handle.move_down()
                                    } else {
                                        self.log_handle.move_down()
                                    }
                                }
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
                let result = match msg {
                    ServerMessage::Ack(msg) => {
                        if msg.username == self.username {
                            match msg.command {
                                // Our action was acknowledged, so we don't need warnings anymore.
                                UserCommand::TakeAction(_) => {
                                    turn_warnings.clear();
                                }
                                // Our action timed-out and so the server booted us; let's exit.
                                UserCommand::Disconnect => return Ok(()),
                                _ => {}
                            }
                        }
                        Some(Record::new(RecordKind::Ack, msg.to_string()))
                    }
                    ServerMessage::ClientError(error) => {
                        Some(Record::new(RecordKind::Error, error.to_string()))
                    }
                    ServerMessage::GameEvent(event) => {
                        Some(Record::new(RecordKind::Game, event.to_string()))
                    }
                    ServerMessage::GameView(new_view) => {
                        view = new_view;
                        None
                    }
                    ServerMessage::Status(msg) => Some(Record::new(RecordKind::Game, msg)),
                    ServerMessage::TurnSignal(new_action_choices) => {
                        action_choices = new_action_choices;
                        turn_warnings.reset();
                        Some(Record::new(
                            RecordKind::Alert,
                            "it's your turn!".to_string(),
                        ))
                    }
                    ServerMessage::UserError(error) => {
                        Some(Record::new(RecordKind::Error, error.to_string()))
                    }
                };
                if let Some(record) = result {
                    self.log_handle.push(record.into());
                }
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
        let mut spectators = Vec::from_iter(view.spectators.iter());
        spectators.sort_unstable();
        let spectators = Table::new(
            spectators
                .iter()
                .map(|user| user_to_row(&self.username, user)),
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
            view.waitlist
                .iter()
                .map(|user| user_to_row(&self.username, user)),
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
                let move_repr = match view.play_positions.next_action_idx {
                    Some(next_action_idx) if player_idx == next_action_idx => "→",
                    _ => "",
                };
                let move_repr = Text::from(move_repr);

                // Indicator for what blind each player pays.
                let button_repr = if player_idx == view.play_positions.big_blind_idx {
                    "BB"
                } else if player_idx == view.play_positions.small_blind_idx {
                    "SB"
                } else {
                    ""
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
                        None => Text::from(""),
                    };
                    let card_cell = Cell::new(card_repr.alignment(Alignment::Right));
                    row.push(card_cell);
                }

                // Player's highest subhand displayed.
                let hand_repr = if player.cards.is_empty() {
                    String::new()
                } else {
                    let mut cards = view.board.clone();
                    cards.extend(player.cards.clone());
                    functional::prepare_hand(&mut cards);
                    let hand = functional::eval(&cards);
                    if let Some(subhand) = hand.first() {
                        format!("({})", subhand.rank)
                    } else {
                        String::new()
                    }
                };
                let hand_repr = Text::from(hand_repr).alignment(Alignment::Right);
                let hand_cell = Cell::new(hand_repr);
                row.push(hand_cell);

                let mut row = Row::new(row);
                if self.username == player.user.name {
                    row = row.bold().white();
                }
                row
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
            "Tab".bold().white(),
            " to view help, press ".into(),
            "Enter".bold().white(),
            " to record a command, or press ".into(),
            "Esc".bold().white(),
            " to exit".into(),
        ];
        let help_style = Style::default();
        let help_message = Text::from(Line::from(help_message)).patch_style(help_style);
        let help_message = Paragraph::new(help_message);
        frame.render_widget(help_message, help_area);

        // Render the help menu.
        if self.show_help_menu {
            let vertical = Layout::vertical([Constraint::Max(29)]).flex(Flex::Center);
            let horizontal = Layout::horizontal([Constraint::Max(92)]).flex(Flex::Center);
            let [help_menu_area] = vertical.areas(frame.area());
            let [help_menu_area] = horizontal.areas(help_menu_area);
            frame.render_widget(Clear, help_menu_area); // clears out the background

            // Render help text.
            let help_items = self.help_handle.list_items.clone();
            let help_items = List::new(help_items)
                .direction(ListDirection::BottomToTop)
                .block(block::Block::bordered().title(" commands  "));
            frame.render_stateful_widget(
                help_items,
                help_menu_area,
                &mut self.help_handle.list_state,
            );

            // Render help scrollbar.
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .symbols(scrollbar::VERTICAL)
                    .begin_symbol(None)
                    .end_symbol(None),
                help_menu_area.inner(Margin {
                    vertical: 1,
                    horizontal: 1,
                }),
                &mut self.help_handle.scroll_state,
            );
        }
    }
}
