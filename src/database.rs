use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use tui_textarea::TextArea;

use crate::adb::Adb;
use crate::app::Action;

pub const TABLE_PAGE_SIZE: usize = 50;
pub const AUTO_REFRESH: Duration = Duration::from_secs(1);
const PREFETCH_MARGIN: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub enum ReplLine {
    Input(String),
    Output(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TreeNode {
    Database { name: String, expanded: bool },
    Table { db: String, name: String },
    Loading { db: String },
    NoTables { db: String },
}

#[derive(Debug, Clone)]
pub struct TableData {
    pub db: String,
    pub table: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub offset: usize,
}

/// Tells the fetch worker which slice of the table to return.
#[derive(Debug, Clone, Copy)]
pub enum FetchKind {
    /// The last `TABLE_PAGE_SIZE` rows (tail). Worker queries `COUNT(*)` first.
    Tail,
    /// A specific offset chosen by the caller (used for loading older rows).
    At(usize),
}

pub struct DatabaseState {
    pub databases: Vec<String>,
    pub error: Option<String>,

    pub expanded: HashSet<String>,
    pub tables: HashMap<String, Vec<String>>,
    pub tables_loading: HashSet<String>,
    pub tree_cursor: usize,

    pub detail_open: bool,
    pub detail_focused: bool,
    pub detail_scroll: usize,
    pub detail_h_scroll: usize,
    pub detail_visible_rows: usize,
    pub selected_table: Option<(String, String)>,
    pub table_data: Option<TableData>,
    pub table_loading: bool,
    pub table_error: Option<String>,
    pub last_refresh: Option<Instant>,
    pub panel_visible: bool,

    pub repl_active: bool,
    pub repl_db: Option<String>,
    pub history: Vec<ReplLine>,
    pub textarea: TextArea<'static>,
    pub query_history: Vec<String>,
    pub history_index: Option<usize>,
    pub repl_scroll: usize,
    pub editing_query: bool,

    pub confirming_pull: Option<String>,
    pub copied_at: Option<Instant>,
}

fn new_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(ratatui::style::Style::default());
    ta
}

fn is_at_table_tail(data: &TableData) -> bool {
    data.offset + data.rows.len() >= data.row_count
}

impl DatabaseState {
    pub fn new() -> Self {
        Self {
            databases: Vec::new(),
            error: None,
            expanded: HashSet::new(),
            tables: HashMap::new(),
            tables_loading: HashSet::new(),
            tree_cursor: 0,
            detail_open: false,
            detail_focused: false,
            detail_scroll: 0,
            detail_h_scroll: 0,
            detail_visible_rows: 0,
            selected_table: None,
            table_data: None,
            table_loading: false,
            table_error: None,
            last_refresh: None,
            panel_visible: true,
            repl_active: false,
            repl_db: None,
            history: Vec::new(),
            textarea: new_textarea(),
            query_history: Vec::new(),
            history_index: None,
            repl_scroll: 0,
            editing_query: false,
            confirming_pull: None,
            copied_at: None,
        }
    }

    pub fn flatten_tree(&self) -> Vec<TreeNode> {
        let mut out = Vec::new();
        for db in &self.databases {
            let expanded = self.expanded.contains(db);
            out.push(TreeNode::Database { name: db.clone(), expanded });
            if !expanded {
                continue;
            }
            if self.tables_loading.contains(db) {
                out.push(TreeNode::Loading { db: db.clone() });
            } else if let Some(tables) = self.tables.get(db) {
                if tables.is_empty() {
                    out.push(TreeNode::NoTables { db: db.clone() });
                } else {
                    for t in tables {
                        out.push(TreeNode::Table { db: db.clone(), name: t.clone() });
                    }
                }
            } else {
                out.push(TreeNode::Loading { db: db.clone() });
            }
        }
        out
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        let code = key.code;

        // Mode A: REPL editing query
        if self.repl_active && self.editing_query {
            match code {
                KeyCode::Enter => {
                    if let Some(db) = self.repl_db.clone()
                        && let Some(sql) = self.submit_query()
                    {
                        return Some(Action::RunQuery(db, sql));
                    }
                }
                KeyCode::Esc => self.editing_query = false,
                KeyCode::Up => self.history_up(),
                KeyCode::Down => self.history_down(),
                _ => { self.textarea.input(key); }
            }
            return Some(Action::Noop);
        }

        // Mode B: REPL browsing
        if self.repl_active {
            match code {
                KeyCode::Char('e') => self.editing_query = true,
                KeyCode::Up => self.repl_scroll += 1,
                KeyCode::Down => self.repl_scroll = self.repl_scroll.saturating_sub(1),
                KeyCode::Char('c') => {
                    if let Some(text) = self.history_text() {
                        self.copied_at = Some(Instant::now());
                        return Some(Action::CopyDbResult(text));
                    }
                }
                KeyCode::Esc => {
                    self.repl_active = false;
                    self.editing_query = false;
                    self.repl_scroll = 0;
                }
                _ => {}
            }
            return Some(Action::Noop);
        }

        // Mode C: pull confirmation
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

        // Mode D: detail focused
        if self.detail_open && self.detail_focused {
            return Some(match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.detail_scroll = self.detail_scroll.saturating_sub(1);
                    Action::Noop
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.detail_scroll = self.detail_scroll.saturating_add(1);
                    Action::Noop
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    self.detail_h_scroll = self.detail_h_scroll.saturating_sub(4);
                    Action::Noop
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.detail_h_scroll += 4;
                    Action::Noop
                }
                KeyCode::Char(' ') => {
                    self.detail_scroll = self.detail_scroll.saturating_add(20);
                    Action::Noop
                }
                KeyCode::Tab => {
                    self.detail_focused = false;
                    Action::Noop
                }
                KeyCode::Char('/') => {
                    self.activate_repl_for_selected_table();
                    Action::Noop
                }
                KeyCode::Enter => {
                    self.close_detail();
                    Action::Noop
                }
                KeyCode::Esc => {
                    self.detail_focused = false;
                    Action::Noop
                }
                _ => return None,
            });
        }

        // Mode E/F: tree focused (possibly with detail open)
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor_up();
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor_down();
                Some(Action::Noop)
            }
            KeyCode::Enter => Some(self.handle_tree_enter()),
            KeyCode::Tab if self.detail_open => {
                self.detail_focused = true;
                Some(Action::Noop)
            }
            KeyCode::Char('/') => {
                self.activate_repl_from_tree();
                Some(Action::Noop)
            }
            KeyCode::Char('p') => {
                self.start_pull_from_cursor();
                Some(Action::Noop)
            }
            KeyCode::Char('r') => {
                self.refresh();
                Some(Action::RefreshDb)
            }
            KeyCode::Esc => {
                if self.detail_open {
                    self.close_detail();
                }
                Some(Action::Unfocus)
            }
            _ => None,
        }
    }

    fn handle_tree_enter(&mut self) -> Action {
        let tree = self.flatten_tree();
        let Some(node) = tree.get(self.tree_cursor).cloned() else {
            return Action::Noop;
        };
        match node {
            TreeNode::Database { name, expanded: true } => {
                self.expanded.remove(&name);
                self.clamp_cursor();
                Action::Noop
            }
            TreeNode::Database { name, expanded: false } => {
                self.expanded.insert(name.clone());
                if self.tables.contains_key(&name) {
                    Action::Noop
                } else {
                    self.tables_loading.insert(name.clone());
                    Action::DbFetchTables(name)
                }
            }
            TreeNode::Table { db, name } => {
                self.selected_table = Some((db, name));
                self.table_data = None;
                self.table_error = None;
                self.table_loading = false;
                self.last_refresh = None;
                self.detail_scroll = 0;
                self.detail_h_scroll = 0;
                self.detail_focused = true;
                if self.detail_open {
                    Action::Noop
                } else {
                    self.detail_open = true;
                    Action::ZoomIn
                }
            }
            TreeNode::Loading { .. } | TreeNode::NoTables { .. } => Action::Noop,
        }
    }

    fn cursor_up(&mut self) {
        self.tree_cursor = self.tree_cursor.saturating_sub(1);
    }

    fn cursor_down(&mut self) {
        let len = self.flatten_tree().len();
        if len > 0 && self.tree_cursor + 1 < len {
            self.tree_cursor += 1;
        }
    }

    fn clamp_cursor(&mut self) {
        let len = self.flatten_tree().len();
        if len == 0 {
            self.tree_cursor = 0;
        } else if self.tree_cursor >= len {
            self.tree_cursor = len - 1;
        }
    }

    fn start_pull_from_cursor(&mut self) {
        if let Some(db) = self.cursor_db() {
            self.confirming_pull = Some(db);
        }
    }

    fn activate_repl_from_tree(&mut self) {
        if let Some(db) = self.cursor_db() {
            self.repl_active = true;
            self.repl_db = Some(db);
            self.editing_query = true;
            self.history.clear();
            self.textarea = new_textarea();
            self.query_history.clear();
            self.history_index = None;
            self.repl_scroll = 0;
        }
    }

    fn activate_repl_for_selected_table(&mut self) {
        if let Some((db, _)) = self.selected_table.clone() {
            self.repl_active = true;
            self.repl_db = Some(db);
            self.editing_query = true;
            self.history.clear();
            self.textarea = new_textarea();
            self.query_history.clear();
            self.history_index = None;
            self.repl_scroll = 0;
        }
    }

    /// Returns the database name associated with the current cursor position.
    pub fn cursor_db(&self) -> Option<String> {
        let tree = self.flatten_tree();
        let node = tree.get(self.tree_cursor)?;
        match node {
            TreeNode::Database { name, .. } => Some(name.clone()),
            TreeNode::Table { db, .. } => Some(db.clone()),
            TreeNode::Loading { db } | TreeNode::NoTables { db } => Some(db.clone()),
        }
    }

    fn close_detail(&mut self) {
        self.detail_open = false;
        self.detail_focused = false;
        self.detail_scroll = 0;
        self.detail_h_scroll = 0;
        self.selected_table = None;
        self.table_data = None;
        self.table_loading = false;
        self.table_error = None;
        self.last_refresh = None;
    }

    /// Returns true when the detail pane should (re-)fetch the tail chunk.
    /// Fires for the initial load, and periodically when the user is parked
    /// at the tail (loaded bottom == table tail) so new rows append in place.
    pub fn needs_table_refresh(&self) -> bool {
        if !self.panel_visible || self.repl_active || self.table_loading || !self.detail_open {
            return false;
        }
        if self.selected_table.is_none() {
            return false;
        }
        let Some(data) = self.table_data.as_ref() else {
            return true; // initial fetch
        };
        if !self.is_at_loaded_bottom() || !is_at_table_tail(data) {
            return false;
        }
        self.last_refresh.map_or(true, |t| t.elapsed() >= AUTO_REFRESH)
    }

    /// Returns the offset of the previous chunk to fetch when the user has
    /// scrolled near the top of the loaded window and older rows remain.
    pub fn needs_previous_rows(&self) -> Option<usize> {
        if !self.panel_visible || self.repl_active || self.table_loading || !self.detail_open {
            return None;
        }
        let data = self.table_data.as_ref()?;
        if data.offset == 0 {
            return None;
        }
        if self.detail_scroll > PREFETCH_MARGIN {
            return None;
        }
        Some(data.offset.saturating_sub(TABLE_PAGE_SIZE))
    }

    /// Returns the offset of the next chunk to fetch when the user has
    /// scrolled near the bottom of the loaded window and newer rows remain.
    pub fn needs_next_rows(&self) -> Option<usize> {
        if !self.panel_visible || self.repl_active || self.table_loading || !self.detail_open {
            return None;
        }
        let data = self.table_data.as_ref()?;
        let ex_end = data.offset + data.rows.len();
        if ex_end >= data.row_count {
            return None;
        }
        let last_visible = self.detail_scroll.saturating_add(self.detail_visible_rows);
        if last_visible.saturating_add(PREFETCH_MARGIN) < data.rows.len() {
            return None;
        }
        Some(ex_end)
    }

    fn is_at_loaded_bottom(&self) -> bool {
        let Some(data) = self.table_data.as_ref() else { return true };
        if self.detail_visible_rows == 0 {
            return true;
        }
        self.detail_scroll.saturating_add(self.detail_visible_rows) >= data.rows.len()
    }

    pub fn mark_refresh_started(&mut self) {
        self.table_loading = true;
        self.last_refresh = Some(Instant::now());
    }

    pub fn mark_load_more_started(&mut self) {
        self.table_loading = true;
    }

    pub fn receive_tables(&mut self, db: String, tables: Vec<String>) {
        self.tables_loading.remove(&db);
        self.tables.insert(db, tables);
        self.clamp_cursor();
    }

    pub fn receive_table_data(&mut self, data: TableData) {
        self.table_loading = false;
        if self.selected_table.as_ref()
            != Some(&(data.db.clone(), data.table.clone()))
        {
            return;
        }
        self.table_error = None;

        let was_at_loaded_bottom = self.is_at_loaded_bottom();

        let Some(existing) = self.table_data.as_mut() else {
            // First chunk: install and park at the bottom (log-style tail).
            self.table_data = Some(data);
            self.detail_scroll = usize::MAX;
            return;
        };

        let ex_start = existing.offset;
        let ex_end = ex_start + existing.rows.len();
        let in_start = data.offset;
        let in_end = in_start + data.rows.len();

        // Case 4: fully contained — sync row_count and drop any
        // existing rows that no longer exist (table shrank).
        if in_start >= ex_start && in_end <= ex_end {
            existing.row_count = data.row_count;
            let max_loaded = data.row_count.saturating_sub(existing.offset);
            if existing.rows.len() > max_loaded {
                existing.rows.truncate(max_loaded);
            }
            return;
        }

        // Case 2: extends forward (touches or overlaps the tail end).
        if in_start <= ex_end && in_end > ex_end {
            let skip = ex_end - in_start;
            existing.rows.extend_from_slice(&data.rows[skip..]);
            existing.row_count = data.row_count;
            if was_at_loaded_bottom {
                self.detail_scroll = usize::MAX;
            }
            return;
        }

        // Case 3: extends backward (touches or overlaps the head end).
        if in_start < ex_start && in_end >= ex_start {
            let take = ex_start - in_start;
            let mut new_rows: Vec<Vec<String>> = data.rows[..take].to_vec();
            new_rows.extend(std::mem::take(&mut existing.rows));
            existing.rows = new_rows;
            existing.offset = in_start;
            existing.row_count = data.row_count;
            self.detail_scroll = self.detail_scroll.saturating_add(take);
            return;
        }

        // Case 5: disjoint — replace and park at the bottom.
        *existing = data;
        self.detail_scroll = usize::MAX;
    }

    pub fn receive_table_error(&mut self, err: String) {
        self.table_error = Some(err);
        self.table_loading = false;
    }

    pub fn textarea_text(&self) -> String {
        self.textarea.lines().join("")
    }

    pub fn submit_query(&mut self) -> Option<String> {
        let sql = self.textarea_text();
        if sql.is_empty() {
            return None;
        }
        if sql.trim().eq_ignore_ascii_case("clear") {
            self.query_history.push(sql);
            self.history_index = None;
            self.textarea = new_textarea();
            self.history.clear();
            self.repl_scroll = 0;
            return None;
        }
        self.push_query(&sql);
        self.query_history.push(sql.clone());
        self.history_index = None;
        self.textarea = new_textarea();
        self.repl_scroll = 0;
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

    pub fn clamp_repl_scroll(&mut self, total: usize, visible: usize) {
        let max = total.saturating_sub(visible);
        self.repl_scroll = self.repl_scroll.min(max);
    }

    pub fn refresh(&mut self) {
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

pub type TableListResult = Result<(String, Vec<String>), String>;

pub fn spawn_table_list(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    db: String,
) -> mpsc::Receiver<TableListResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let sql = "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name";
        let result = adb
            .query_database(&serial, &package, &db, sql)
            .map(|out| {
                let tables: Vec<String> = out
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && !s.starts_with("sqlite_"))
                    .collect();
                (db, tables)
            })
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub type TableDataResult = Result<TableData, String>;

pub fn spawn_table_data(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    db: String,
    table: String,
    kind: FetchKind,
) -> mpsc::Receiver<TableDataResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let escaped = table.replace('"', "\"\"");
        let cols_sql = format!("PRAGMA table_info(\"{escaped}\");");
        let count_sql = format!("SELECT COUNT(*) FROM \"{escaped}\";");

        let columns = match adb.query_database(&serial, &package, &db, &cols_sql) {
            Ok(out) => parse_pragma_columns(&out),
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                return;
            }
        };
        let row_count: usize = match adb.query_database(&serial, &package, &db, &count_sql) {
            Ok(out) => out.lines().next().and_then(|l| l.trim().parse().ok()).unwrap_or(0),
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                return;
            }
        };
        let offset = match kind {
            FetchKind::Tail => row_count.saturating_sub(TABLE_PAGE_SIZE),
            FetchKind::At(n) => n.min(row_count),
        };
        let rows_sql = format!(
            "SELECT * FROM \"{escaped}\" LIMIT {} OFFSET {offset};",
            TABLE_PAGE_SIZE,
        );
        let rows = match adb.query_database(&serial, &package, &db, &rows_sql) {
            Ok(out) => parse_rows(&out, columns.len().max(1)),
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                return;
            }
        };
        let _ = tx.send(Ok(TableData {
            db,
            table,
            columns,
            rows,
            row_count,
            offset,
        }));
    });
    rx
}

