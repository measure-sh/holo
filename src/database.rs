use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use crossterm::event::{KeyCode, KeyEvent};
use tui_textarea::TextArea;

use crate::adb::Adb;
use crate::app::Action;

#[derive(Debug, Clone, PartialEq)]
pub enum ReplLine {
    Input(String),
    Output(String),
    Error(String),
}

pub struct DatabaseState {
    pub databases: Vec<String>,
    pub selected_index: usize,
    pub selected_db: Option<String>,
    pub history: Vec<ReplLine>,
    pub textarea: TextArea<'static>,
    pub query_history: Vec<String>,
    pub history_index: Option<usize>,
    pub scroll: usize,
    pub error: Option<String>,
    pub confirming_pull: Option<String>,
    pub copied_at: Option<std::time::Instant>,
    pub editing_query: bool,
}

fn new_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(ratatui::style::Style::default());
    ta
}

impl DatabaseState {
    pub fn new() -> Self {
        Self {
            databases: Vec::new(),
            selected_index: 0,
            selected_db: None,
            history: Vec::new(),
            textarea: new_textarea(),
            query_history: Vec::new(),
            history_index: None,
            scroll: 0,
            error: None,
            confirming_pull: None,
            copied_at: None,
            editing_query: false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        let code = key.code;

        if self.editing_query {
            match code {
                KeyCode::Enter => {
                    if let Some(db) = self.selected_db.clone() {
                        if let Some(sql) = self.submit_query() {
                            return Some(Action::RunQuery(db, sql));
                        }
                    }
                }
                KeyCode::Esc => self.editing_query = false,
                KeyCode::Up => self.history_up(),
                KeyCode::Down => self.history_down(),
                _ => { self.textarea.input(key); }
            }
            return Some(Action::Noop);
        }

        if self.confirming_pull.is_some() {
            return Some(match code {
                KeyCode::Char('p') => {
                    let db = self.confirming_pull.take().unwrap();
                    Action::PullDb(db)
                }
                _ => {
                    self.confirming_pull = None;
                    Action::Noop
                }
            });
        }

        match code {
            KeyCode::Up | KeyCode::Down if self.selected_db.is_none() => {
                if code == KeyCode::Up {
                    self.move_up();
                } else {
                    self.move_down();
                }
                Some(Action::Noop)
            }
            KeyCode::Char('p') if self.selected_db.is_none() => {
                if let Some(db) = self.databases.get(self.selected_index).cloned() {
                    self.confirming_pull = Some(db);
                }
                Some(Action::Noop)
            }
            KeyCode::Enter if self.selected_db.is_none() => {
                self.select_db();
                if self.selected_db.is_some() {
                    self.editing_query = true;
                }
                Some(Action::Noop)
            }
            KeyCode::Char('p') if self.selected_db.is_some() => {
                self.confirming_pull = self.selected_db.clone();
                Some(Action::Noop)
            }
            KeyCode::Char('e') if self.selected_db.is_some() => {
                self.editing_query = true;
                Some(Action::Noop)
            }
            KeyCode::Char('c') if self.selected_db.is_some() => {
                if let Some(text) = self.history_text() {
                    self.copied_at = Some(std::time::Instant::now());
                    return Some(Action::CopyDbResult(text));
                }
                Some(Action::Noop)
            }
            KeyCode::Char('r') => {
                self.reset();
                Some(Action::ResetDb)
            }
            KeyCode::Up if self.selected_db.is_some() => {
                self.move_up();
                Some(Action::Noop)
            }
            KeyCode::Down if self.selected_db.is_some() => {
                self.move_down();
                Some(Action::Noop)
            }
            KeyCode::Esc if self.selected_db.is_some() => {
                self.deselect_db();
                Some(Action::Noop)
            }
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
        }
    }

    pub fn textarea_text(&self) -> String {
        self.textarea.lines().join("")
    }

    pub fn move_up(&mut self) {
        if self.selected_db.is_some() {
            self.scroll += 1;
        } else if !self.databases.is_empty() {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_db.is_some() {
            self.scroll = self.scroll.saturating_sub(1);
        } else if !self.databases.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.databases.len() - 1);
        }
    }

    pub fn select_db(&mut self) {
        if let Some(db) = self.databases.get(self.selected_index) {
            self.selected_db = Some(db.clone());
            self.history.clear();
            self.textarea = new_textarea();
            self.query_history.clear();
            self.history_index = None;
            self.scroll = 0;
        }
    }

    pub fn deselect_db(&mut self) {
        self.selected_db = None;
        self.history.clear();
        self.textarea = new_textarea();
        self.query_history.clear();
        self.history_index = None;
        self.scroll = 0;
    }

