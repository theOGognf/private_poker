use std::{
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use anyhow::Error;

use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Alignment, Constraint, Flex, Layout, Position},
    style::{Style, Stylize},
    text::{Line, Text},
    widgets::{block, Cell, Clear, Padding, Paragraph, Row, Table, TableState},
    DefaultTerminal, Frame,
};

mod widgets;

use widgets::UserInput;

use crate::bot::{Bot, QLearning};

const EXIT: &str = "\
exiting will remove all bots and erase their memory.

are you sure you want to exit?

  leave             go back
(Enter)             (Esc)
";

enum PopupMenu {
    BotCreation,
    Error(String),
    Exit,
}

type Worker = (String, Sender<()>, JoinHandle<Result<(), Error>>);

fn worker(
    mut env: Bot,
    policy: Arc<Mutex<QLearning>>,
    interrupt: Receiver<()>,
) -> Result<(), Error> {
    loop {
        if interrupt.try_recv().is_ok() {
            return Ok(());
        }
        let (mut state1, mut masks1) = env.reset()?;
        loop {
            let action = {
                let mut policy = policy.lock().expect("sample lock");
                policy.sample(state1.clone(), masks1.clone())
            };
            let (state2, masks2, reward, done) = env.step(action.clone())?;
            if done {
                let mut policy = policy.lock().expect("done lock");
                policy.update_done(state1.clone(), action.clone(), reward);
                break;
            }
            {
                let mut policy = policy.lock().expect("step lock");
                policy.update_step(
                    state1.clone(),
                    action.clone(),
                    reward,
                    state2.clone(),
                    masks2.clone(),
                );
            }
            state1.clone_from(&state2);
            masks1.clone_from(&masks2);
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

            if let Event::Key(key) = event::read()? {
                match self.popup_menu {
                    Some(PopupMenu::BotCreation) => match key.code {
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
                                    self.workers.push((
                                        botname.clone(),
                                        tx_server,
                                        thread::spawn(move || worker(env, policy, rx_worker)),
                                    ));
                                    self.table_state.select(Some(self.workers.len() - 1));
                                    self.popup_menu = None;
                                }
                                Err(msg) => {
                                    self.popup_menu = Some(PopupMenu::Error(msg.to_string()))
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
                    Some(PopupMenu::Exit) => match key.code {
                        KeyCode::Enter => return Ok(()),
                        KeyCode::Esc => self.popup_menu = None,
                        _ => {}
                    },
                    None => match key.code {
                        KeyCode::Char('d') if self.table_state.selected().is_some() => {
                            if let Some(idx) = self.table_state.selected() {
                                let (_, tx_server, _) = self.workers.remove(idx);
                                tx_server.send(())?;
                            }
                        }
                        KeyCode::Char('i') => self.popup_menu = Some(PopupMenu::BotCreation),
                        KeyCode::Esc => self.popup_menu = Some(PopupMenu::Exit),
                        KeyCode::Down => self.table_state.select_next(),
                        KeyCode::Up => self.table_state.select_previous(),
                        _ => {}
                    },
                }
            }

            // Only keep workers that're doing work.
            self.workers.retain(|(.., handle)| !handle.is_finished());
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let window = Layout::vertical([Constraint::Length(1), Constraint::Min(6)]);
        let [help_area, table_area] = window.areas(frame.area());

        // Render current bots.
        let table = Table::new(
            self.workers.iter().map(|(botname, ..)| {
                let text = Text::raw(botname);
                let cell = Cell::new(text);
                Row::new([cell])
            }),
            [Constraint::Fill(1)],
        )
        .block(block::Block::bordered().padding(Padding::uniform(1)))
        .highlight_style(Style::new().bg(ratatui::style::Color::White));
        frame.render_stateful_widget(table, table_area, &mut self.table_state);

        // Render user input help message.
        let help_message = vec![
            "press ".into(),
            "i".bold().white(),
            " to create a bot, ".into(),
            "up/down".bold().white(),
            " to select a bot, ".into(),
            "d".bold().white(),
            " delete a bot, or press ".into(),
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
