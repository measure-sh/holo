use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::List,
    Frame,
};

use crate::memory::{MemoryState, Trend};
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

fn metric_line(
    label: &str,
    data: &[u64],
    trend: Trend,
    spark_width: usize,
) -> Line<'static> {
    let current = data.last().copied().unwrap_or(0);
    let spark = sparkline_str(data, spark_width);
    let (arrow, arrow_color) = trend_symbol(trend);

    Line::from(vec![
        Span::styled(format!(" {:<12}", label), Style::new().fg(theme::FG)),
        Span::styled(spark, Style::new().fg(theme::ACCENT)),
        Span::styled(format!("  {:>8}", format_mb(current)), Style::new().fg(theme::FG)),
        Span::raw(" "),
        Span::styled(arrow.to_string(), Style::new().fg(arrow_color)),
    ])
}

pub fn render_memory_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &MemoryState,
) {
    let block = panel_block(panel::MEMORY, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.history.is_empty() {
        let items = vec![Line::styled(" waiting...", Style::new().fg(theme::MUTED))];
        frame.render_widget(List::new(items), inner);
        return;
    }

    let spark_width = (inner.width as usize).saturating_sub(25).max(5);

    let total_data = state.sparkline_data(|m| m.total_pss_kb);
    let java_data = state.sparkline_data(|m| m.java_heap_kb);
    let native_data = state.sparkline_data(|m| m.native_heap_kb);

    let items = vec![
        metric_line("Total PSS", &total_data, state.trend(|m| m.total_pss_kb), spark_width),
        metric_line("Java Heap", &java_data, state.trend(|m| m.java_heap_kb), spark_width),
        metric_line("Native", &native_data, state.trend(|m| m.native_heap_kb), spark_width),
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
    fn format_mb_small() {
        assert_eq!(format_mb(512), "0.5 MB");
    }

    #[test]
    fn format_mb_large() {
        assert_eq!(format_mb(128000), "125 MB");
    }
}