    pub fn submit_query(&mut self) -> Option<String> {
        let sql = self.textarea_text();
        if sql.is_empty() {
            return None;
        }
        self.push_query(&sql);
        self.query_history.push(sql.clone());
        self.history_index = None;
        self.textarea = new_textarea();
        self.scroll = 0;
        Some(sql)
    }

    pub fn history_up(&mut self) {
        if self.query_history.is_empty() {
            return;
        }
        let idx = match self.history_index {
            None => self.query_history.len() - 1,
            Some(0) => return,
            Some(i) => i - 1,
        };
        self.history_index = Some(idx);
        self.set_textarea_text(&self.query_history[idx].clone());
    }

    pub fn history_down(&mut self) {
        let Some(idx) = self.history_index else { return };
        if idx + 1 >= self.query_history.len() {
            self.history_index = None;
            self.textarea = new_textarea();
        } else {
            self.history_index = Some(idx + 1);
            self.set_textarea_text(&self.query_history[idx + 1].clone());
        }
    }

    fn set_textarea_text(&mut self, text: &str) {
        self.textarea = new_textarea();
        self.textarea.insert_str(text);
    }

    pub fn push_query(&mut self, sql: &str) {
        self.history.push(ReplLine::Input(sql.to_string()));
    }

    pub fn push_result(&mut self, output: &str) {
        for line in output.lines() {
            self.history.push(ReplLine::Output(line.to_string()));
        }
    }

    pub fn push_error(&mut self, err: &str) {
        self.history.push(ReplLine::Error(err.to_string()));
    }

    pub fn clamp_scroll(&mut self, total: usize, visible: usize) {
        let max = total.saturating_sub(visible);
        self.scroll = self.scroll.min(max);
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn history_text(&self) -> Option<String> {
        if self.history.is_empty() {
            return None;
        }
        let text: Vec<String> = self.history.iter().map(|line| match line {
            ReplLine::Input(s) => format!("> {s}"),
            ReplLine::Output(s) | ReplLine::Error(s) => s.clone(),
        }).collect();
        Some(text.join("\n"))
    }
}

pub fn spawn_db_detector(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
) -> mpsc::Receiver<Result<Vec<String>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = adb
            .list_databases(&serial, &package)
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn spawn_pull_db(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    db: String,
) -> mpsc::Receiver<Result<PathBuf, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let dest = std::env::temp_dir().join("holo").join(&package).join("db").join(format!("{}_{}", timestamp, db));
        let result = std::fs::create_dir_all(&dest)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                adb.pull_database(&serial, &package, &db, &dest)
                    .map_err(|e| e.to_string())
            })
            .map(|_| dest);
        let _ = tx.send(result);
    });
    rx
}

