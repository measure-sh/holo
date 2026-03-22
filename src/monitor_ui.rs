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

fn format_bytes_per_sec(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB/s", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB/s", bytes as f64 / 1024.0)
    } else {
        format!("{} B/s", bytes)
    }
}

fn trend_symbol(trend: Trend) -> (&'static str, ratatui::style::Color) {
    match trend {
        Trend::Rising => ("▲", theme::RED),
        Trend::Falling => ("▼", theme::GREEN),
        Trend::Stable => ("─", theme::MUTED),
    }
}

fn sparkline_str(data: &[u64], width: usize) -> String {
    if data.is_empty() {
        return String::new();
    }
    let start = data.len().saturating_sub(width);
    let slice = &data[start..];
    let min = *slice.iter().min().unwrap();
    let max = *slice.iter().max().unwrap();
    let range = max.saturating_sub(min);

    slice
        .iter()
        .map(|&v| {
            if range == 0 {
                SPARK_CHARS[3]
            } else {
                let idx = ((v - min) * 7 / range).min(7) as usize;
                SPARK_CHARS[idx]
            }
        })
        .collect()
}

fn sparkline_str_f32(data: &[f32], width: usize) -> String {
    if data.is_empty() {
        return String::new();
    }
    let start = data.len().saturating_sub(width);
    let slice = &data[start..];
    let min = slice.iter().copied().reduce(f32::min).unwrap();
    let max = slice.iter().copied().reduce(f32::max).unwrap();
    let range = max - min;

    slice
        .iter()
        .map(|&v| {
            if range < 0.01 {
                SPARK_CHARS[3]
            } else {
                let idx = (((v - min) / range * 7.0) as usize).min(7);
                SPARK_CHARS[idx]
            }
        })
        .collect()
}

fn mem_item(
    label: &str,
    data: &[u64],
    trend: Trend,
    spark_width: usize,
) -> ListItem<'static> {
    let current = data.last().copied().unwrap_or(0);
    let spark = sparkline_str(data, spark_width);
    let (arrow, arrow_color) = trend_symbol(trend);

    ListItem::new(vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", label), Style::new().fg(theme::FG)),
            Span::styled(spark, Style::new().fg(theme::ACCENT)),
            Span::styled(format!("  {:>8}", format_mb(current)), Style::new().fg(theme::FG)),
            Span::raw(" "),
            Span::styled(arrow.to_string(), Style::new().fg(arrow_color)),
        ]),
        Line::raw(""),
    ])
}

fn cpu_item(data: &[f32], spark_width: usize) -> ListItem<'static> {
    let current = data.last().copied().unwrap_or(0.0);
    let spark = sparkline_str_f32(data, spark_width);

    ListItem::new(vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", "CPU"), Style::new().fg(theme::FG)),
            Span::styled(spark, Style::new().fg(theme::GREEN)),
            Span::styled(format!("  {:>7.1}%", current), Style::new().fg(theme::FG)),
        ]),
        Line::raw(""),
    ])
}

fn jank_item(data: &[f32], spark_width: usize) -> ListItem<'static> {
    let current = data.last().copied().unwrap_or(0.0);
    let spark = sparkline_str_f32(data, spark_width);

    ListItem::new(vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", "Janky"), Style::new().fg(theme::FG)),
            Span::styled(spark, Style::new().fg(theme::RED)),
            Span::styled(format!("  {:>7.1}%", current), Style::new().fg(theme::FG)),
        ]),
        Line::raw(""),
    ])
}

fn frames_item(data: &[u64], spark_width: usize) -> ListItem<'static> {
    let current = data.last().copied().unwrap_or(0);
    let spark = sparkline_str(data, spark_width);

    ListItem::new(vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", "Rendered"), Style::new().fg(theme::FG)),
            Span::styled(spark, Style::new().fg(theme::MAGENTA)),
            Span::styled(format!("  {:>8}", current), Style::new().fg(theme::FG)),
        ]),
        Line::raw(""),
    ])
}

