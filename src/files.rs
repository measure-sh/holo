use std::sync::{mpsc, Arc};

use crossterm::event::{KeyCode, KeyEvent};

use crate::adb::Adb;
use crate::app::Action;

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
        if let Some(entry) = flat.get(self.selected_index) {
            if entry.is_dir && entry.expanded {
                if let Some(node) = find_node_mut(self.root_children.as_mut().unwrap(), &entry.path) {
                    node.expanded = false;
                }
            }
        }
    }

    pub fn selected_path(&self) -> Option<String> {
        let flat = self.flatten_visible();
        flat.get(self.selected_index).map(|e| e.path.clone())
    }

    pub fn selected_is_dir(&self) -> bool {
        let flat = self.flatten_visible();
        flat.get(self.selected_index).map_or(false, |e| e.is_dir)
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
        if let Some(root) = &mut self.root_children {
            if let Some(node) = find_node_mut(root, path) {
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

    if node.expanded {
        if let Some(children) = &node.children {
            let mut next_ancestors = ancestor_is_last.to_vec();
            next_ancestors.push(is_last);
            let len = children.len();
            for (i, child) in children.iter().enumerate() {
                let child_is_last = i == len - 1;
                flatten_node(child, depth + 1, child_is_last, &next_ancestors, out);
            }
        }
    }
}

fn find_node_mut<'a>(nodes: &'a mut [FileNode], path: &str) -> Option<&'a mut FileNode> {
    for node in nodes.iter_mut() {
        if node.path == path {
            return Some(node);
        }
        if path.starts_with(&node.path) && path[node.path.len()..].starts_with('/') {
            if let Some(children) = &mut node.children {
                return find_node_mut(children, path);
            }
        }
    }
    None
}

pub fn spawn_list_dir(
    adb: Arc<dyn Adb>,
    serial: String,
    package: String,
    path: String,
) -> mpsc::Receiver<Result<(String, Vec<(String, bool)>), String>> {
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
        let dest = std::env::temp_dir().join("msh")
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
