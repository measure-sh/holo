use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::monitor::MonitorState;
use crate::panel;
use crate::theme;
use crate::ui::panel_block;

pub(crate) const SPARK_CHARS: [char; 8] = ['▁', '▁', '▂', '▂', '▃', '▃', '▄', '▄'];

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

pub(crate) fn sparkline_str(data: &[u64], width: usize) -> (String, u64, u64) {
    if data.is_empty() {
        return (String::new(), 0, 0);
    }
    let start = data.len().saturating_sub(width);
    let slice = &data[start..];
    let min = *slice.iter().min().unwrap();
    let max = *slice.iter().max().unwrap();
    let range = max.saturating_sub(min);

    let spark: String = slice
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
    let pad = width.saturating_sub(spark.chars().count());
    let s = format!("{}{}", " ".repeat(pad), spark);
    (s, min, max)
}

pub(crate) fn sparkline_str_f32(data: &[f32], width: usize) -> (String, f32, f32) {
    if data.is_empty() {
        return (String::new(), 0.0, 0.0);
    }
    let start = data.len().saturating_sub(width);
    let slice = &data[start..];
    let min = slice.iter().copied().reduce(f32::min).unwrap();
    let max = slice.iter().copied().reduce(f32::max).unwrap();
    let range = max - min;

    let spark: String = slice
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
    let pad = width.saturating_sub(spark.chars().count());
    let s = format!("{}{}", " ".repeat(pad), spark);
    (s, min, max)
}

/// Splits a 2+ row area into a 1-row label area on top and the rest for the chart.
fn split_label_chart(area: Rect) -> (Rect, Rect) {
    let label = Rect { height: 1, ..area };
    let chart = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    (label, chart)
}

fn render_cpu_chart(frame: &mut Frame, area: Rect, data: &[f32]) {
    let t = theme::current();
    let current = data.last().copied().unwrap_or(0.0);
    let (label_area, spark_area) = split_label_chart(area);

    let label = Line::from(Span::styled(
        format!(" CPU {:.1}%", current),
        Style::new().fg(t.fg),
    ));
    frame.render_widget(Paragraph::new(label), label_area);

    if spark_area.height > 0 {
        let width = spark_area.width.saturating_sub(1) as usize;
        let (spark, _, _) = sparkline_str_f32(data, width);
        let spark_line = Line::from(Span::styled(
            format!(" {spark}"),
            Style::new().fg(t.sparkline),
        ));
        frame.render_widget(Paragraph::new(spark_line), spark_area);
    }
}

fn render_mem_chart(frame: &mut Frame, area: Rect, label: &str, data: &[u64]) {
    let t = theme::current();
    let current = data.last().copied().unwrap_or(0);
    let (label_area, spark_area) = split_label_chart(area);

    let label = Line::from(Span::styled(
        format!(" {} {}", label, format_mb(current)),
        Style::new().fg(t.fg),
    ));
    frame.render_widget(Paragraph::new(label), label_area);

    if spark_area.height > 0 {
        let width = spark_area.width.saturating_sub(1) as usize;
        let (spark, _, _) = sparkline_str(data, width);
        let spark_line = Line::from(Span::styled(
            format!(" {spark}"),
            Style::new().fg(t.sparkline),
        ));
        frame.render_widget(Paragraph::new(spark_line), spark_area);
    }
}

fn render_disk_chart(frame: &mut Frame, area: Rect, data: &[u64]) {
    let t = theme::current();
    let current = data.last().copied().unwrap_or(0);
    let (label_area, spark_area) = split_label_chart(area);

    let label = Line::from(Span::styled(
        format!(" Disk {}", format_mb_precise(current)),
        Style::new().fg(t.fg),
    ));
    frame.render_widget(Paragraph::new(label), label_area);

    if spark_area.height > 0 {
        let width = spark_area.width.saturating_sub(1) as usize;
        let (spark, _, _) = sparkline_str(data, width);
        let spark_line = Line::from(Span::styled(
            format!(" {spark}"),
            Style::new().fg(t.sparkline),
        ));
        frame.render_widget(Paragraph::new(spark_line), spark_area);
    }
}

fn render_bar_metric(frame: &mut Frame, area: Rect, label: &str, used_kb: u64, total_kb: u64) {
    let t = theme::current();
    let (label_area, bar_area) = split_label_chart(area);

    let value_str = format!("{}/{}", format_mb(used_kb), format_mb(total_kb));
    let label_line = Line::from(vec![
        Span::styled(format!(" {label}"), Style::new().fg(t.fg)),
        Span::styled(format!(" {value_str}"), Style::new().fg(t.muted)),
    ]);
    frame.render_widget(Paragraph::new(label_line), label_area);

    if bar_area.height > 0 {
        let bar_width = bar_area.width.saturating_sub(1) as usize;
        let filled = if total_kb > 0 {
            ((used_kb as f64 / total_kb as f64) * bar_width as f64).round() as usize
        } else {
            0
        };
        let filled = filled.min(bar_width);
        let empty = bar_width - filled;

        let bar_color = if total_kb > 0 && used_kb * 100 / total_kb >= 85 {
            t.danger
        } else {
            t.sparkline
        };

        let bar_line = Line::from(vec![
            Span::raw(" "),
            Span::styled("█".repeat(filled), Style::new().fg(bar_color)),
            Span::styled("░".repeat(empty), Style::new().fg(t.surface)),
        ]);
        frame.render_widget(Paragraph::new(bar_line), bar_area);
    }
}

fn split_two_columns(area: Rect) -> (Rect, Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    (cols[0], cols[1])
}

