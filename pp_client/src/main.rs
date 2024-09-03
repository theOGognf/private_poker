use chrono::{DateTime, Utc};
use ratatui::{
    self,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Margin, Position},
    style::{Style, Stylize},
    symbols::scrollbar,
    text::{Line, Span, Text},
    widgets::{
        Block, List, ListDirection, ListItem, ListState, Paragraph, ScrollDirection, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    DefaultTerminal, Frame,
};
use std::{collections::VecDeque, fmt, io};

pub const MAX_LOG_RECORDS: usize = 1024;

struct LogHandle {
    records: VecDeque<Record>,
    list_state: ListState,
    scroll_state: ScrollbarState,
}

impl LogHandle {
    pub fn clear(&mut self) {
        self.jump_to_last();
        self.scroll_state = self.scroll_state.content_length(0);
        self.records.clear();
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
        if let Some(_) = self.list_state.selected() {
            self.scroll_state.scroll(ScrollDirection::Forward);
        }
    }

    pub fn move_up(&mut self) {
        self.list_state.scroll_down_by(1);
        if let Some(_) = self.list_state.selected() {
            self.scroll_state.scroll(ScrollDirection::Backward);
        }
    }

    pub fn new() -> Self {
        Self {
            records: VecDeque::with_capacity(MAX_LOG_RECORDS),
            list_state: ListState::default(),
            scroll_state: ScrollbarState::new(0),
        }
    }

    pub fn push(&mut self, source: RecordSource, content: String) {
        let record = Record::new(source, content);
        if self.records.len() == MAX_LOG_RECORDS {
            self.records.pop_back();
        }
        self.records.push_front(record);
        self.scroll_state = self.scroll_state.content_length(self.records.len());
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

enum RecordSource {
    SYSTEM,
    USER,
}

impl fmt::Display for RecordSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = match self {
            RecordSource::SYSTEM => "SYSTEM",
            RecordSource::USER => "USER",
        };
        write!(f, "{:6}", value)
    }
}

struct Record {
    datetime: DateTime<Utc>,
    source: RecordSource,
    content: String,
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "  [{} {}]: {}",
            self.datetime.format("%Y-%m-%d %H:%M:%S"),
            self.source,
            self.content
        )
    }
}

impl Record {
    fn new(source: RecordSource, content: String) -> Self {
        Self {
            datetime: Utc::now(),
            source,
            content,
        }
    }
}

fn main() -> io::Result<()> {
    let terminal = ratatui::init();
    let app_result = App::new().run(terminal);
    ratatui::restore();
    app_result
}

/// App holds the state of the application
struct App {
    /// History of recorded messages
    log_handle: LogHandle,
    /// Current value of the input box
    user_input: UserInput,
}

impl App {
    fn new() -> Self {
        Self {
            log_handle: LogHandle::new(),
            user_input: UserInput::new(),
        }
    }

    fn run(mut self, mut terminal: DefaultTerminal) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;

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
                            KeyCode::Char('c') => return Ok(()),
                            KeyCode::Home => self.log_handle.jump_to_first(),
                            KeyCode::End => self.log_handle.jump_to_last(),
                            _ => {}
                        },
                        KeyModifiers::NONE => match code {
                            KeyCode::Enter => {
                                let content = self.user_input.submit();
                                self.log_handle.push(RecordSource::USER, content.clone());
                                match content.as_str() {
                                    "clear" => self.log_handle.clear(),
                                    "exit" => return Ok(()),
                                    _ => {}
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
    }

    fn draw(&mut self, frame: &mut Frame) {
        let vertical = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ]);
        let [log_area, input_area, help_area] = vertical.areas(frame.area());

        let (msg, style) = (
            vec![
                "Press ".into(),
                "Enter".bold(),
                " to record a command, enter ".into(),
                "help".bold(),
                " to view commands,".into(),
                " or press ".into(),
                "CTRL+C".bold(),
                " to exit.".into(),
            ],
            Style::default(),
        );
        let text = Text::from(Line::from(msg)).patch_style(style);
        let help_message = Paragraph::new(text);
        frame.render_widget(help_message, help_area);

        let input = Paragraph::new(self.user_input.value.as_str())
            .style(Style::default())
            .block(Block::bordered().title("Input"));
        frame.render_widget(input, input_area);

        frame.set_cursor_position(Position::new(
            // Draw the cursor at the current position in the input field.
            // This position is can be controlled via the left and right arrow key
            input_area.x + self.user_input.char_idx as u16 + 1,
            // Move one line down, from the border to the input line
            input_area.y + 1,
        ));

        let items: VecDeque<ListItem> = self
            .log_handle
            .records
            .iter()
            .map(|r| {
                let content = Line::from(Span::raw(r.to_string()));
                ListItem::new(content)
            })
            .collect();
        let items = List::new(items)
            .direction(ListDirection::BottomToTop)
            .block(Block::bordered().title("Log"));
        frame.render_stateful_widget(items, log_area, &mut self.log_handle.list_state);

        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalLeft)
                .symbols(scrollbar::VERTICAL)
                .begin_symbol(None)
                .end_symbol(None),
            log_area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.log_handle.scroll_state,
        );
    }
}
