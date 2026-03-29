use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};

use crate::monitor::{MonitorState, Trend};
use crate::panel;
use crate::theme;
use crate::ui::panel_block;

const SPARK_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn format_mb(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    if mb >= 100.0 {
        format!("{:.0} MB", mb)
    } else {
        format!("{:.1} MB", mb)
    }
}

fn format_mb_precise(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    format!("{:.2} MB", mb)
}

fn trend_symbol(trend: Trend) -> (&'static str, ratatui::style::Color) {
    let t = theme::current();
    match trend {
        Trend::Rising => ("▲", t.danger),
        Trend::Falling => ("▼", t.success),
        Trend::Stable => ("─", t.muted),
    }
}

fn sparkline_str(data: &[u64], width: usize) -> (String, u64, u64) {
    if data.is_empty() {
        return (String::new(), 0, 0);
    }
    let start = data.len().saturating_sub(width);
    let slice = &data[start..];
    let min = *slice.iter().min().unwrap();
    let max = *slice.iter().max().unwrap();
    let range = max.saturating_sub(min);

    let s = slice
        .iter()
        .map(|&v| {
            if range == 0 {
                SPARK_CHARS[3]
            } else {
                let idx = ((v - min) * 7 / range).min(7) as usize;
                SPARK_CHARS[idx]
            }
        })
        .collect();
    (s, min, max)
}

fn sparkline_str_f32(data: &[f32], width: usize) -> (String, f32, f32) {
    if data.is_empty() {
        return (String::new(), 0.0, 0.0);
    }
    let start = data.len().saturating_sub(width);
    let slice = &data[start..];
    let min = slice.iter().copied().reduce(f32::min).unwrap();
    let max = slice.iter().copied().reduce(f32::max).unwrap();
    let range = max - min;

    let s = slice
        .iter()
        .map(|&v| {
            if range < 0.01 {
                SPARK_CHARS[3]
            } else {
                let idx = (((v - min) / range * 7.0) as usize).min(7);
                SPARK_CHARS[idx]
            }
        })
        .collect();
    (s, min, max)
}

fn mem_item(
    label: &str,
    data: &[u64],
    trend: Trend,
    spark_width: usize,
) -> ListItem<'static> {
    let t = theme::current();
    let current = data.last().copied().unwrap_or(0);
    let (spark, min, max) = sparkline_str(data, spark_width);
    let (arrow, arrow_color) = trend_symbol(trend);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", label), Style::new().fg(t.fg)),
            Span::styled(spark, Style::new().fg(t.sparkline)),
            Span::styled(format!("  {:>8}", format_mb(current)), Style::new().fg(t.fg)),
            Span::raw(" "),
            Span::styled(arrow.to_string(), Style::new().fg(arrow_color)),
        ]),
    ];
    if min != max {
        lines.push(Line::from(Span::styled(
            format!(" {:>12}{}-{}", "", format_mb(min), format_mb(max)),
            Style::new().fg(t.muted),
        )));
    }
    lines.push(Line::raw(""));
    ListItem::new(lines)
}

fn disk_item(
    label: &str,
    data: &[u64],
    trend: Trend,
    spark_width: usize,
) -> ListItem<'static> {
    let t = theme::current();
    let current = data.last().copied().unwrap_or(0);
    let (spark, min, max) = sparkline_str(data, spark_width);
    let (arrow, arrow_color) = trend_symbol(trend);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", label), Style::new().fg(t.fg)),
            Span::styled(spark, Style::new().fg(t.sparkline)),
            Span::styled(format!("  {:>8}", format_mb_precise(current)), Style::new().fg(t.fg)),
            Span::raw(" "),
            Span::styled(arrow.to_string(), Style::new().fg(arrow_color)),
        ]),
    ];
    if min != max {
        lines.push(Line::from(Span::styled(
            format!(" {:>12}{}-{}", "", format_mb_precise(min), format_mb_precise(max)),
            Style::new().fg(t.muted),
        )));
    }
    lines.push(Line::raw(""));
    ListItem::new(lines)
}