fn render_charts(frame: &mut Frame, inner: Rect, state: &MonitorState, measure_sdk: bool) {
    let h = inner.height as usize;
    let cpu_data = state.sparkline_f32(|m| m.cpu_percent);

    if measure_sdk {
        let rss_data = state.sparkline_u64(|m| m.rss_kb);
        let disk_data = state.sparkline_u64(|m| m.data_kb);
        let last = state.history.last().unwrap();
        let java_used = last.java_total_heap_kb.saturating_sub(last.java_free_heap_kb);
        let native_used = last.native_total_heap_kb.saturating_sub(last.native_free_heap_kb);

        if h < 2 {
            // Ultra-compact: single summary line
            let t = theme::current();
            let cpu = cpu_data.last().copied().unwrap_or(0.0);
            let rss = rss_data.last().copied().unwrap_or(0);
            let line = Line::from(Span::styled(
                format!(" CPU {:.1}%  Mem {}", cpu, format_mb(rss)),
                Style::new().fg(t.fg),
            ));
            frame.render_widget(Paragraph::new(line), inner);
        } else {
            // Build vertical layout: CPU+Mem(2), sep(1), Java+Native(2), sep(1), Disk(2)
            let mut constraints: Vec<Constraint> = Vec::new();
            constraints.push(Constraint::Length(2)); // CPU + Mem
            if h >= 5 {
                constraints.push(Constraint::Length(1)); // separator
                constraints.push(Constraint::Length(2)); // Java + Native
            }
            if h >= 7 {
                constraints.push(Constraint::Length(1)); // separator
            }
            if h >= 4 {
                constraints.push(Constraint::Length(2)); // Disk
            }
            if h >= 9 {
                constraints.push(Constraint::Min(0)); // absorb extra space
            }

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(inner);

            let mut idx = 0;

            // CPU + Mem side by side
            let (cpu_area, mem_area) = split_two_columns(rows[idx]);
            render_cpu_chart(frame, cpu_area, &cpu_data);
            render_mem_chart(frame, mem_area, "Mem", &rss_data);
            idx += 1;

            // Java + Native
            if h >= 5 {
                idx += 1; // skip separator
                let (java_area, native_area) = split_two_columns(rows[idx]);
                render_bar_metric(frame, java_area, "Java", java_used, last.java_max_heap_kb);
                render_bar_metric(frame, native_area, "Native", native_used, last.native_total_heap_kb);
                idx += 1;
            }

            // Disk
            if h >= 4 {
                if h >= 7 {
                    idx += 1; // skip separator
                }
                render_disk_chart(frame, rows[idx], &disk_data);
            }
        }
    } else {
        let rss_data = state.sparkline_u64(|m| m.rss_kb);
        let disk_data = state.sparkline_u64(|m| m.data_kb);

        if h < 2 {
            let t = theme::current();
            let cpu = cpu_data.last().copied().unwrap_or(0.0);
            let rss = rss_data.last().copied().unwrap_or(0);
            let line = Line::from(Span::styled(
                format!(" CPU {:.1}%  RSS {}", cpu, format_mb(rss)),
                Style::new().fg(t.fg),
            ));
            frame.render_widget(Paragraph::new(line), inner);
        } else {
            let mut constraints: Vec<Constraint> = Vec::new();
            constraints.push(Constraint::Length(2)); // CPU + Mem
            if h >= 4 {
                constraints.push(Constraint::Length(1)); // separator
            }
            if h >= 3 {
                constraints.push(Constraint::Length(2)); // Disk
            }
            if h >= 6 {
                constraints.push(Constraint::Min(0)); // absorb extra space
            }

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(inner);

            let mut idx = 0;

            // CPU + RSS side by side
            let (cpu_area, mem_area) = split_two_columns(rows[idx]);
            render_cpu_chart(frame, cpu_area, &cpu_data);
            render_mem_chart(frame, mem_area, "RSS", &rss_data);
            idx += 1;

            // Disk
            if h >= 3 {
                if h >= 4 {
                    idx += 1; // skip separator
                }
                render_disk_chart(frame, rows[idx], &disk_data);
            }
        }
    }
}

pub fn render_monitor_panel(frame: &mut Frame, area: Rect, focused: bool, state: &MonitorState, measure_sdk: bool) {
    let t = theme::current();
    let block = panel_block(panel::MONITOR, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.history.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(" waiting...", Style::new().fg(t.muted))),
            inner,
        );
        return;
    }

    if !state.debuggable {
        frame.render_widget(
            Paragraph::new(Line::styled(" not debuggable", Style::new().fg(t.muted))),
            inner,
        );
        return;
    }

    render_charts(frame, inner, state, measure_sdk);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_single_value() {
        let (s, _, _) = sparkline_str(&[100], 10);
        // padded to width 10: 9 spaces + 1 spark char
        assert_eq!(s.chars().count(), 10);
        assert_eq!(s.trim_start().chars().count(), 1);
    }

    #[test]
    fn sparkline_constant_values() {
        let (s, min, max) = sparkline_str(&[50, 50, 50], 10);
        // padded to width 10: 7 spaces + 3 spark chars
        assert!(s.trim_start().chars().all(|c| c == SPARK_CHARS[3]));
        assert_eq!(min, max);
    }

    #[test]
    fn sparkline_range_values() {
        let (s, min, max) = sparkline_str(&[0, 100], 10);
        let chars: Vec<char> = s.chars().collect();
        // 8 spaces of padding, then 2 spark chars
        assert_eq!(chars[8], SPARK_CHARS[0]);
        assert_eq!(chars[9], SPARK_CHARS[7]);
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
        // 7 spaces of padding, then 3 spark chars
        assert_eq!(chars[7], SPARK_CHARS[0]);
        assert_eq!(chars[9], SPARK_CHARS[7]);
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