fn parse_rows(raw: &str, expected_cols: usize) -> Vec<Vec<String>> {
    raw.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut parts: Vec<String> = line.splitn(expected_cols, '|').map(|s| s.to_string()).collect();
            while parts.len() < expected_cols {
                parts.push(String::new());
            }
            parts
        })
        .collect()
}

fn parse_pragma_columns(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| line.split('|').nth(1).map(|s| s.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn state_with_dbs(dbs: &[&str]) -> DatabaseState {
        let mut s = DatabaseState::new();
        s.databases = dbs.iter().map(|d| d.to_string()).collect();
        s
    }

    #[test]
    fn flatten_tree_collapsed() {
        let s = state_with_dbs(&["a.db", "b.db"]);
        let tree = s.flatten_tree();
        assert_eq!(tree.len(), 2);
        assert!(matches!(&tree[0], TreeNode::Database { name, expanded: false } if name == "a.db"));
    }

    #[test]
    fn flatten_tree_expanded_shows_loading_when_not_cached() {
        let mut s = state_with_dbs(&["a.db"]);
        s.expanded.insert("a.db".into());
        s.tables_loading.insert("a.db".into());
        let tree = s.flatten_tree();
        assert_eq!(tree.len(), 2);
        assert!(matches!(&tree[1], TreeNode::Loading { db } if db == "a.db"));
    }

    #[test]
    fn flatten_tree_expanded_shows_tables() {
        let mut s = state_with_dbs(&["a.db"]);
        s.expanded.insert("a.db".into());
        s.tables.insert("a.db".into(), vec!["users".into(), "posts".into()]);
        let tree = s.flatten_tree();
        assert_eq!(tree.len(), 3);
        assert!(matches!(&tree[1], TreeNode::Table { name, .. } if name == "users"));
        assert!(matches!(&tree[2], TreeNode::Table { name, .. } if name == "posts"));
    }

    #[test]
    fn flatten_tree_expanded_shows_no_tables_when_empty() {
        let mut s = state_with_dbs(&["a.db"]);
        s.expanded.insert("a.db".into());
        s.tables.insert("a.db".into(), vec![]);
        let tree = s.flatten_tree();
        assert_eq!(tree.len(), 2);
        assert!(matches!(&tree[1], TreeNode::NoTables { .. }));
    }

    #[test]
    fn enter_on_db_expands_and_requests_fetch() {
        let mut s = state_with_dbs(&["a.db"]);
        let action = s.handle_key(key(KeyCode::Enter)).unwrap();
        assert!(matches!(action, Action::DbFetchTables(ref d) if d == "a.db"));
        assert!(s.expanded.contains("a.db"));
        assert!(s.tables_loading.contains("a.db"));
    }

    #[test]
    fn enter_on_expanded_db_collapses() {
        let mut s = state_with_dbs(&["a.db"]);
        s.expanded.insert("a.db".into());
        s.tables.insert("a.db".into(), vec!["t".into()]);
        let action = s.handle_key(key(KeyCode::Enter)).unwrap();
        assert!(matches!(action, Action::Noop));
        assert!(!s.expanded.contains("a.db"));
    }

    #[test]
    fn enter_on_expanded_cached_db_only_toggles() {
        let mut s = state_with_dbs(&["a.db"]);
        s.tables.insert("a.db".into(), vec!["t".into()]);
        let action = s.handle_key(key(KeyCode::Enter)).unwrap();
        assert!(matches!(action, Action::Noop));
        assert!(s.expanded.contains("a.db"));
    }

    #[test]
    fn enter_on_table_opens_detail_and_zooms() {
        let mut s = state_with_dbs(&["a.db"]);
        s.expanded.insert("a.db".into());
        s.tables.insert("a.db".into(), vec!["users".into()]);
        s.tree_cursor = 1;
        let action = s.handle_key(key(KeyCode::Enter)).unwrap();
        assert!(matches!(action, Action::ZoomIn));
        assert!(s.detail_open);
        assert_eq!(s.selected_table.as_ref().map(|(_, t)| t.as_str()), Some("users"));
        assert!(s.last_refresh.is_none());
    }

    #[test]
    fn cursor_clamps_at_bounds() {
        let mut s = state_with_dbs(&["a.db", "b.db"]);
        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.tree_cursor, 0);
        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.tree_cursor, 1);
        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.tree_cursor, 1);
    }

    #[test]
    fn p_confirms_pull_for_cursor_db() {
        let mut s = state_with_dbs(&["a.db", "b.db"]);
        s.tree_cursor = 1;
        s.handle_key(key(KeyCode::Char('p')));
        assert_eq!(s.confirming_pull.as_deref(), Some("b.db"));
    }

    #[test]
    fn pp_confirms_pull() {
        let mut s = state_with_dbs(&["a.db"]);
        s.handle_key(key(KeyCode::Char('p')));
        let action = s.handle_key(key(KeyCode::Char('p'))).unwrap();
        assert!(matches!(action, Action::PullDb(ref d) if d == "a.db"));
    }

    #[test]
    fn slash_activates_repl() {
        let mut s = state_with_dbs(&["a.db"]);
        s.handle_key(key(KeyCode::Char('/')));
        assert!(s.repl_active);
        assert_eq!(s.repl_db.as_deref(), Some("a.db"));
        assert!(s.editing_query);
    }

    #[test]
    fn repl_enter_runs_query() {
        let mut s = state_with_dbs(&["a.db"]);
        s.handle_key(key(KeyCode::Char('/')));
        s.textarea.insert_str("SELECT 1");
        let action = s.handle_key(key(KeyCode::Enter)).unwrap();
        assert!(matches!(action, Action::RunQuery(ref d, ref q) if d == "a.db" && q == "SELECT 1"));
    }

    #[test]
    fn repl_esc_exits_repl() {
        let mut s = state_with_dbs(&["a.db"]);
        s.handle_key(key(KeyCode::Char('/')));
        // esc exits editing first
        s.handle_key(key(KeyCode::Esc));
        assert!(s.repl_active);
        assert!(!s.editing_query);
        // esc again exits repl
        s.handle_key(key(KeyCode::Esc));
        assert!(!s.repl_active);
    }

    #[test]
    fn needs_table_refresh_initial() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        assert!(s.needs_table_refresh());
    }

    #[test]
    fn needs_table_refresh_skipped_while_loading() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.table_loading = true;
        assert!(!s.needs_table_refresh());
    }

    #[test]
    fn needs_table_refresh_skipped_in_repl() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.repl_active = true;
        assert!(!s.needs_table_refresh());
    }

    #[test]
    fn receive_table_data_stores_and_clears_loading() {
        let mut s = DatabaseState::new();
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.table_loading = true;
        s.receive_table_data(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into(), "name".into()],
            rows: vec![vec!["1".into(), "alice".into()]],
            row_count: 1,
            offset: 0,
        });
        assert!(!s.table_loading);
        assert!(s.table_data.is_some());
    }

    #[test]
    fn receive_table_data_appends_when_offset_positive() {
        let mut s = DatabaseState::new();
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (0..50).map(|i| vec![i.to_string()]).collect(),
            row_count: 100,
            offset: 0,
        });
        s.receive_table_data(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (50..60).map(|i| vec![i.to_string()]).collect(),
            row_count: 100,
            offset: 50,
        });
        let data = s.table_data.as_ref().unwrap();
        assert_eq!(data.rows.len(), 60);
        assert_eq!(data.rows[59][0], "59");
    }

    #[test]
    fn receive_table_data_discards_when_stale() {
        let mut s = DatabaseState::new();
        s.selected_table = Some(("a.db".into(), "other".into()));
        s.receive_table_data(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec![],
            rows: vec![],
            row_count: 0,
            offset: 0,
        });
        assert!(s.table_data.is_none());
    }

    #[test]
    fn receive_tables_clears_loading_and_stores() {
        let mut s = state_with_dbs(&["a.db"]);
        s.tables_loading.insert("a.db".into());
        s.receive_tables("a.db".into(), vec!["t1".into()]);
        assert!(!s.tables_loading.contains("a.db"));
        assert_eq!(s.tables.get("a.db"), Some(&vec!["t1".to_string()]));
    }

    #[test]
    fn esc_from_tree_unfocuses() {
        let mut s = state_with_dbs(&["a.db"]);
        let action = s.handle_key(key(KeyCode::Esc)).unwrap();
        assert!(matches!(action, Action::Unfocus));
    }

    #[test]
    fn esc_from_detail_tree_closes_and_unfocuses() {
        let mut s = state_with_dbs(&["a.db"]);
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        let action = s.handle_key(key(KeyCode::Esc)).unwrap();
        assert!(matches!(action, Action::Unfocus));
        assert!(!s.detail_open);
    }

    #[test]
    fn tab_focuses_detail_when_open() {
        let mut s = state_with_dbs(&["a.db"]);
        s.detail_open = true;
        s.handle_key(key(KeyCode::Tab));
        assert!(s.detail_focused);
    }

    #[test]
    fn detail_focused_up_down_scrolls() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.detail_focused = true;
        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.detail_scroll, 1);
        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.detail_scroll, 0);
    }

    #[test]
    fn needs_previous_rows_when_near_top() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_visible_rows = 20;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec![],
            rows: (0..50).map(|_| vec![]).collect(),
            row_count: 200,
            offset: 150,
        });
        s.detail_scroll = 5;
        assert_eq!(s.needs_previous_rows(), Some(100));
    }

    #[test]
    fn needs_previous_rows_none_when_at_start() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_visible_rows = 20;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec![],
            rows: (0..100).map(|_| vec![]).collect(),
            row_count: 100,
            offset: 0,
        });
        s.detail_scroll = 0;
        assert!(s.needs_previous_rows().is_none());
    }

    #[test]
    fn needs_next_rows_when_near_loaded_bottom() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_visible_rows = 20;
        // Loaded window [0, 50); more rows exist forward.
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec![],
            rows: (0..50).map(|_| vec![]).collect(),
            row_count: 500,
            offset: 0,
        });
        // Visible window reaches within PREFETCH_MARGIN of rows.len().
        s.detail_scroll = 25;
        assert_eq!(s.needs_next_rows(), Some(50));
    }

    #[test]
    fn needs_next_rows_none_when_at_table_tail() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_visible_rows = 20;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec![],
            rows: (0..50).map(|_| vec![]).collect(),
            row_count: 50,
            offset: 0,
        });
        s.detail_scroll = 25;
        assert!(s.needs_next_rows().is_none());
    }

    #[test]
    fn auto_refresh_skipped_when_not_at_loaded_bottom() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_visible_rows = 20;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec![],
            rows: (0..50).map(|_| vec![]).collect(),
            row_count: 500,
            offset: 450,
        });
        s.detail_scroll = 0;
        assert!(!s.needs_table_refresh());
    }

    #[test]
    fn auto_refresh_skipped_when_not_at_table_tail() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_visible_rows = 20;
        // Loaded bottom but there are more rows forward in the table.
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec![],
            rows: (0..50).map(|_| vec![]).collect(),
            row_count: 500,
            offset: 0,
        });
        s.detail_scroll = 30;
        assert!(!s.needs_table_refresh());
    }

    #[test]
    fn auto_refresh_runs_at_loaded_bottom_and_table_tail() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_visible_rows = 20;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec![],
            rows: (0..50).map(|_| vec![]).collect(),
            row_count: 500,
            offset: 450,
        });
        s.detail_scroll = 30;
        assert!(s.needs_table_refresh());
    }

    #[test]
    fn fetches_skipped_when_panel_not_visible() {
        let mut s = DatabaseState::new();
        s.detail_open = true;
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_visible_rows = 20;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec![],
            rows: (0..50).map(|_| vec![]).collect(),
            row_count: 500,
            offset: 0,
        });
        s.detail_scroll = 0;
        s.panel_visible = false;
        assert!(!s.needs_table_refresh());
        assert!(s.needs_previous_rows().is_none());
        assert!(s.needs_next_rows().is_none());
    }

    #[test]
    fn receive_table_data_initial_parks_at_bottom() {
        let mut s = DatabaseState::new();
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.receive_table_data(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (0..50).map(|i| vec![i.to_string()]).collect(),
            row_count: 200,
            offset: 150,
        });
        assert_eq!(s.detail_scroll, usize::MAX);
    }

    #[test]
    fn receive_table_data_prepends_older_chunk_exact() {
        let mut s = DatabaseState::new();
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_scroll = 3;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (50..100).map(|i| vec![i.to_string()]).collect(),
            row_count: 100,
            offset: 50,
        });
        s.receive_table_data(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (0..50).map(|i| vec![i.to_string()]).collect(),
            row_count: 100,
            offset: 0,
        });
        let data = s.table_data.as_ref().unwrap();
        assert_eq!(data.offset, 0);
        assert_eq!(data.rows.len(), 100);
        assert_eq!(data.rows[0][0], "0");
        assert_eq!(data.rows[50][0], "50");
        assert_eq!(s.detail_scroll, 53);
    }

    #[test]
    fn receive_extends_backward_with_overlap() {
        // Existing [12, 112); incoming [0, 50) overlaps and should prepend 12 rows.
        let mut s = DatabaseState::new();
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_scroll = 5;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (12..112).map(|i| vec![i.to_string()]).collect(),
            row_count: 112,
            offset: 12,
        });
        s.receive_table_data(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (0..50).map(|i| vec![i.to_string()]).collect(),
            row_count: 112,
            offset: 0,
        });
        let data = s.table_data.as_ref().unwrap();
        assert_eq!(data.offset, 0);
        assert_eq!(data.rows.len(), 112);
        assert_eq!(data.rows[0][0], "0");
        assert_eq!(data.rows[11][0], "11");
        assert_eq!(data.rows[12][0], "12");
        assert_eq!(data.rows[111][0], "111");
        assert_eq!(s.detail_scroll, 17);
    }

    #[test]
    fn receive_extends_forward_with_overlap() {
        // Existing [0, 50); incoming [30, 80) overlaps, should append rows 50..80.
        let mut s = DatabaseState::new();
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_visible_rows = 10;
        s.detail_scroll = 40;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (0..50).map(|i| vec![i.to_string()]).collect(),
            row_count: 80,
            offset: 0,
        });
        s.receive_table_data(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (30..80).map(|i| vec![i.to_string()]).collect(),
            row_count: 80,
            offset: 30,
        });
        let data = s.table_data.as_ref().unwrap();
        assert_eq!(data.offset, 0);
        assert_eq!(data.rows.len(), 80);
        assert_eq!(data.rows[79][0], "79");
    }

    #[test]
    fn receive_disjoint_replaces() {
        let mut s = DatabaseState::new();
        s.selected_table = Some(("a.db".into(), "t".into()));
        s.detail_scroll = 30;
        s.table_data = Some(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (0..50).map(|i| vec![i.to_string()]).collect(),
            row_count: 500,
            offset: 0,
        });
        s.receive_table_data(TableData {
            db: "a.db".into(),
            table: "t".into(),
            columns: vec!["id".into()],
            rows: (450..500).map(|i| vec![i.to_string()]).collect(),
            row_count: 500,
            offset: 450,
        });
        let data = s.table_data.as_ref().unwrap();
        assert_eq!(data.offset, 450);
        assert_eq!(data.rows.len(), 50);
        assert_eq!(s.detail_scroll, usize::MAX);
    }

    #[test]
    fn push_query_appends_input_line() {
        let mut s = DatabaseState::new();
        s.push_query("SELECT 1");
        assert_eq!(s.history, vec![ReplLine::Input("SELECT 1".into())]);
    }

    #[test]
    fn push_result_appends_output_lines() {
        let mut s = DatabaseState::new();
        s.push_result("row1\nrow2");
        assert_eq!(s.history, vec![
            ReplLine::Output("row1".into()),
            ReplLine::Output("row2".into()),
        ]);
    }

    #[test]
    fn repl_clear_empties_history() {
        let mut s = DatabaseState::new();
        s.repl_active = true;
        s.repl_db = Some("a.db".into());
        s.push_query("SELECT 1");
        s.push_result("1");
        s.repl_scroll = 2;
        s.textarea.insert_str("clear");
        let out = s.submit_query();
        assert!(out.is_none());
        assert!(s.history.is_empty());
        assert_eq!(s.repl_scroll, 0);
        assert_eq!(s.query_history, vec!["clear".to_string()]);
    }

    #[test]
    fn repl_clear_is_case_insensitive() {
        let mut s = DatabaseState::new();
        s.push_query("SELECT 1");
        s.textarea.insert_str("  CLEAR  ");
        let out = s.submit_query();
        assert!(out.is_none());
        assert!(s.history.is_empty());
    }

    #[test]
    fn history_text_returns_full_history() {
        let mut s = DatabaseState::new();
        s.push_query("SELECT 1");
        s.push_result("1");
        assert_eq!(s.history_text().as_deref(), Some("> SELECT 1\n1"));
    }

    #[test]
    fn history_up_recalls_previous_query() {
        let mut s = state_with_dbs(&["a.db"]);
        s.handle_key(key(KeyCode::Char('/')));
        s.textarea.insert_str("SELECT 1");
        s.handle_key(key(KeyCode::Enter));
        s.textarea.insert_str("SELECT 2");
        s.handle_key(key(KeyCode::Enter));
        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.textarea_text(), "SELECT 2");
        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.textarea_text(), "SELECT 1");
    }
}