pub fn spawn_query(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    db: String,
    sql: String,
) -> mpsc::Receiver<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = adb
            .query_database(&serial, &package, &db, &sql)
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_state_navigate_list() {
        let mut state = DatabaseState::new();
        state.databases = vec!["a.db".into(), "b.db".into(), "c.db".into()];
        assert_eq!(state.selected_index, 0);
        state.move_down();
        assert_eq!(state.selected_index, 1);
        state.move_down();
        assert_eq!(state.selected_index, 2);
        state.move_down();
        assert_eq!(state.selected_index, 2);
        state.move_up();
        assert_eq!(state.selected_index, 1);
        state.move_up();
        assert_eq!(state.selected_index, 0);
        state.move_up();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn db_state_select_and_deselect() {
        let mut state = DatabaseState::new();
        state.databases = vec!["app.db".into(), "cache.db".into()];
        state.selected_index = 1;
        state.select_db();
        assert_eq!(state.selected_db.as_deref(), Some("cache.db"));
        state.deselect_db();
        assert_eq!(state.selected_db, None);
    }

    #[test]
    fn db_state_scroll_history() {
        let mut state = DatabaseState::new();
        state.selected_db = Some("test.db".into());
        state.move_up();
        assert_eq!(state.scroll, 1);
        state.move_down();
        assert_eq!(state.scroll, 0);
        state.move_down();
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn push_query_appends_input_line() {
        let mut state = DatabaseState::new();
        state.push_query("SELECT 1");
        assert_eq!(state.history, vec![ReplLine::Input("SELECT 1".into())]);
    }

    #[test]
    fn push_result_appends_output_lines() {
        let mut state = DatabaseState::new();
        state.push_result("row1\nrow2\nrow3");
        assert_eq!(state.history, vec![
            ReplLine::Output("row1".into()),
            ReplLine::Output("row2".into()),
            ReplLine::Output("row3".into()),
        ]);
    }

    #[test]
    fn push_error_appends_error_line() {
        let mut state = DatabaseState::new();
        state.push_error("syntax error");
        assert_eq!(state.history, vec![ReplLine::Error("syntax error".into())]);
    }

    #[test]
    fn history_accumulates() {
        let mut state = DatabaseState::new();
        state.push_query("SELECT 1");
        state.push_result("1");
        state.push_query("bad sql");
        state.push_error("error near bad");
        assert_eq!(state.history.len(), 4);
        assert_eq!(state.history[0], ReplLine::Input("SELECT 1".into()));
        assert_eq!(state.history[1], ReplLine::Output("1".into()));
        assert_eq!(state.history[2], ReplLine::Input("bad sql".into()));
        assert_eq!(state.history[3], ReplLine::Error("error near bad".into()));
    }

    #[test]
    fn select_db_clears_history() {
        let mut state = DatabaseState::new();
        state.databases = vec!["a.db".into()];
        state.selected_index = 0;
        state.select_db();
        state.push_query("SELECT 1");
        state.push_result("1");
        state.select_db();
        assert!(state.history.is_empty());
    }

    #[test]
    fn reset_clears_everything() {
        let mut state = DatabaseState::new();
        state.databases = vec!["a.db".into()];
        state.select_db();
        state.push_query("SELECT 1");
        state.push_result("1");
        state.reset();
        assert!(state.databases.is_empty());
        assert!(state.selected_db.is_none());
        assert!(state.history.is_empty());
    }

    #[test]
    fn history_text_returns_full_history() {
        let mut state = DatabaseState::new();
        state.push_query("SELECT 1");
        state.push_result("1");
        state.push_query("SELECT 2");
        state.push_result("row1\nrow2");
        assert_eq!(state.history_text().as_deref(), Some("> SELECT 1\n1\n> SELECT 2\nrow1\nrow2"));
    }

    #[test]
    fn history_text_returns_none_when_empty() {
        let state = DatabaseState::new();
        assert_eq!(state.history_text(), None);
    }

    #[test]
    fn history_text_includes_errors() {
        let mut state = DatabaseState::new();
        state.push_query("bad sql");
        state.push_error("syntax error");
        assert_eq!(state.history_text().as_deref(), Some("> bad sql\nsyntax error"));
    }

    #[test]
    fn submit_query_returns_text_and_clears() {
        let mut state = DatabaseState::new();
        state.textarea.insert_str("SELECT 1");
        let sql = state.submit_query();
        assert_eq!(sql.as_deref(), Some("SELECT 1"));
        assert!(state.textarea_text().is_empty());
        assert_eq!(state.history, vec![ReplLine::Input("SELECT 1".into())]);
        assert_eq!(state.query_history, vec!["SELECT 1".to_string()]);
    }

    #[test]
    fn submit_empty_query_returns_none() {
        let mut state = DatabaseState::new();
        assert_eq!(state.submit_query(), None);
    }

    #[test]
    fn history_up_recalls_previous_query() {
        let mut state = DatabaseState::new();
        state.textarea.insert_str("SELECT 1");
        state.submit_query();
        state.textarea.insert_str("SELECT 2");
        state.submit_query();
        state.history_up();
        assert_eq!(state.textarea_text(), "SELECT 2");
        state.history_up();
        assert_eq!(state.textarea_text(), "SELECT 1");
    }

    #[test]
    fn history_up_stops_at_oldest() {
        let mut state = DatabaseState::new();
        state.textarea.insert_str("SELECT 1");
        state.submit_query();
        state.history_up();
        assert_eq!(state.textarea_text(), "SELECT 1");
        state.history_up();
        assert_eq!(state.textarea_text(), "SELECT 1");
    }

    #[test]
    fn history_down_returns_to_empty() {
        let mut state = DatabaseState::new();
        state.textarea.insert_str("SELECT 1");
        state.submit_query();
        state.history_up();
        assert_eq!(state.textarea_text(), "SELECT 1");
        state.history_down();
        assert!(state.textarea_text().is_empty());
    }

    #[test]
    fn history_down_noop_without_browsing() {
        let mut state = DatabaseState::new();
        state.textarea.insert_str("SELECT 1");
        state.submit_query();
        state.history_down();
        assert!(state.textarea_text().is_empty());
    }

    #[test]
    fn history_up_noop_when_empty() {
        let mut state = DatabaseState::new();
        state.history_up();
        assert!(state.textarea_text().is_empty());
    }

    #[test]
    fn select_db_clears_query_history() {
        let mut state = DatabaseState::new();
        state.databases = vec!["a.db".into()];
        state.select_db();
        state.textarea.insert_str("SELECT 1");
        state.submit_query();
        assert!(!state.query_history.is_empty());
        state.select_db();
        assert!(state.query_history.is_empty());
        assert!(state.history_index.is_none());
    }
}
