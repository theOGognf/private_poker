use std::{
    fmt::Display,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::Error;

use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Alignment, Constraint, Flex, Layout, Position},
    style::{Style, Stylize},
    text::{Line, Text},
    widgets::{block, Cell, Clear, Padding, Paragraph, Row, Table, TableState},
    DefaultTerminal, Frame,
};

mod widgets;

use widgets::UserInput;

use super::bot::{Bot, QLearning};

const EXIT: &str = "\
exiting will remove all bots and erase their memory.

are you sure you want to exit?

  leave             go back
(Enter)             (Esc)
";
const POLL_TIMEOUT: Duration = Duration::from_millis(100);

enum PopupMenu {
    BotCreation,
    Error(String),
    Exit,
}

enum WorkerState {
    Active,
    Deleted,
}

impl Display for WorkerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let repr = match self {
            WorkerState::Active => "active",
            WorkerState::Deleted => "to be deleted after next game",
        };
        write!(f, "{repr}")
    }
}

struct Worker {
    botname: String,
    state: WorkerState,
    handle: JoinHandle<Result<(), Error>>,
    delete_signaler: Sender<()>,
}

fn worker(
    mut env: Bot,
    policy: &Arc<Mutex<QLearning>>,
    interrupt: &Receiver<()>,
) -> Result<(), Error> {
    loop {
        let (mut state1, mut masks1) = env.reset()?;
        loop {
            let action_choice = {
                let mut policy = policy.lock().expect("sample lock");
                policy.sample(state1.clone(), &masks1)
            };
            let (state2, masks2, reward, done) = env.step(&action_choice)?;
            if done {
                let mut policy = policy.lock().expect("done lock");
                policy.update_done(state1.clone(), action_choice.clone(), reward);
                break;
            }
            {
                let mut policy = policy.lock().expect("step lock");
                policy.update_step(
                    state1.clone(),
                    action_choice.clone(),
                    reward,
                    state2.clone(),
                    masks2.clone(),
                );
            }
            state1.clone_from(&state2);
            masks1.clone_from(&masks2);
        }
        if interrupt.try_recv().is_ok() {
            return Ok(());
        }
    }
}

pub struct App {
    addr: String,
    policy: Arc<Mutex<QLearning>>,
    workers: Vec<Worker>,
    table_state: TableState,
    user_input: UserInput,
    popup_menu: Option<PopupMenu>,
}

