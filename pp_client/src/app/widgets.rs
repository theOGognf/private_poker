use ratatui::{
    self,
    widgets::{ListItem, ListState, ScrollDirection, ScrollbarState},
};

use std::collections::VecDeque;

use private_poker::constants::MAX_USER_INPUT_LENGTH;

/// Manages terminal messages and the terminal view position.
pub struct ScrollableList {
    max_items: usize,
    pub list_items: VecDeque<ListItem<'static>>,
    pub list_state: ListState,
    pub scroll_state: ScrollbarState,
}

impl ScrollableList {
    pub fn jump_to_first(&mut self) {
        self.list_state.scroll_down_by(self.max_items as u16);
        self.scroll_state.first();
    }

    pub fn jump_to_last(&mut self) {
        self.list_state.scroll_up_by(self.max_items as u16);
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

    pub fn new(max_items: usize) -> Self {
        Self {
            max_items,
            list_items: VecDeque::with_capacity(max_items),
            list_state: ListState::default(),
            scroll_state: ScrollbarState::new(0),
        }
    }

    pub fn push(&mut self, item: ListItem<'static>) {
        if self.list_items.len() == self.max_items {
            self.list_items.pop_back();
        }
        self.list_items.push_front(item);
        self.scroll_state = self.scroll_state.content_length(self.list_items.len());
        self.move_down();
    }
}

/// Manages user inputs at the terminal.
pub struct UserInput {
    /// Position of cursor in the input box.
    pub char_idx: usize,
    /// Current value of the input box.
    pub value: String,
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
        // Username length is about the same size as the largest allowed
        if self.value.len() < MAX_USER_INPUT_LENGTH {
            let idx = self.byte_idx();
            self.value.insert(idx, new_char);
            self.move_right();
        }
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
