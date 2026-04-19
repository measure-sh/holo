use std::sync::{mpsc, Arc};

use crossterm::event::{KeyCode, KeyEvent};

use crate::adb::{Adb, FileMeta};
use crate::app::Action;

pub const MAX_DETAIL_BYTES: u64 = 1024 * 1024;

pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub expanded: bool,
    pub children: Option<Vec<FileNode>>,
    pub loading: bool,
}

pub struct FlatEntry {
    pub depth: usize,
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub expanded: bool,
    pub loading: bool,
    pub is_last_sibling: bool,
    pub ancestor_is_last: Vec<bool>,
}

pub enum ToggleResult {
    Expand(String),
    ExpandCached,
    Collapse,
}

pub struct FilesState {
    pub package: String,
    pub root_children: Option<Vec<FileNode>>,
    pub selected_index: usize,
    pub error: Option<String>,
    pub action_flash: Option<(&'static str, std::time::Instant)>,

    pub detail_open: bool,
    pub detail_focused: bool,
    pub detail_scroll: usize,
    pub detail_visible_rows: usize,
    pub selected_file: Option<String>,
    pub selected_meta: Option<FileMeta>,
    pub selected_kind: Option<DetailKind>,
    pub loading_meta: bool,
    pub loading_content: bool,
    pub pending_cat: Option<String>,
    pub detail_error: Option<String>,
}

impl FilesState {
    pub fn new(package: &str) -> Self {
        Self {
            package: package.to_string(),
            root_children: None,
            selected_index: 0,
            error: None,
            action_flash: None,
            detail_open: false,
            detail_focused: false,
            detail_scroll: 0,
            detail_visible_rows: 0,
            selected_file: None,
            selected_meta: None,
            selected_kind: None,
            loading_meta: false,
            loading_content: false,
            pending_cat: None,
            detail_error: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        let code = key.code;

        // Detail pane focused — scroll/open-editor/close.
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
                KeyCode::Char(' ') => {
                    self.detail_scroll = self.detail_scroll.saturating_add(20);
                    Action::Noop
                }
                KeyCode::Tab => {
                    self.detail_focused = false;
                    Action::Noop
                }
                KeyCode::Char('o') => {
                    if let Some(path) = self.selected_file.clone() {
                        Action::OpenFile(path)
                    } else {
                        Action::Noop
                    }
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

        // Tree focused.
        match code {
            KeyCode::Up => {
                self.move_up();
                Some(Action::Noop)
            }
            KeyCode::Down => {
                let count = self.flatten_visible().len();
                self.move_down(count);
                Some(Action::Noop)
            }
            KeyCode::Enter | KeyCode::Right => {
                if self.selected_is_dir() {
                    if let Some(result) = self.toggle_selected() {
                        Some(match result {
                            ToggleResult::Expand(path) => Action::ExpandDir(path),
                            ToggleResult::ExpandCached | ToggleResult::Collapse => Action::Noop,
                        })
                    } else {
                        Some(Action::Noop)
                    }
                } else if let Some(path) = self.selected_path() {
                    Some(self.open_detail_for(path))
                } else {
                    Some(Action::Noop)
                }
            }
            KeyCode::Left => {
                self.collapse_selected();
                Some(Action::Noop)
            }
            KeyCode::Tab if self.detail_open => {
                self.detail_focused = true;
                Some(Action::Noop)
            }
            KeyCode::Char('r') => {
                self.error = None;
                self.root_children = None;
                self.selected_index = 0;
                if let Some(path) = self.selected_file.clone() {
                    self.start_detail_load(&path);
                }
                Some(Action::RefreshFiles)
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

    fn open_detail_for(&mut self, path: String) -> Action {
        let first_open = !self.detail_open;
        if self.selected_file.as_deref() == Some(path.as_str()) && self.detail_open {
            return Action::Noop;
        }
        self.detail_open = true;
        self.start_detail_load(&path);
        if first_open {
            Action::ZoomIn
        } else {
            Action::Noop
        }
    }

    fn start_detail_load(&mut self, path: &str) {
        self.selected_file = Some(path.to_string());
        self.selected_meta = None;
        self.selected_kind = None;
        self.detail_error = None;
        self.detail_scroll = 0;
        self.loading_meta = true;
        self.loading_content = false;
        self.pending_cat = None;
    }

    pub fn close_detail(&mut self) {
        self.detail_open = false;
        self.detail_focused = false;
        self.detail_scroll = 0;
        self.selected_file = None;
        self.selected_meta = None;
        self.selected_kind = None;
        self.loading_meta = false;
        self.loading_content = false;
        self.pending_cat = None;
        self.detail_error = None;
    }

    pub fn receive_meta(&mut self, path: String, meta: FileMeta) {
        if self.selected_file.as_deref() != Some(path.as_str()) {
            return;
        }
        self.loading_meta = false;
        let hint = classify(&path, meta.size_bytes);
        match hint {
            DetailKindHint::Text(lang) => {
                self.selected_kind = Some(DetailKind::Text {
                    language: lang,
                    content: String::new(),
                });
                self.loading_content = true;
                self.pending_cat = Some(path);
            }
            DetailKindHint::Binary(reason) => {
                self.selected_kind = Some(DetailKind::Binary { reason });
            }
            DetailKindHint::TooLarge => {
                self.selected_kind = Some(DetailKind::TooLarge {
                    size_bytes: meta.size_bytes,
                });
            }
        }
        self.selected_meta = Some(meta);
    }

    pub fn receive_content(&mut self, path: String, bytes: Vec<u8>) {
        if self.selected_file.as_deref() != Some(path.as_str()) {
            return;
        }
        self.loading_content = false;
        let Some(DetailKind::Text { language, .. }) = self.selected_kind.take() else {
            return;
        };
        let content = String::from_utf8_lossy(&bytes).into_owned();
        self.selected_kind = Some(DetailKind::Text { language, content });
    }

    pub fn receive_detail_error(&mut self, err: String) {
        self.loading_meta = false;
        self.loading_content = false;
        self.pending_cat = None;
        self.detail_error = Some(err);
    }

    pub fn take_pending_cat(&mut self) -> Option<String> {
        self.pending_cat.take()
    }

    pub fn flatten_visible(&self) -> Vec<FlatEntry> {
        let mut entries = Vec::new();
        if let Some(children) = &self.root_children {
            let len = children.len();
            for (i, node) in children.iter().enumerate() {
                let is_last = i == len - 1;
                flatten_node(node, 0, is_last, &[], &mut entries);
            }
        }
        entries
    }

    pub fn move_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn move_down(&mut self, visible_count: usize) {
        if visible_count > 0 && self.selected_index < visible_count - 1 {
            self.selected_index += 1;
        }
    }

    pub fn toggle_selected(&mut self) -> Option<ToggleResult> {
        let flat = self.flatten_visible();
        let entry = flat.get(self.selected_index)?;
        if !entry.is_dir {
            return None;
        }
        let path = entry.path.clone();
        let node = find_node_mut(self.root_children.as_mut()?, &path)?;
        if node.expanded {
            node.expanded = false;
            Some(ToggleResult::Collapse)
        } else {
            node.expanded = true;
            if node.children.is_some() {
                Some(ToggleResult::ExpandCached)
            } else {
                node.loading = true;
                Some(ToggleResult::Expand(path))
            }
        }
    }

    pub fn collapse_selected(&mut self) {
        let flat = self.flatten_visible();
        let Some(entry) = flat.get(self.selected_index) else {
            return;
        };
        if entry.is_dir && entry.expanded {
            let path = entry.path.clone();
            if let Some(root) = self.root_children.as_mut()
                && let Some(node) = find_node_mut(root, &path)
            {
                node.expanded = false;
            }
            return;
        }
        if entry.depth == 0 {
            return;
        }
        let target_depth = entry.depth - 1;
        for i in (0..self.selected_index).rev() {
            if flat[i].depth == target_depth {
                self.selected_index = i;
                return;
            }
        }
    }

    pub fn selected_path(&self) -> Option<String> {
        let flat = self.flatten_visible();
        flat.get(self.selected_index).map(|e| e.path.clone())
    }

    pub fn selected_is_dir(&self) -> bool {
        let flat = self.flatten_visible();
        flat.get(self.selected_index).is_some_and(|e| e.is_dir)
    }

    pub fn set_root_children(&mut self, entries: Vec<(String, bool)>) {
        self.root_children = Some(
            entries
                .into_iter()
                .map(|(name, is_dir)| FileNode {
                    path: name.clone(),
                    name,
                    is_dir,
                    expanded: false,
                    children: None,
                    loading: false,
                })
                .collect(),
        );
    }

    pub fn set_children(&mut self, path: &str, entries: Vec<(String, bool)>) {
        if let Some(root) = &mut self.root_children
            && let Some(node) = find_node_mut(root, path)
        {
            node.loading = false;
            node.children = Some(
                entries
                    .into_iter()
                    .map(|(name, is_dir)| {
                        let child_path = format!("{}/{}", path, name);
                        FileNode {
                            name,
                            path: child_path,
                            is_dir,
                            expanded: false,
                            children: None,
                            loading: false,
                        }
                    })
                    .collect(),
            );
        }
    }
}

fn flatten_node(
    node: &FileNode,
    depth: usize,
    is_last: bool,
    ancestor_is_last: &[bool],
    out: &mut Vec<FlatEntry>,
) {
    out.push(FlatEntry {
        depth,
        name: node.name.clone(),
        path: node.path.clone(),
        is_dir: node.is_dir,
        expanded: node.expanded,
        loading: node.loading,
        is_last_sibling: is_last,
        ancestor_is_last: ancestor_is_last.to_vec(),
    });

    if node.expanded
        && let Some(children) = &node.children
    {
        let mut next_ancestors = ancestor_is_last.to_vec();
        next_ancestors.push(is_last);
        let len = children.len();
        for (i, child) in children.iter().enumerate() {
            let child_is_last = i == len - 1;
            flatten_node(child, depth + 1, child_is_last, &next_ancestors, out);
        }
    }
}

fn find_node_mut<'a>(nodes: &'a mut [FileNode], path: &str) -> Option<&'a mut FileNode> {
    for node in nodes.iter_mut() {
        if node.path == path {
            return Some(node);
        }
        if path.starts_with(&node.path) && path[node.path.len()..].starts_with('/')
            && let Some(children) = &mut node.children
        {
            return find_node_mut(children, path);
        }
    }
    None
}

pub type DirListResult = Result<(String, Vec<(String, bool)>), String>;

pub fn spawn_list_dir(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    path: String,
) -> mpsc::Receiver<DirListResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = adb
            .list_files(&serial, &package, &path)
            .map(|entries| (path, entries))
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailKind {
    Text { language: &'static str, content: String },
    Binary { reason: &'static str },
    TooLarge { size_bytes: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailKindHint {
    Text(&'static str),
    Binary(&'static str),
    TooLarge,
}

pub fn classify(path: &str, size_bytes: u64) -> DetailKindHint {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    let text_language: Option<&'static str> = match ext.as_str() {
        "json" => Some("json"),
        "xml" | "html" | "htm" => Some("xml"),
        "txt" | "log" | "md" => Some("text"),
        "csv" | "tsv" => Some("csv"),
        "yml" | "yaml" => Some("yaml"),
        "toml" => Some("toml"),
        "ini" | "properties" | "conf" | "cfg" => Some("ini"),
        "sh" => Some("sh"),
        "sql" => Some("sql"),
        _ => None,
    };

    if let Some(lang) = text_language {
        if size_bytes > MAX_DETAIL_BYTES {
            return DetailKindHint::TooLarge;
        }
        return DetailKindHint::Text(lang);
    }

    let binary_reason = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg" => "image",
        "db" | "db-wal" | "db-shm" | "sqlite" | "sqlite3" => "sqlite database",
        "apk" | "zip" | "jar" | "dex" | "so" | "bin" => "binary file",
        _ => "unknown format, showing meta only",
    };
    DetailKindHint::Binary(binary_reason)
}

pub type StatResult = Result<(String, FileMeta), String>;
pub type CatResult = Result<(String, Vec<u8>), String>;

pub fn spawn_stat_file(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    remote_path: String,
) -> mpsc::Receiver<StatResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = adb
            .stat_file(&serial, &package, &remote_path)
            .map(|meta| (remote_path, meta))
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn spawn_cat_file(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    remote_path: String,
    max_bytes: u64,
) -> mpsc::Receiver<CatResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = adb
            .cat_file(&serial, &package, &remote_path, max_bytes)
            .map(|bytes| (remote_path, bytes))
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

pub fn spawn_pull_file(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    remote_path: String,
) -> mpsc::Receiver<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let file_name = std::path::Path::new(&remote_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let dest = std::env::temp_dir().join("holo")
            .join(&package)
            .join("files")
            .join(format!("{timestamp}_{file_name}"));
        let result = adb
            .pull_file(&serial, &package, &remote_path, &dest)
            .map(|_| {
                if let Some(editor) = std::env::var("EDITOR").ok().or_else(|| std::env::var("VISUAL").ok()) {
                    let _ = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(format!("{} \"{}\"", editor, dest.display()))
                        .spawn();
                } else {
                    let _ = open::that(&dest);
                }
                format!("{}", dest.display())
            })
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state_with_children() -> FilesState {
        let mut state = FilesState::new("com.test");
        state.set_root_children(vec![
            ("cache".into(), true),
            ("databases".into(), true),
            ("config.xml".into(), false),
        ]);
        state
    }

    #[test]
    fn flatten_visible_root_only() {
        let state = make_state_with_children();
        let flat = state.flatten_visible();
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].name, "cache");
        assert!(flat[0].is_dir);
        assert_eq!(flat[0].depth, 0);
        assert_eq!(flat[1].name, "databases");
        assert_eq!(flat[2].name, "config.xml");
        assert!(!flat[2].is_dir);
    }

    #[test]
    fn flatten_visible_expanded_dir() {
        let mut state = make_state_with_children();
        state.set_children("cache", vec![
            ("images".into(), true),
            ("tmp.dat".into(), false),
        ]);
        find_node_mut(state.root_children.as_mut().unwrap(), "cache").unwrap().expanded = true;

        let flat = state.flatten_visible();
        assert_eq!(flat.len(), 5);
        assert_eq!(flat[0].name, "cache");
        assert_eq!(flat[1].name, "images");
        assert_eq!(flat[1].depth, 1);
        assert_eq!(flat[1].path, "cache/images");
        assert_eq!(flat[2].name, "tmp.dat");
        assert_eq!(flat[2].depth, 1);
        assert_eq!(flat[3].name, "databases");
        assert_eq!(flat[4].name, "config.xml");
    }

    #[test]
    fn flatten_visible_empty_root() {
        let state = FilesState::new("com.test");
        assert!(state.flatten_visible().is_empty());
    }

    #[test]
    fn move_up_saturates_at_zero() {
        let mut state = make_state_with_children();
        state.selected_index = 0;
        state.move_up();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn move_down_clamps_at_end() {
        let mut state = make_state_with_children();
        state.selected_index = 2;
        state.move_down(3);
        assert_eq!(state.selected_index, 2);
    }

    #[test]
    fn move_up_and_down() {
        let mut state = make_state_with_children();
        state.move_down(3);
        assert_eq!(state.selected_index, 1);
        state.move_down(3);
        assert_eq!(state.selected_index, 2);
        state.move_up();
        assert_eq!(state.selected_index, 1);
    }

    #[test]
    fn toggle_expands_dir_needing_load() {
        let mut state = make_state_with_children();
        state.selected_index = 0;
        match state.toggle_selected() {
            Some(ToggleResult::Expand(path)) => assert_eq!(path, "cache"),
            other => panic!("expected Expand, got {:?}", other.is_some()),
        }
        let node = find_node_mut(state.root_children.as_mut().unwrap(), "cache").unwrap();
        assert!(node.expanded);
        assert!(node.loading);
    }

    #[test]
    fn toggle_collapses_expanded_dir() {
        let mut state = make_state_with_children();
        state.set_children("cache", vec![("a.txt".into(), false)]);
        find_node_mut(state.root_children.as_mut().unwrap(), "cache").unwrap().expanded = true;

        state.selected_index = 0;
        assert!(matches!(state.toggle_selected(), Some(ToggleResult::Collapse)));
    }

    #[test]
    fn toggle_expand_cached() {
        let mut state = make_state_with_children();
        state.set_children("cache", vec![("a.txt".into(), false)]);

        state.selected_index = 0;
        assert!(matches!(state.toggle_selected(), Some(ToggleResult::ExpandCached)));
    }

    #[test]
    fn toggle_on_file_returns_none() {
        let mut state = make_state_with_children();
        state.selected_index = 2;
        assert!(state.toggle_selected().is_none());
    }

    #[test]
    fn selected_path_returns_correct_value() {
        let state = make_state_with_children();
        assert_eq!(state.selected_path().as_deref(), Some("cache"));
    }

    #[test]
    fn is_last_sibling_set_correctly() {
        let state = make_state_with_children();
        let flat = state.flatten_visible();
        assert!(!flat[0].is_last_sibling);
        assert!(!flat[1].is_last_sibling);
        assert!(flat[2].is_last_sibling);
    }

    #[test]
    fn collapse_on_child_moves_to_parent() {
        let mut state = make_state_with_children();
        state.set_children("cache", vec![
            ("images".into(), true),
            ("tmp.dat".into(), false),
        ]);
        find_node_mut(state.root_children.as_mut().unwrap(), "cache").unwrap().expanded = true;

        state.selected_index = 2;
        state.collapse_selected();
        assert_eq!(state.selected_index, 0);
        let cache = find_node_mut(state.root_children.as_mut().unwrap(), "cache").unwrap();
        assert!(cache.expanded, "parent should remain expanded on first Left");
    }

    #[test]
    fn collapse_on_expanded_dir_collapses_in_place() {
        let mut state = make_state_with_children();
        state.set_children("cache", vec![("a.txt".into(), false)]);
        find_node_mut(state.root_children.as_mut().unwrap(), "cache").unwrap().expanded = true;

        state.selected_index = 0;
        state.collapse_selected();
        let cache = find_node_mut(state.root_children.as_mut().unwrap(), "cache").unwrap();
        assert!(!cache.expanded);
    }

    #[test]
    fn collapse_on_root_level_file_is_noop() {
        let mut state = make_state_with_children();
        state.selected_index = 2;
        state.collapse_selected();
        assert_eq!(state.selected_index, 2);
    }

    #[test]
    fn classify_json_is_text() {
        assert_eq!(classify("config.json", 1000), DetailKindHint::Text("json"));
    }

    #[test]
    fn classify_xml_and_prefs() {
        assert_eq!(classify("prefs.xml", 1000), DetailKindHint::Text("xml"));
        assert_eq!(classify("notes.md", 50), DetailKindHint::Text("text"));
    }

    #[test]
    fn classify_image_is_binary() {
        assert_eq!(classify("icon.png", 4096), DetailKindHint::Binary("image"));
        assert_eq!(classify("logo.SVG", 4096), DetailKindHint::Binary("image"));
    }

    #[test]
    fn classify_sqlite_is_binary() {
        assert_eq!(classify("app.db", 1000), DetailKindHint::Binary("sqlite database"));
        assert_eq!(classify("cache.sqlite3", 1000), DetailKindHint::Binary("sqlite database"));
    }

    #[test]
    fn classify_unknown_is_binary_unknown() {
        assert_eq!(classify("data.bin", 1000), DetailKindHint::Binary("binary file"));
        assert_eq!(classify("mystery", 1000), DetailKindHint::Binary("unknown format, showing meta only"));
    }

    #[test]
    fn classify_too_large_text_is_too_large() {
        assert_eq!(classify("huge.json", 5_000_000), DetailKindHint::TooLarge);
        assert_eq!(classify("huge.log", MAX_DETAIL_BYTES + 1), DetailKindHint::TooLarge);
    }

    #[test]
    fn classify_at_limit_is_text() {
        assert_eq!(classify("ok.json", MAX_DETAIL_BYTES), DetailKindHint::Text("json"));
    }

    #[test]
    fn ancestor_is_last_propagated() {
        let mut state = make_state_with_children();
        state.set_children("databases", vec![
            ("app.db".into(), false),
        ]);
        find_node_mut(state.root_children.as_mut().unwrap(), "databases").unwrap().expanded = true;

        let flat = state.flatten_visible();
        let db_entry = &flat[2];
        assert_eq!(db_entry.name, "app.db");
        assert_eq!(db_entry.ancestor_is_last, vec![false]);
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
    }

    #[test]
    fn enter_on_file_opens_detail_and_zooms_first_time() {
        let mut state = make_state_with_children();
        state.selected_index = 2;
        let action = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, Some(Action::ZoomIn)));
        assert!(state.detail_open);
        assert_eq!(state.selected_file.as_deref(), Some("config.xml"));
        assert!(state.loading_meta);
    }

    #[test]
    fn enter_on_same_file_is_noop() {
        let mut state = make_state_with_children();
        state.selected_index = 2;
        state.handle_key(key(KeyCode::Enter));
        // loading done, kind resolved
        state.loading_meta = false;
        let again = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(again, Some(Action::Noop)));
    }

    #[test]
    fn enter_on_different_file_rescopes_detail() {
        let mut state = make_state_with_children();
        state.set_children("cache", vec![("a.txt".into(), false)]);
        find_node_mut(state.root_children.as_mut().unwrap(), "cache").unwrap().expanded = true;

        state.selected_index = 3; // config.xml after expansion
        state.handle_key(key(KeyCode::Enter));
        assert!(state.detail_open);
        // Finish "loading" for the first file
        state.loading_meta = false;

        state.selected_index = 1; // a.txt
        let action = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(&action, Some(Action::Noop)));
        assert_eq!(state.selected_file.as_deref(), Some("cache/a.txt"));
        assert!(state.loading_meta, "re-scoping must trigger a fresh stat load");
    }

    #[test]
    fn tree_navigation_does_not_trigger_load() {
        let mut state = make_state_with_children();
        state.set_children("cache", vec![
            ("a.txt".into(), false),
            ("b.txt".into(), false),
        ]);
        find_node_mut(state.root_children.as_mut().unwrap(), "cache").unwrap().expanded = true;
        // open detail on one file first
        state.selected_index = 1;
        state.handle_key(key(KeyCode::Enter));
        state.loading_meta = false;
        let pinned = state.selected_file.clone();

        for k in [KeyCode::Down, KeyCode::Down, KeyCode::Up, KeyCode::Down] {
            let action = state.handle_key(key(k));
            assert!(matches!(action, Some(Action::Noop)), "{:?} must not fire", k);
            assert_eq!(state.selected_file, pinned, "cursor nav must not change selected_file");
            assert!(!state.loading_meta, "cursor nav must not start a load");
        }
    }

    #[test]
    fn tab_toggles_detail_focus_when_open() {
        let mut state = make_state_with_children();
        state.selected_index = 2;
        state.handle_key(key(KeyCode::Enter));
        assert!(state.detail_open);
        assert!(!state.detail_focused);

        let a = state.handle_key(key(KeyCode::Tab));
        assert!(matches!(a, Some(Action::Noop)));
        assert!(state.detail_focused);

        let b = state.handle_key(key(KeyCode::Tab));
        assert!(matches!(b, Some(Action::Noop)));
        assert!(!state.detail_focused);
    }

    #[test]
    fn esc_from_tree_closes_detail_then_unfocuses() {
        let mut state = make_state_with_children();
        state.selected_index = 2;
        state.handle_key(key(KeyCode::Enter));
        let action = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, Some(Action::Unfocus)));
        assert!(!state.detail_open);
    }

