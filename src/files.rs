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
}

impl FilesState {
    pub fn new(package: &str) -> Self {
        Self {
            package: package.to_string(),
            root_children: None,
            selected_index: 0,
            error: None,
            action_flash: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        let code = key.code;
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
                    Some(Action::OpenFile(path))
                } else {
                    Some(Action::Noop)
                }
            }
            KeyCode::Left => {
                self.collapse_selected();
                Some(Action::Noop)
            }
            KeyCode::Char('r') => {
                self.error = None;
                self.root_children = None;
                self.selected_index = 0;
                Some(Action::RefreshFiles)
            }
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
        }
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
}
