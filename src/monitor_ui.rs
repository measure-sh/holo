use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Paragraph, RenderDirection, Sparkline, SparklineBar},
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

/// Range-shift a series so stable non-zero values render as a flat low bar
/// instead of clipping to the top, and zero samples render as absent (e.g.
/// the app wasn't running yet). Returns bars in chronological order plus the
/// `max` to feed the widget.
fn range_shifted(samples: &[u64]) -> (Vec<SparklineBar>, u64) {
    let nonzero: Vec<u64> = samples.iter().copied().filter(|&v| v > 0).collect();
    if nonzero.is_empty() {
        return (samples.iter().map(|_| SparklineBar::from(None)).collect(), 0);
    }
    let min = *nonzero.iter().min().unwrap();
    let max = *nonzero.iter().max().unwrap();
    let data = samples
        .iter()
        .map(|&v| {
            if v == 0 {
                SparklineBar::from(None)
            } else {
                SparklineBar::from(Some(v - min))
            }
        })
        .collect();
    (data, max - min)
}

fn metric_row(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    label_width: u16,
    data: Vec<SparklineBar>,
    max: u64,
    color: Color,
) {
    let t = theme::current();
    let chunks = Layout::horizontal([
        Constraint::Length(label_width),
        Constraint::Min(0),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::styled(label.to_string(), Style::new().fg(t.fg))),
        chunks[0],
    );
    let reversed: Vec<SparklineBar> = data.into_iter().rev().collect();
    let mut spark = Sparkline::default()
        .data(reversed)
        .direction(RenderDirection::RightToLeft)
        .style(Style::new().fg(color))
        .absent_value_symbol("▁")
        .absent_value_style(Style::new().fg(color));
    if max > 0 {
        spark = spark.max(max);
    }
    frame.render_widget(spark, chunks[1]);
}

fn render_charts(frame: &mut Frame, inner: Rect, state: &MonitorState, measure_sdk: bool) {
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let t = theme::current();

    let cpu_now = state.history.last().map(|m| m.cpu_percent).unwrap_or(0.0);
    let cpu_data: Vec<SparklineBar> = state
        .history
        .iter()
        .map(|m| SparklineBar::from(Some(m.cpu_percent.max(0.0).round() as u64)))
        .collect();

    let rss: Vec<u64> = state.history.iter().map(|m| m.rss_kb).collect();
    let rss_now = state.history.last().map(|m| m.rss_kb).unwrap_or(0);
    let rss_label = if measure_sdk { "Mem" } else { "RSS" };
    let (rss_data, rss_max) = range_shifted(&rss);

    let disk: Vec<u64> = state.history.iter().map(|m| m.data_kb).collect();
    let disk_now = state.history.last().map(|m| m.data_kb).unwrap_or(0);
    let (disk_data, disk_max) = range_shifted(&disk);

    let mut rows: Vec<(String, Vec<SparklineBar>, u64, Color)> = Vec::new();
    rows.push((format!(" CPU {:.1}%  ", cpu_now), cpu_data, 100, t.spark_cpu));
    rows.push((format!(" {} {}  ", rss_label, format_mb(rss_now)), rss_data, rss_max, t.spark_mem));

    if measure_sdk {
        let last = state.history.last().copied().unwrap_or_default();
        let java_max = last.java_max_heap_kb;
        let java_used_now = last.java_total_heap_kb.saturating_sub(last.java_free_heap_kb);
        let java_data: Vec<SparklineBar> = state
            .history
            .iter()
            .map(|m| {
                if m.java_max_heap_kb == 0 {
                    SparklineBar::from(None)
                } else {
                    SparklineBar::from(Some(m.java_total_heap_kb.saturating_sub(m.java_free_heap_kb)))
                }
            })
            .collect();
        rows.push((
            format!(" Java {}/{}  ", format_mb(java_used_now), format_mb(java_max)),
            java_data,
            java_max,
            t.spark_java,
        ));

        let native_max = last.native_total_heap_kb;
        let native_used_now = last.native_total_heap_kb.saturating_sub(last.native_free_heap_kb);
        let native_data: Vec<SparklineBar> = state
            .history
            .iter()
            .map(|m| {
                if m.native_total_heap_kb == 0 {
                    SparklineBar::from(None)
                } else {
                    SparklineBar::from(Some(m.native_total_heap_kb.saturating_sub(m.native_free_heap_kb)))
                }
            })
            .collect();
        rows.push((
            format!(" Native {}/{}  ", format_mb(native_used_now), format_mb(native_max)),
            native_data,
            native_max,
            t.spark_native,
        ));
    }

    rows.push((format!(" Disk {}  ", format_mb_precise(disk_now)), disk_data, disk_max, t.spark_disk));

    let label_width = rows.iter().map(|(l, _, _, _)| l.chars().count()).max().unwrap_or(0) as u16;
    let visible_rows = rows.len().min(inner.height as usize);
    if visible_rows == 0 {
        return;
    }
    let constraints: Vec<Constraint> = (0..visible_rows).map(|_| Constraint::Length(1)).collect();
    let chunks = Layout::vertical(constraints).split(inner);

    for (i, (label, data, max, color)) in rows.into_iter().take(visible_rows).enumerate() {
        metric_row(frame, chunks[i], &label, label_width, data, max, color);
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
    fn format_mb_small() {
        assert_eq!(format_mb(512), "0.5 MB");
    }

    #[test]
    fn format_mb_large() {
        assert_eq!(format_mb(128000), "125 MB");
    }
}