    #[test]
    fn receive_meta_for_text_queues_cat() {
        let mut state = FilesState::new("com.test");
        state.detail_open = true;
        state.selected_file = Some("a.json".into());
        state.loading_meta = true;
        state.receive_meta(
            "a.json".into(),
            FileMeta { size_bytes: 100, modified: None, mode: "-rw-".into() },
        );
        assert!(!state.loading_meta);
        assert!(state.loading_content);
        assert_eq!(state.take_pending_cat().as_deref(), Some("a.json"));
        assert!(matches!(state.selected_kind, Some(DetailKind::Text { .. })));
    }

    #[test]
    fn receive_meta_for_binary_does_not_queue_cat() {
        let mut state = FilesState::new("com.test");
        state.detail_open = true;
        state.selected_file = Some("icon.png".into());
        state.loading_meta = true;
        state.receive_meta(
            "icon.png".into(),
            FileMeta { size_bytes: 100, modified: None, mode: "-rw-".into() },
        );
        assert!(!state.loading_content);
        assert!(state.take_pending_cat().is_none());
        assert!(matches!(state.selected_kind, Some(DetailKind::Binary { reason: "image" })));
    }

    #[test]
    fn receive_meta_for_stale_path_is_dropped() {
        let mut state = FilesState::new("com.test");
        state.detail_open = true;
        state.selected_file = Some("current.json".into());
        state.loading_meta = true;
        state.receive_meta(
            "stale.json".into(),
            FileMeta { size_bytes: 100, modified: None, mode: "-rw-".into() },
        );
        assert!(state.loading_meta);
        assert!(state.selected_kind.is_none());
    }

    #[test]
    fn receive_content_for_stale_path_is_dropped() {
        let mut state = FilesState::new("com.test");
        state.detail_open = true;
        state.selected_file = Some("current.json".into());
        state.selected_kind = Some(DetailKind::Text { language: "json", content: String::new() });
        state.loading_content = true;
        state.receive_content("stale.json".into(), b"{}".to_vec());
        assert!(state.loading_content);
        match state.selected_kind.as_ref().unwrap() {
            DetailKind::Text { content, .. } => assert!(content.is_empty()),
            _ => panic!("expected Text"),
        }
    }
}
