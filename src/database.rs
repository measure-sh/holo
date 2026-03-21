use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use crate::adb::Adb;

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
    pub input: String,
    pub scroll: usize,
    pub error: Option<String>,
}

impl DatabaseState {
    pub fn new() -> Self {
        Self {
            databases: Vec::new(),
            selected_index: 0,
            selected_db: None,
            history: Vec::new(),
            input: String::new(),
            scroll: 0,
            error: None,
        }
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
            self.input.clear();
            self.scroll = 0;
        }
    }

    pub fn deselect_db(&mut self) {
        self.selected_db = None;
        self.history.clear();
        self.input.clear();
        self.scroll = 0;
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
        let dest = PathBuf::from(format!("tmp/msh/{}/db/{}_{}/", package, timestamp, db));
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
}