impl App {
    pub fn new(addr: String, policy: Arc<Mutex<QLearning>>) -> Self {
        Self {
            addr,
            policy,
            workers: Vec::new(),
            table_state: TableState::new(),
            user_input: UserInput::new(),
            popup_menu: None,
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<(), Error> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            if event::poll(POLL_TIMEOUT)? {
                if let Event::Key(KeyEvent { code, kind, .. }) = event::read()? {
                    if kind == KeyEventKind::Press {
                        match self.popup_menu {
                            Some(PopupMenu::BotCreation) => match code {
                                KeyCode::Char(to_insert) => self.user_input.input(to_insert),
                                KeyCode::Delete => self.user_input.delete(),
                                KeyCode::Backspace => self.user_input.backspace(),
                                KeyCode::Left => self.user_input.move_left(),
                                KeyCode::Right => self.user_input.move_right(),
                                KeyCode::Home => self.user_input.jump_to_first(),
                                KeyCode::End => self.user_input.jump_to_last(),
                                KeyCode::Enter if !self.user_input.value.is_empty() => {
                                    let botname = self.user_input.submit();
                                    let addr = self.addr.clone();
                                    match Bot::new(&botname, &addr) {
                                        Ok(env) => {
                                            let policy = self.policy.clone();
                                            let (tx_server, rx_worker): (Sender<()>, Receiver<()>) =
                                                channel();
                                            let worker = Worker {
                                                botname: botname.clone(),
                                                state: WorkerState::Active,
                                                handle: thread::spawn(move || {
                                                    worker(env, &policy, &rx_worker)
                                                }),
                                                delete_signaler: tx_server,
                                            };
                                            self.workers.push(worker);
                                            self.table_state.select(Some(self.workers.len() - 1));
                                            self.popup_menu = None;
                                        }
                                        Err(msg) => {
                                            self.popup_menu =
                                                Some(PopupMenu::Error(msg.to_string()));
                                        }
                                    }
                                }
                                KeyCode::Esc => {
                                    self.user_input.clear();
                                    self.popup_menu = None;
                                }
                                _ => {}
                            },
                            Some(PopupMenu::Error(_)) => self.popup_menu = None,
                            Some(PopupMenu::Exit) => match code {
                                KeyCode::Enter => return Ok(()),
                                KeyCode::Esc => self.popup_menu = None,
                                _ => {}
                            },
                            None => match code {
                                KeyCode::Char('d') if self.table_state.selected().is_some() => {
                                    if let Some(idx) = self.table_state.selected() {
                                        let worker =
                                            self.workers.get_mut(idx).expect("worker should exist");
                                        worker.state = WorkerState::Deleted;
                                        worker.delete_signaler.send(())?;
                                    }
                                }
                                KeyCode::Char('i') => {
                                    self.popup_menu = Some(PopupMenu::BotCreation);
                                }
                                KeyCode::Esc => self.popup_menu = Some(PopupMenu::Exit),
                                KeyCode::Down => self.table_state.select_next(),
                                KeyCode::Up => self.table_state.select_previous(),
                                _ => {}
                            },
                        }
                    }
                }
            }

            // Only keep workers that're doing work.
            self.workers.retain(|w| !w.handle.is_finished());
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let window = Layout::vertical([Constraint::Length(1), Constraint::Min(6)]);
        let [help_area, table_area] = window.areas(frame.area());

        // Render current bots.
        let table = Table::new(
            self.workers.iter().map(|w| {
                let name_text = Text::raw(w.botname.clone());
                let name_cell = Cell::new(name_text);

                let state_text = Text::from(w.state.to_string());
                let state_cell = Cell::new(state_text);
                Row::new([name_cell, state_cell])
            }),
            [Constraint::Fill(1), Constraint::Fill(1)],
        )
        .block(block::Block::bordered().padding(Padding::uniform(1)))
        .highlight_style(Style::new().bg(ratatui::style::Color::White));
        frame.render_stateful_widget(table, table_area, &mut self.table_state);

        // Render user input help message.
        let help_message = vec![
            "press ".into(),
            "i".bold().white(),
            " to create a bot, ".into(),
            "↑".bold().white(),
            "/".into(),
            "↓".bold().white(),
            " to select a bot, ".into(),
            "d".bold().white(),
            " to delete a bot, or press ".into(),
            "Esc".bold().white(),
            " to exit".into(),
        ];
        let help_style = Style::default();
        let help_message = Text::from(Line::from(help_message)).patch_style(help_style);
        let help_message = Paragraph::new(help_message);
        frame.render_widget(help_message, help_area);

        // Render popup menus.
        match self.popup_menu {
            Some(PopupMenu::BotCreation) => {
                let vertical = Layout::vertical([Constraint::Length(3)]).flex(Flex::Center);
                let horizontal = Layout::horizontal([Constraint::Max(60)]).flex(Flex::Center);
                let [user_input_area] = vertical.areas(frame.area());
                let [user_input_area] = horizontal.areas(user_input_area);
                frame.render_widget(Clear, user_input_area); // clears out the background

                let user_input = Paragraph::new(self.user_input.value.as_str())
                    .style(Style::default())
                    .block(block::Block::bordered().title(" create a new bot  "));
                frame.render_widget(user_input, user_input_area);
                frame.set_cursor_position(Position::new(
                    // Draw the cursor at the current position in the input field.
                    // This position is can be controlled via the left and right arrow key
                    user_input_area.x + self.user_input.char_idx as u16 + 1,
                    // Move one line down, from the border to the input line
                    user_input_area.y + 1,
                ));
            }
            Some(PopupMenu::Error(ref msg)) => {
                let vertical = Layout::vertical([Constraint::Max(8)]).flex(Flex::Center);
                let horizontal = Layout::horizontal([Constraint::Max(60)]).flex(Flex::Center);
                let [error_menu_area] = vertical.areas(frame.area());
                let [error_menu_area] = horizontal.areas(error_menu_area);
                frame.render_widget(Clear, error_menu_area); // clears out the background

                // Render error text.
                let error_text =
                    Paragraph::new(format!("{}\n\n\npress any key to continue", msg.clone()))
                        .style(Style::default())
                        .block(
                            block::Block::bordered()
                                .padding(Padding::uniform(1))
                                .title(" error  "),
                        )
                        .alignment(Alignment::Center);
                frame.render_widget(error_text, error_menu_area);
            }
            Some(PopupMenu::Exit) => {
                let vertical = Layout::vertical([Constraint::Max(10)]).flex(Flex::Center);
                let horizontal = Layout::horizontal([Constraint::Max(60)]).flex(Flex::Center);
                let [exit_menu_area] = vertical.areas(frame.area());
                let [exit_menu_area] = horizontal.areas(exit_menu_area);
                frame.render_widget(Clear, exit_menu_area); // clears out the background

                // Render exit text.
                let exit_text = Paragraph::new(EXIT)
                    .style(Style::default())
                    .block(block::Block::bordered().padding(Padding::uniform(1)))
                    .alignment(Alignment::Center);
                frame.render_widget(exit_text, exit_menu_area);
            }
            None => {}
        }
    }
}
