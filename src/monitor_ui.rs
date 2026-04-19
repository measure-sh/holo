use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::monitor::MonitorState;
use crate::panel;
use crate::theme;
use crate::ui::panel_block;

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

/// Render the last `width` values of `samples` as unicode sparkline characters.
/// Right-aligned: the most recent sample is the rightmost character.
fn sparkline(samples: &[f64], width: usize) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if width == 0 {
        return String::new();
    }
    let visible: Vec<f64> = samples.iter().rev().take(width).copied().collect();
    let max = visible.iter().copied().fold(0.0_f64, f64::max);
    let pad = width.saturating_sub(visible.len());
    let mut out = String::with_capacity(width);
    for _ in 0..pad {
        out.push(' ');
    }
    for &v in visible.iter().rev() {
        let idx = if max <= 0.0 {
            0
        } else {
            ((v / max) * (BLOCKS.len() - 1) as f64).round() as usize
        };
        out.push(BLOCKS[idx.min(BLOCKS.len() - 1)]);
    }
    out
}

fn metric_line<'a>(
    label: String,
    label_width: usize,
    samples: &[f64],
    width: usize,
    text_style: Style,
    spark_style: Style,
) -> Line<'a> {
    let label_cols = label.chars().count();
    let pad = label_width.saturating_sub(label_cols);
    let padded = format!("{}{}", label, " ".repeat(pad));
    let spark_cols = width.saturating_sub(padded.chars().count());
    let spark = sparkline(samples, spark_cols);
    Line::from(vec![
        Span::styled(padded, text_style),
        Span::styled(spark, spark_style),
    ])
}

fn render_charts(frame: &mut Frame, inner: Rect, state: &MonitorState, measure_sdk: bool) {
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let t = theme::current();
    let text_style = Style::new().fg(t.fg);
    let spark_style = Style::new().fg(t.sparkline);
    let width = inner.width as usize;

    let cpu_series: Vec<f64> = state.history.iter().map(|m| m.cpu_percent as f64).collect();
    let cpu_now = cpu_series.last().copied().unwrap_or(0.0);

    let rss_series: Vec<f64> = state.history.iter().map(|m| m.rss_kb as f64).collect();
    let rss_now = rss_series.last().copied().unwrap_or(0.0) as u64;
    let rss_label = if measure_sdk { "Mem" } else { "RSS" };

    let disk_series: Vec<f64> = state.history.iter().map(|m| m.data_kb as f64).collect();
    let disk_now = disk_series.last().copied().unwrap_or(0.0) as u64;

    let mut rows: Vec<(String, Vec<f64>)> = Vec::new();
    rows.push((format!(" CPU {:.1}%  ", cpu_now), cpu_series));
    rows.push((format!(" {} {}  ", rss_label, format_mb(rss_now)), rss_series));

    if measure_sdk {
        let last = state.history.last().copied().unwrap_or_default();
        let java_used_now = last.java_total_heap_kb.saturating_sub(last.java_free_heap_kb);
        let java_series: Vec<f64> = state
            .history
            .iter()
            .map(|m| m.java_total_heap_kb.saturating_sub(m.java_free_heap_kb) as f64)
            .collect();
        rows.push((
            format!(" Java {}/{}  ", format_mb(java_used_now), format_mb(last.java_max_heap_kb)),
            java_series,
        ));

        let native_used_now = last
            .native_total_heap_kb
            .saturating_sub(last.native_free_heap_kb);
        let native_series: Vec<f64> = state
            .history
            .iter()
            .map(|m| m.native_total_heap_kb.saturating_sub(m.native_free_heap_kb) as f64)
            .collect();
        rows.push((
            format!(
                " Native {}/{}  ",
                format_mb(native_used_now),
                format_mb(last.native_total_heap_kb)
            ),
            native_series,
        ));
    }

    rows.push((format!(" Disk {}  ", format_mb_precise(disk_now)), disk_series));

    let label_width = rows.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0);
    let max_rows = inner.height as usize;
    let lines: Vec<Line> = rows
        .iter()
        .take(max_rows)
        .map(|(label, data)| metric_line(label.clone(), label_width, data, width, text_style, spark_style))
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
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
    fn sparkline_empty_is_empty() {
        assert_eq!(sparkline(&[], 10), "          ");
    }

    #[test]
    fn sparkline_zero_width() {
        assert_eq!(sparkline(&[1.0, 2.0, 3.0], 0), "");
    }

    #[test]
    fn sparkline_pads_short_series() {
        let s = sparkline(&[100.0], 5);
        assert_eq!(s.chars().count(), 5);
        assert_eq!(s.chars().take(4).collect::<String>(), "    ");
    }

    #[test]
    fn sparkline_constant_nonzero_values() {
        let s = sparkline(&[50.0, 50.0, 50.0], 5);
        // all three map to the max bucket (index 7)
        let last_three: String = s.chars().skip(2).collect();
        assert!(last_three.chars().all(|c| c == '█'));
    }

    #[test]
    fn sparkline_all_zero() {
        let s = sparkline(&[0.0, 0.0, 0.0], 3);
        assert_eq!(s, "▁▁▁");
    }

    #[test]
    fn sparkline_range_values() {
        let s = sparkline(&[0.0, 100.0], 2);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars[0], '▁');
        assert_eq!(chars[1], '█');
    }

    #[test]
    fn sparkline_caps_to_width() {
        let data: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let s = sparkline(&data, 5);
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