fn cpu_item(data: &[f32], spark_width: usize) -> ListItem<'static> {
    let t = theme::current();
    let current = data.last().copied().unwrap_or(0.0);
    let (spark, min, max) = sparkline_str_f32(data, spark_width);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", "CPU"), Style::new().fg(t.fg)),
            Span::styled(spark, Style::new().fg(t.sparkline)),
            Span::styled(format!("  {:>7.1}%", current), Style::new().fg(t.fg)),
        ]),
    ];
    if (max - min) >= 0.01 {
        lines.push(Line::from(Span::styled(
            format!(" {:>12}{:.1}-{:.1}%", "", min, max),
            Style::new().fg(t.muted),
        )));
    }
    lines.push(Line::raw(""));
    ListItem::new(lines)
}

fn render_monitor(
    frame: &mut Frame,
    area: Rect,
    panel_number: u8,
    focused: bool,
    state: &MonitorState,
    items_fn: impl FnOnce(usize, &MonitorState) -> Vec<ListItem<'static>>,
) {
    let t = theme::current();
    let block = panel_block(panel_number, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.history.is_empty() {
        let items = vec![Line::styled(" waiting...", Style::new().fg(t.muted))];
        frame.render_widget(List::new(items), inner);
        return;
    }

    if !state.debuggable {
        let items = vec![Line::styled(" not debuggable", Style::new().fg(t.muted))];
        frame.render_widget(List::new(items), inner);
        return;
    }

    let spark_width = (inner.width as usize).saturating_sub(27).max(5);
    frame.render_widget(List::new(items_fn(spark_width, state)), inner);
}

pub fn render_disk_panel(frame: &mut Frame, area: Rect, focused: bool, state: &MonitorState) {
    render_monitor(frame, area, panel::DISK, focused, state, |sw, st| {
        let data = st.sparkline_u64(|m| m.data_kb);
        let cache = st.sparkline_u64(|m| m.cache_kb);
        vec![
            disk_item("Data", &data, st.trend_u64(|m| m.data_kb), sw),
            disk_item("Cache", &cache, st.trend_u64(|m| m.cache_kb), sw),
        ]
    });
}

pub fn render_system_panel(frame: &mut Frame, area: Rect, focused: bool, state: &MonitorState) {
    render_monitor(frame, area, panel::SYSTEM, focused, state, |sw, st| {
        let cpu = st.sparkline_f32(|m| m.cpu_percent);
        let rss = st.sparkline_u64(|m| m.rss_kb);
        vec![
            cpu_item(&cpu, sw),
            mem_item("RSS", &rss, st.trend_u64(|m| m.rss_kb), sw),
        ]
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_single_value() {
        let (s, _, _) = sparkline_str(&[100], 10);
        assert_eq!(s.chars().count(), 1);
    }

    #[test]
    fn sparkline_constant_values() {
        let (s, min, max) = sparkline_str(&[50, 50, 50], 10);
        assert!(s.chars().all(|c| c == SPARK_CHARS[3]));
        assert_eq!(min, max);
    }

    #[test]
    fn sparkline_range_values() {
        let (s, min, max) = sparkline_str(&[0, 100], 10);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars[0], SPARK_CHARS[0]);
        assert_eq!(chars[1], SPARK_CHARS[7]);
        assert_eq!(min, 0);
        assert_eq!(max, 100);
    }

    #[test]
    fn sparkline_caps_to_width() {
        let data: Vec<u64> = (0..20).collect();
        let (s, min, max) = sparkline_str(&data, 5);
        assert_eq!(s.chars().count(), 5);
        assert_eq!(min, 15);
        assert_eq!(max, 19);
    }

    #[test]
    fn sparkline_f32_range() {
        let (s, min, max) = sparkline_str_f32(&[0.0, 50.0, 100.0], 10);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars[0], SPARK_CHARS[0]);
        assert_eq!(chars[2], SPARK_CHARS[7]);
        assert_eq!(min, 0.0);
        assert_eq!(max, 100.0);
    }

    #[test]
    fn format_mb_small() {
        assert_eq!(format_mb(512), "0.5 MB");
    }

    #[test]
    fn format_mb_large() {
        assert_eq!(format_mb(128000), "125 MB");
    }

}