fn net_item(
    label: &str,
    data: &[u64],
    color: ratatui::style::Color,
    spark_width: usize,
) -> ListItem<'static> {
    let current = data.last().copied().unwrap_or(0);
    let spark = sparkline_str(data, spark_width);

    ListItem::new(vec![
        Line::from(vec![
            Span::styled(format!(" {:<12}", label), Style::new().fg(theme::FG)),
            Span::styled(spark, Style::new().fg(color)),
            Span::styled(format!("  {:>8}", format_bytes_per_sec(current)), Style::new().fg(theme::FG)),
        ]),
        Line::raw(""),
    ])
}

fn section_header(label: &str, first: bool) -> ListItem<'static> {
    let label_line = Line::from(Span::styled(
        format!(" {}", label),
        Style::new().fg(theme::MUTED),
    ));
    if first {
        ListItem::new(vec![label_line])
    } else {
        ListItem::new(vec![Line::raw(""), label_line])
    }
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

    let spark_width = (inner.width as usize).saturating_sub(25).max(5);

    let total_data = state.sparkline_u64(|m| m.total_pss_kb);
    let java_data = state.sparkline_u64(|m| m.java_heap_kb);
    let native_data = state.sparkline_u64(|m| m.native_heap_kb);
    let cpu_data = state.sparkline_f32(|m| m.cpu_percent);

    let items: Vec<ListItem> = vec![
        section_header("── memory", true),
        mem_item("Total PSS", &total_data, state.trend_u64(|m| m.total_pss_kb), spark_width),
        mem_item("Java Heap", &java_data, state.trend_u64(|m| m.java_heap_kb), spark_width),
        mem_item("Native", &native_data, state.trend_u64(|m| m.native_heap_kb), spark_width),
        section_header("── cpu", false),
        cpu_item(&cpu_data, spark_width),
        section_header("── network (device)", false),
        net_item("↓ down", &state.download_history, theme::CYAN, spark_width),
        net_item("↑ up", &state.upload_history, theme::YELLOW, spark_width),
        section_header("── frames", false),
        jank_item(&state.janky_percent_history, spark_width),
        frames_item(&state.frame_count_history, spark_width),
    ];

    frame.render_widget(List::new(items), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_single_value() {
        let s = sparkline_str(&[100], 10);
        assert_eq!(s.chars().count(), 1);
    }

    #[test]
    fn sparkline_constant_values() {
        let s = sparkline_str(&[50, 50, 50], 10);
        assert!(s.chars().all(|c| c == SPARK_CHARS[3]));
    }

    #[test]
    fn sparkline_range_values() {
        let s = sparkline_str(&[0, 100], 10);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars[0], SPARK_CHARS[0]);
        assert_eq!(chars[1], SPARK_CHARS[7]);
    }

    #[test]
    fn sparkline_caps_to_width() {
        let data: Vec<u64> = (0..20).collect();
        let s = sparkline_str(&data, 5);
        assert_eq!(s.chars().count(), 5);
    }

    #[test]
    fn sparkline_f32_range() {
        let s = sparkline_str_f32(&[0.0, 50.0, 100.0], 10);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars[0], SPARK_CHARS[0]);
        assert_eq!(chars[2], SPARK_CHARS[7]);
    }

    #[test]
    fn format_mb_small() {
        assert_eq!(format_mb(512), "0.5 MB");
    }

    #[test]
    fn format_mb_large() {
        assert_eq!(format_mb(128000), "125 MB");
    }

    #[test]
    fn format_bytes_per_sec_kb() {
        assert_eq!(format_bytes_per_sec(2048), "2 KB/s");
    }

    #[test]
    fn format_bytes_per_sec_mb() {
        assert_eq!(format_bytes_per_sec(2_097_152), "2.0 MB/s");
    }

    #[test]
    fn format_bytes_per_sec_bytes() {
        assert_eq!(format_bytes_per_sec(42), "42 B/s");
    }
}
