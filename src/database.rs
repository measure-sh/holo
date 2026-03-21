use std::sync::{mpsc, Arc};

use crate::adb::Adb;

pub struct DatabaseState {
    pub databases: Vec<String>,
    pub selected_index: usize,
    pub selected_db: Option<String>,
    pub query_input: String,
    pub query_result: Option<Result<String, String>>,
    pub result_scroll: usize,
    pub error: Option<String>,
}

impl DatabaseState {
    pub fn new() -> Self {
        Self {
            databases: Vec::new(),
            selected_index: 0,
            selected_db: None,
            query_input: String::new(),
            query_result: None,
            result_scroll: 0,
            error: None,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_db.is_some() {
            self.result_scroll += 1;
        } else if !self.databases.is_empty() {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_db.is_some() {
            self.result_scroll = self.result_scroll.saturating_sub(1);
        } else if !self.databases.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.databases.len() - 1);
        }
    }

    pub fn select_db(&mut self) {
        if let Some(db) = self.databases.get(self.selected_index) {
            self.selected_db = Some(db.clone());
            self.query_input.clear();
            self.query_result = None;
            self.result_scroll = 0;
        }
    }

    pub fn deselect_db(&mut self) {
        self.selected_db = None;
        self.query_input.clear();
        self.query_result = None;
        self.result_scroll = 0;
    }

    pub fn result_lines(&self) -> Vec<Vec<String>> {
        match &self.query_result {
            Some(Ok(raw)) => parse_query_result(raw),
            _ => Vec::new(),
        }
    }

    pub fn clamp_result_scroll(&mut self, total: usize, visible: usize) {
        let max = total.saturating_sub(visible);
        self.result_scroll = self.result_scroll.min(max);
    }
}

pub fn parse_query_result(raw: &str) -> Vec<Vec<String>> {
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| line.split('|').map(|s| s.to_string()).collect())
        .collect()
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
    fn parse_query_result_basic() {
        let raw = "id|name|age\n1|Alice|30\n2|Bob|25\n";
        let rows = parse_query_result(raw);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["id", "name", "age"]);
        assert_eq!(rows[1], vec!["1", "Alice", "30"]);
        assert_eq!(rows[2], vec!["2", "Bob", "25"]);
    }

    #[test]
    fn parse_query_result_empty() {
        assert!(parse_query_result("").is_empty());
        assert!(parse_query_result("\n\n").is_empty());
    }

    #[test]
    fn parse_query_result_single_column() {
        let raw = "count(*)\n42\n";
        let rows = parse_query_result(raw);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["count(*)"]);
        assert_eq!(rows[1], vec!["42"]);
    }

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
    fn db_state_scroll_results() {
        let mut state = DatabaseState::new();
        state.selected_db = Some("test.db".into());
        state.move_up();
        assert_eq!(state.result_scroll, 1);
        state.move_down();
        assert_eq!(state.result_scroll, 0);
        state.move_down();
        assert_eq!(state.result_scroll, 0);
    }
}
