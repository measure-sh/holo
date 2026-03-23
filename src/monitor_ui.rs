use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
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

fn trend_symbol(trend: Trend) -> (&'static str, ratatui::style::Color) {
    match trend {
        Trend::Rising => ("▲", theme::RED),
        Trend::Falling => ("▼", theme::GREEN),
        Trend::Stable => ("─", theme::MUTED),
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
    let current = data.last().copied().unwrap_or(0);
    let (spark, min, max) = sparkline_str(data, spark_width);
    let (arrow, arrow_color) = trend_symbol(trend);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", label), Style::new().fg(theme::FG)),
            Span::styled(spark, Style::new().fg(theme::ACCENT)),
            Span::styled(format!("  {:>8}", format_mb(current)), Style::new().fg(theme::FG)),
            Span::raw(" "),
            Span::styled(arrow.to_string(), Style::new().fg(arrow_color)),
        ]),
    ];
    if min != max {
        lines.push(Line::from(Span::styled(
            format!(" {:>12}{}-{}", "", format_mb(min), format_mb(max)),
            Style::new().fg(theme::MUTED),
        )));
    }
    lines.push(Line::raw(""));
    ListItem::new(lines)
}

fn cpu_item(data: &[f32], spark_width: usize) -> ListItem<'static> {
    let current = data.last().copied().unwrap_or(0.0);
    let (spark, min, max) = sparkline_str_f32(data, spark_width);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", "CPU"), Style::new().fg(theme::FG)),
            Span::styled(spark, Style::new().fg(theme::GREEN)),
            Span::styled(format!("  {:>7.1}%", current), Style::new().fg(theme::FG)),
        ]),
    ];
    if (max - min) >= 0.01 {
        lines.push(Line::from(Span::styled(
            format!(" {:>12}{:.1}-{:.1}%", "", min, max),
            Style::new().fg(theme::MUTED),
        )));
    }
    lines.push(Line::raw(""));
    ListItem::new(lines)
}

fn percent_item(label: &str, data: &[f32], color: ratatui::style::Color, spark_width: usize) -> ListItem<'static> {
    let current = data.last().copied().unwrap_or(0.0);
    let (spark, min, max) = sparkline_str_f32(data, spark_width);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", label), Style::new().fg(theme::FG)),
            Span::styled(spark, Style::new().fg(color)),
            Span::styled(format!("  {:>7.1}%", current), Style::new().fg(theme::FG)),
        ]),
    ];
    if (max - min) >= 0.01 {
        lines.push(Line::from(Span::styled(
            format!(" {:>12}{:.1}-{:.1}%", "", min, max),
            Style::new().fg(theme::MUTED),
        )));
    }
    lines.push(Line::raw(""));
    ListItem::new(lines)
}

fn frames_item(data: &[u64], spark_width: usize) -> ListItem<'static> {
    let current = data.last().copied().unwrap_or(0);
    let (spark, min, max) = sparkline_str(data, spark_width);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", "Rendered"), Style::new().fg(theme::FG)),
            Span::styled(spark, Style::new().fg(theme::MAGENTA)),
            Span::styled(format!("  {:>8}", current), Style::new().fg(theme::FG)),
        ]),
    ];
    if min != max {
        lines.push(Line::from(Span::styled(
            format!(" {:>12}{}-{}", "", min, max),
            Style::new().fg(theme::MUTED),
        )));
    }
    lines.push(Line::raw(""));
    ListItem::new(lines)
}


fn sub_block(title: &str) -> Block<'static> {
    let color = panel::by_number(panel::MONITOR).dim_color;
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(Span::styled(
            format!(" {title} "),
            Style::new().fg(color),
        )))
        .border_style(Style::new().fg(color))
}

pub fn render_monitor_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &MonitorState,
) {
    let block = panel_block(panel::MONITOR, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.history.is_empty() {
        let items = vec![Line::styled(" waiting...", Style::new().fg(theme::MUTED))];
        frame.render_widget(List::new(items), inner);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(inner);

    let spark_width = (inner.width as usize).saturating_sub(27).max(5);

    let rss_data = state.sparkline_u64(|m| m.rss_kb);
    let cpu_data = state.sparkline_f32(|m| m.cpu_percent);
    let data_kb_data = state.sparkline_u64(|m| m.data_kb);
    let cache_kb_data = state.sparkline_u64(|m| m.cache_kb);

    let frames_block = sub_block("frames");
    let frames_inner = frames_block.inner(chunks[0]);
    frame.render_widget(frames_block, chunks[0]);
    frame.render_widget(List::new(vec![
        percent_item("Slow", &state.slow_percent_history, theme::YELLOW, spark_width),
        percent_item("Frozen", &state.frozen_percent_history, theme::RED, spark_width),
        frames_item(&state.frame_count_history, spark_width),
    ]), frames_inner);

    let disk_block = sub_block("disk");
    let disk_inner = disk_block.inner(chunks[1]);
    frame.render_widget(disk_block, chunks[1]);
    frame.render_widget(List::new(vec![
        mem_item("Data", &data_kb_data, state.trend_u64(|m| m.data_kb), spark_width),
        mem_item("Cache", &cache_kb_data, state.trend_u64(|m| m.cache_kb), spark_width),
    ]), disk_inner);

    let cpu_block = sub_block("cpu");
    let cpu_inner = cpu_block.inner(chunks[2]);
    frame.render_widget(cpu_block, chunks[2]);
    frame.render_widget(List::new(vec![
        cpu_item(&cpu_data, spark_width),
    ]), cpu_inner);

    let mem_block = sub_block("memory");
    let mem_inner = mem_block.inner(chunks[3]);
    frame.render_widget(mem_block, chunks[3]);
    frame.render_widget(List::new(vec![
        mem_item("RSS", &rss_data, state.trend_u64(|m| m.rss_kb), spark_width),
    ]), mem_inner);
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
