use crossterm::event::{KeyCode, KeyEvent};

use crate::app::Action;
use crate::session::{self, SessionId, SessionSummary};

/// State backing the sessions history dialog (`Ctrl+H`). Lives on `App` and
/// is queried by the dialog renderer. Not a panel — the dialog is opened via
/// `Navigation::Sessions`, mirroring the device/app dropdowns.
///
/// Row layout: index 0 is always a synthetic **Live** entry — selecting it
/// switches back to the live-data view, even when no app is currently
/// attached (in which case the row reads "no app attached" and selecting it
/// just exits whatever captured-session view is open). Indices 1..=N are the
/// captured sessions in `sessions` (with the live one filtered out so the
/// same directory isn't shown twice).
pub struct SessionsState {
    pub sessions: Vec<SessionSummary>,
    /// 0-based index into the rendered list (Live row at 0, history at
    /// 1..=visible_history().count()).
    pub selected: usize,
    pub error: Option<String>,
    /// Identifies the live capture's session, if `DataSources` is currently
    /// writing one. The synthetic Live row at the top of the list shows
    /// this session's metadata when present; otherwise it reads "no app
    /// attached" but stays selectable so users can always exit a captured
    /// view.
    pub live_id: Option<SessionId>,
}

impl SessionsState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            selected: 0,
            error: None,
            live_id: None,
        }
    }

    pub fn set_sessions(&mut self, sessions: Vec<SessionSummary>) {
        self.sessions = sessions;
        // selected counts the Live row plus visible history; clamp to that.
        let max = self.total_rows().saturating_sub(1);
        if self.selected > max {
            self.selected = max;
        }
        self.error = None;
    }

    /// Sessions to render below the Live row — everything in `sessions`
    /// except the entry whose id matches `live_id` (we render that one
    /// at the top instead of duplicating it).
    pub fn visible_history(&self) -> impl Iterator<Item = &SessionSummary> {
        self.sessions
            .iter()
            .filter(move |s| Some(&s.id) != self.live_id.as_ref())
    }

    /// One Live row plus visible history.
    pub fn total_rows(&self) -> usize {
        1 + self.visible_history().count()
    }

    /// Lookup the captured session at a given rendered index. Returns None
    /// for index 0 (the Live row, which has no captured `SessionSummary`).
    pub fn history_at(&self, idx: usize) -> Option<&SessionSummary> {
        if idx == 0 {
            return None;
        }
        self.visible_history().nth(idx - 1)
    }

    /// `viewing_id` is the id of the session currently on screen (sourced
    /// from `App::session_view()`). Passed in rather than stored on the
    /// dialog so the only place "which session is open" lives is App.
    pub fn handle_key(&mut self, key: KeyEvent, viewing_id: Option<&SessionId>) -> Option<Action> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                Some(Action::Noop)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.total_rows().saturating_sub(1);
                self.selected = (self.selected + 1).min(max);
                Some(Action::Noop)
            }
            KeyCode::Char('c') => Some(Action::CopyPath(
                session::sessions_root().display().to_string(),
            )),
            KeyCode::Enter => {
                if self.selected == 0 {
                    // Live row — always selectable. Closes any captured
                    // view; no-op when nothing was open.
                    return Some(Action::CloseSession);
                }
                let summary = self.history_at(self.selected)?;
                Some(Action::OpenSession(summary.id.clone()))
            }
            KeyCode::Char('d') => {
                if self.selected == 0 {
                    // Live row isn't deletable — the writer is still
                    // appending, and "exit view" is the closest semantic.
                    return Some(Action::Noop);
                }
                let summary = self.history_at(self.selected)?;
                // Don't delete the directory the session-view panels are
                // reading from — trace files etc. would 404 mid-render.
                if viewing_id == Some(&summary.id) {
                    return Some(Action::Noop);
                }
                Some(Action::DeleteSession(summary.id.clone()))
            }
            KeyCode::Esc => Some(Action::Unfocus),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn summary(pid: u32, dir: &str) -> SessionSummary {
        SessionSummary {
            id: SessionId {
                package: "com.test".into(),
                dir_name: dir.into(),
            },
            started_at: Local::now(),
            ended_at: None,
            duration: None,
            size_bytes: 0,
            pid,
            device_serial: "emu".into(),
            device_model: None,
        }
    }

    #[test]
    fn dialog_always_has_a_live_row() {
        // Empty history: just the Live row.
        let s = SessionsState::new();
        assert_eq!(s.total_rows(), 1);
        assert!(s.history_at(0).is_none());
    }

    #[test]
    fn live_session_is_filtered_out_of_history() {
        let mut s = SessionsState::new();
        let live = summary(99, "live");
        s.live_id = Some(live.id.clone());
        s.set_sessions(vec![live, summary(1, "older"), summary(2, "older2")]);
        // 1 (Live row) + 2 (older + older2). The "live" SessionSummary
        // is rendered as the synthetic top row, not duplicated below.
        assert_eq!(s.total_rows(), 3);
        assert_eq!(s.history_at(1).unwrap().id.dir_name, "older");
        assert_eq!(s.history_at(2).unwrap().id.dir_name, "older2");
    }

    #[test]
    fn navigate_clamps_to_total_rows() {
        let mut s = SessionsState::new();
        s.set_sessions(vec![summary(1, "a"), summary(2, "b")]);
        // Rows: [Live, a, b].
        s.handle_key(key(KeyCode::Char('j')), None);
        assert_eq!(s.selected, 1);
        s.handle_key(key(KeyCode::Char('j')), None);
        assert_eq!(s.selected, 2);
        // Past the end clamps; can't move below the last history row.
        s.handle_key(key(KeyCode::Char('j')), None);
        assert_eq!(s.selected, 2);
        s.handle_key(key(KeyCode::Char('k')), None);
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn enter_on_live_row_emits_close_session() {
        // Works even when no app is attached and `live_id` is None —
        // this is the only way out of a captured-session view in that
        // case.
        let s = &mut SessionsState::new();
        let action = s.handle_key(key(KeyCode::Enter), None);
        assert!(matches!(action, Some(Action::CloseSession)));
    }

    #[test]
    fn enter_on_history_row_opens_that_session() {
        let mut s = SessionsState::new();
        s.set_sessions(vec![summary(1, "older")]);
        s.handle_key(key(KeyCode::Char('j')), None); // selected = 1 (the older row)
        let action = s.handle_key(key(KeyCode::Enter), None);
        assert!(matches!(action, Some(Action::OpenSession(_))));
    }

    #[test]
    fn esc_always_unfocuses() {
        let mut s = SessionsState::new();
        let action = s.handle_key(key(KeyCode::Esc), None);
        assert!(matches!(action, Some(Action::Unfocus)));

        let viewed = SessionId {
            package: "com.test".into(),
            dir_name: "x".into(),
        };
        let action = s.handle_key(key(KeyCode::Esc), Some(&viewed));
        assert!(matches!(action, Some(Action::Unfocus)));
    }

    #[test]
    fn d_on_live_row_is_noop() {
        let mut s = SessionsState::new();
        let action = s.handle_key(key(KeyCode::Char('d')), None);
        assert!(matches!(action, Some(Action::Noop)));
    }

    #[test]
    fn d_emits_delete_for_history_row() {
        let mut s = SessionsState::new();
        s.set_sessions(vec![summary(1, "older")]);
        s.handle_key(key(KeyCode::Char('j')), None); // move past Live row
        let action = s.handle_key(key(KeyCode::Char('d')), None);
        assert!(matches!(action, Some(Action::DeleteSession(_))));
    }

    #[test]
    fn d_is_noop_on_currently_viewed_session() {
        let mut s = SessionsState::new();
        let viewed = summary(7, "viewed");
        let viewed_id = viewed.id.clone();
        s.set_sessions(vec![viewed]);
        s.handle_key(key(KeyCode::Char('j')), Some(&viewed_id)); // move past Live row
        let action = s.handle_key(key(KeyCode::Char('d')), Some(&viewed_id));
        assert!(matches!(action, Some(Action::Noop)));
    }

    #[test]
    fn c_emits_copy_path_with_sessions_root() {
        let mut s = SessionsState::new();
        let action = s.handle_key(key(KeyCode::Char('c')), None);
        let Some(Action::CopyPath(path)) = action else {
            panic!("expected CopyPath action");
        };
        assert!(
            path.ends_with("holo/sessions"),
            "expected sessions root path, got {path}"
        );
    }
}
