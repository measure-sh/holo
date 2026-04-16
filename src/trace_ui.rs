use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame, layout::Rect,
};

use crate::panel;
use crate::theme;
use crate::trace::TraceState;
use crate::ui::panel_block;

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn render_trace_panel(frame: &mut Frame, area: Rect, focused: bool, state: &TraceState) {
    let t = theme::current();
    let mut block = panel_block(panel::TRACE, focused);

    if focused {
        let accent = Style::new().fg(t.danger);
        let muted = Style::new().fg(t.muted);
        let border = Style::new().fg(panel::by_number(panel::TRACE).border_color(true));
        let mut spans = Vec::new();
        if state.recording {
            spans.extend([
                Span::styled(" s", accent),
                Span::styled("top ", muted),
            ]);
        } else {
            spans.extend([
                Span::styled(" s", accent),
                Span::styled("tart ", muted),
                Span::styled("───", border),
                Span::styled(" \u{25C2}", accent),
                Span::styled(state.preset.name(), muted),
                Span::styled("\u{25B8} ", accent),
            ]);
            if !state.pulled_traces.is_empty() {
                spans.extend([
                    Span::styled("───", border),
                    Span::styled(" ↩", accent),
                    Span::styled(" open ", muted),
                    Span::styled("───", border),
                    Span::styled(" d", accent),
                    Span::styled("elete ", muted),
                ]);
            }
        }
        block = block.title_bottom(Line::from(spans));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let flash_active = state.message_at
        .is_some_and(|ts| ts.elapsed() < std::time::Duration::from_secs(1));

    let mut items: Vec<ListItem> = Vec::new();

    if state.recording {
        let elapsed = state.started_at
            .map(|ts| ts.elapsed().as_secs())
            .unwrap_or(0);
        let mins = elapsed / 60;
        let secs = elapsed % 60;
        let spinner_idx = state.started_at
            .map(|ts| (ts.elapsed().as_millis() / 80) as usize % SPINNER.len())
            .unwrap_or(0);
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{} ", SPINNER[spinner_idx]),
                Style::new().fg(t.accent),
            ),
            Span::styled(
                format!("{} {:02}:{:02}", state.preset.name().to_lowercase(), mins, secs),
                Style::new().fg(t.accent),
            ),
        ])));
    } else if flash_active {
        if let Some(msg) = &state.status_message {
            items.push(ListItem::new(Line::from(
                Span::styled(msg.clone(), Style::new().fg(t.success)),
            )));
        }
    } else if state.pulled_traces.is_empty() {
        items.push(ListItem::new(Line::from(
            Span::styled("no traces yet", Style::new().fg(t.muted)),
        )));
    } else {
        for (i, path) in state.pulled_traces.iter().enumerate() {
            let name = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let tag = if name.starts_with("mem_") {
                "[mem]"
            } else if name.starts_with("cpu_") {
                "[cpu]"
            } else {
                "[def]"
            };
            let display_name = if name.starts_with("mem_") || name.starts_with("cpu_") || name.starts_with("def_") {
                &name[4..]
            } else {
                &name
            };
            let selected = focused && i == state.selected_index;
            let style = if selected {
                Style::new().fg(t.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(t.muted)
            };
            let prefix = if selected { "▸ " } else { "  " };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{tag} "), Style::new().fg(t.muted)),
                Span::styled(display_name.to_string(), style),
            ])));
        }
    }

    if focused && !state.recording && !flash_active {
        let used_lines = items.len();
        let available = inner.height as usize;
        if available > used_lines + 1 {
            let padding = available - used_lines - 1;
            for _ in 0..padding {
                items.push(ListItem::new(Line::raw("")));
            }
        }
        items.push(ListItem::new(Line::from(
            Span::styled(
                format!(" {}", state.preset.description()),
                Style::new().fg(t.muted),
            ),
        )));
    }

    frame.render_widget(List::new(items), inner);
}
