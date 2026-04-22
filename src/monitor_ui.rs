use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Chart, Dataset, GraphType, Paragraph, RenderDirection, Sparkline, SparklineBar,
    },
    Frame,
};

/// Sparkline glyphs with a visible `▁` baseline instead of the default blank
/// `empty`, so flat or zero stretches still render as a continuous low line.
const BASELINE_BARS: symbols::bar::Set = symbols::bar::Set {
    empty: symbols::bar::ONE_EIGHTH,
    ..symbols::bar::NINE_LEVELS
};

use crate::monitor::MonitorState;
use crate::network::TrafficSample;
use crate::panel;
use crate::theme;
use crate::ui::{panel_block, render_pane_chip, split_chip};

/// Minimum height per stacked chart in the detail view (chip row + chart body).
/// Below this, fall back to the compact list.
const STACKED_MIN_HEIGHT: u16 = 5;

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

fn format_rate(bps: u64) -> String {
    if bps < 1024 {
        format!("{} B/s", bps)
    } else if bps < 1024 * 1024 {
        format!("{:.1} KB/s", bps as f64 / 1024.0)
    } else {
        format!("{:.1} MB/s", bps as f64 / (1024.0 * 1024.0))
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    }
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

#[derive(Clone, Copy, PartialEq)]
enum MetricScale {
    Percent,
    MemKb,
    DiskKb,
    RateBps,
}

struct Metric {
    name: &'static str,
    label: String,
    sparkline_data: Vec<SparklineBar>,
    sparkline_max: u64,
    /// One value per history sample, in chronological order. Zero means
    /// "absent" for MemKb/DiskKb (app wasn't running); genuine for RateBps
    /// and Percent.
    values: Vec<f64>,
    /// Seconds between consecutive samples; used to label the X axis.
    sample_interval_s: u64,
    color: Color,
    scale: MetricScale,
    /// Cumulative bytes transferred since holo started watching. Only set
    /// for network rx/tx; None for CPU/mem/disk.
    total_bytes: Option<u64>,
}

fn format_scale(value: f64, scale: MetricScale) -> String {
    match scale {
        MetricScale::Percent => format!("{:.1}%", value),
        MetricScale::MemKb => format_mb(value.max(0.0).round() as u64),
        MetricScale::DiskKb => format_mb_precise(value.max(0.0).round() as u64),
        MetricScale::RateBps => format_rate(value.max(0.0).round() as u64),
    }
}

fn format_ago(secs: u64) -> String {
    if secs >= 60 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 { format!("{}m", m) } else { format!("{}m{}s", m, s) }
    } else {
        format!("{}s", secs)
    }
}

fn build_metrics(
    state: &MonitorState,
    traffic: &[TrafficSample],
    traffic_baseline: Option<(u64, u64)>,
    measure_sdk: bool,
) -> Vec<Metric> {
    let t = theme::current();
    let mut metrics = Vec::new();

    let cpu_now = state.history.last().map(|m| m.cpu_percent).unwrap_or(0.0);
    let cpu_values: Vec<f64> = state.history.iter().map(|m| m.cpu_percent.max(0.0) as f64).collect();
    let cpu_spark: Vec<SparklineBar> = state
        .history
        .iter()
        .map(|m| SparklineBar::from(Some(m.cpu_percent.max(0.0).round() as u64)))
        .collect();
    metrics.push(Metric {
        name: "CPU",
        label: format!(" CPU {:.1}%  ", cpu_now),
        sparkline_data: cpu_spark,
        sparkline_max: 100,
        values: cpu_values,
        sample_interval_s: 1,
        color: t.spark_cpu,
        scale: MetricScale::Percent,
        total_bytes: None,
    });

    let (mem_name, mem_label_prefix, mem_samples) = if measure_sdk {
        let s: Vec<u64> = state.history.iter().map(|m| m.total_pss_kb).collect();
        ("PSS", "PSS", s)
    } else {
        let s: Vec<u64> = state.history.iter().map(|m| m.rss_kb).collect();
        ("RSS", "RSS", s)
    };
    let mem_now = *mem_samples.last().unwrap_or(&0);
    let (mem_spark, mem_max) = range_shifted(&mem_samples);
    metrics.push(Metric {
        name: mem_name,
        label: format!(" {} {}  ", mem_label_prefix, format_mb(mem_now)),
        sparkline_data: mem_spark,
        sparkline_max: mem_max,
        values: mem_samples.iter().map(|&v| v as f64).collect(),
        sample_interval_s: 1,
        color: t.spark_mem,
        scale: MetricScale::MemKb,
        total_bytes: None,
    });

    let disk_samples: Vec<u64> = state.history.iter().map(|m| m.data_kb).collect();
    let disk_now = *disk_samples.last().unwrap_or(&0);
    let (disk_spark, disk_max) = range_shifted(&disk_samples);
    metrics.push(Metric {
        name: "Disk",
        label: format!(" Disk {}  ", format_mb_precise(disk_now)),
        sparkline_data: disk_spark,
        sparkline_max: disk_max,
        values: disk_samples.iter().map(|&v| v as f64).collect(),
        sample_interval_s: 1,
        color: t.spark_disk,
        scale: MetricScale::DiskKb,
        total_bytes: None,
    });

    if !traffic.is_empty() {
        let last = *traffic.last().unwrap();
        let rx_total = traffic_baseline
            .map(|(b, _)| last.rx_total.saturating_sub(b))
            .unwrap_or(0);
        let tx_total = traffic_baseline
            .map(|(_, b)| last.tx_total.saturating_sub(b))
            .unwrap_or(0);
        let rx_samples: Vec<SparklineBar> = traffic
            .iter()
            .map(|s| SparklineBar::from(Some(s.rx_bps)))
            .collect();
        let tx_samples: Vec<SparklineBar> = traffic
            .iter()
            .map(|s| SparklineBar::from(Some(s.tx_bps)))
            .collect();
        let rx_max = traffic.iter().map(|s| s.rx_bps).max().unwrap_or(0);
        let tx_max = traffic.iter().map(|s| s.tx_bps).max().unwrap_or(0);
        metrics.push(Metric {
            name: "↓",
            label: format!(" ↓ {} ({})  ", format_rate(last.rx_bps), format_bytes(rx_total)),
            sparkline_data: rx_samples,
            sparkline_max: rx_max,
            values: traffic.iter().map(|s| s.rx_bps as f64).collect(),
            sample_interval_s: 2,
            color: t.spark_rx,
            scale: MetricScale::RateBps,
            total_bytes: Some(rx_total),
        });
        metrics.push(Metric {
            name: "↑",
            label: format!(" ↑ {} ({})  ", format_rate(last.tx_bps), format_bytes(tx_total)),
            sparkline_data: tx_samples,
            sparkline_max: tx_max,
            values: traffic.iter().map(|s| s.tx_bps as f64).collect(),
            sample_interval_s: 2,
            color: t.spark_tx,
            scale: MetricScale::RateBps,
            total_bytes: Some(tx_total),
        });
    }

    metrics
}

fn sparkline_row(frame: &mut Frame, area: Rect, metric: &Metric, label_width: u16) {
    let t = theme::current();
    let chunks = Layout::horizontal([
        Constraint::Length(label_width),
        Constraint::Min(0),
    ])
    .split(area);
    let label_style = Style::new().fg(t.fg);
    frame.render_widget(
        Paragraph::new(Line::styled(metric.label.clone(), label_style)),
        chunks[0],
    );
    let reversed: Vec<SparklineBar> = metric.sparkline_data.iter().rev().cloned().collect();
    let mut spark = Sparkline::default()
        .data(reversed)
        .direction(RenderDirection::RightToLeft)
        .style(Style::new().fg(metric.color))
        .bar_set(BASELINE_BARS)
        .absent_value_symbol(symbols::bar::ONE_EIGHTH)
        .absent_value_style(Style::new().fg(metric.color));
    if metric.sparkline_max > 0 {
        spark = spark.max(metric.sparkline_max);
    }
    frame.render_widget(spark, chunks[1]);
}

fn render_compact_list(frame: &mut Frame, area: Rect, metrics: &[Metric]) {
    let visible_rows = metrics.len().min(area.height as usize);
    if visible_rows == 0 {
        return;
    }
    let label_width = metrics.iter().map(|m| m.label.chars().count()).max().unwrap_or(0) as u16;
    let constraints: Vec<Constraint> = (0..visible_rows).map(|_| Constraint::Length(1)).collect();
    let chunks = Layout::vertical(constraints).split(area);

    for (i, metric) in metrics.iter().take(visible_rows).enumerate() {
        sparkline_row(frame, chunks[i], metric, label_width);
    }
}

fn render_metric_detail(frame: &mut Frame, area: Rect, metric: &Metric, focused: bool) {
    let t = theme::current();
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (chip_area, body) = split_chip(area);
    render_pane_chip(frame, chip_area, metric.name, focused, false);

    let (current, min, max, avg) = stats(metric);
    let stats_line = Line::from(vec![
        Span::styled(format_scale(current, metric.scale), Style::new().fg(metric.color).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("  min {}", format_scale(min, metric.scale)),
            Style::new().fg(t.muted),
        ),
        Span::styled(
            format!("  max {}", format_scale(max, metric.scale)),
            Style::new().fg(t.muted),
        ),
        Span::styled(
            format!("  avg {}", format_scale(avg, metric.scale)),
            Style::new().fg(t.muted),
        ),
    ])
    .alignment(Alignment::Right);
    frame.render_widget(stats_line, chip_area);

    if body.height < 3 || body.width < 8 {
        return;
    }
    render_chart(frame, body, metric, min, max);
}

/// Current/min/max/avg over the metric's values. For MemKb/DiskKb, zero
/// samples are excluded (they represent "app wasn't running"). For Percent
/// and RateBps, zero is a legitimate value and is included.
fn stats(metric: &Metric) -> (f64, f64, f64, f64) {
    let skip_zero = matches!(metric.scale, MetricScale::MemKb | MetricScale::DiskKb);
    let filtered: Vec<f64> = metric
        .values
        .iter()
        .copied()
        .filter(|v| !skip_zero || *v > 0.0)
        .collect();
    let current = *metric.values.last().unwrap_or(&0.0);
    if filtered.is_empty() {
        return (current, 0.0, 0.0, 0.0);
    }
    let min = filtered.iter().copied().fold(f64::INFINITY, f64::min);
    let max = filtered.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let avg = filtered.iter().sum::<f64>() / filtered.len() as f64;
    (current, min, max, avg)
}

fn render_chart(frame: &mut Frame, area: Rect, metric: &Metric, min: f64, max: f64) {
    let t = theme::current();
    let skip_zero = matches!(metric.scale, MetricScale::MemKb | MetricScale::DiskKb);
    let points: Vec<(f64, f64)> = metric
        .values
        .iter()
        .enumerate()
        .filter(|&(_, &v)| !skip_zero || v > 0.0)
        .map(|(i, &v)| (i as f64, v))
        .collect();

    if points.len() < 2 {
        // Not enough data to plot a line.
        let hint = Paragraph::new(Line::styled(
            " gathering samples…",
            Style::new().fg(t.muted),
        ));
        frame.render_widget(hint, area);
        return;
    }

    let (y_lo, y_hi) = match metric.scale {
        MetricScale::Percent => (0.0, 100.0),
        _ => {
            // Pad the bounds a touch so the line isn't flush with the frame.
            let span = (max - min).max(1.0);
            let pad = span * 0.1;
            ((min - pad).max(0.0), max + pad)
        }
    };

    let x_max = (metric.values.len().saturating_sub(1)) as f64;
    let y_mid = (y_lo + y_hi) / 2.0;
    let y_labels = vec![
        Line::from(format_scale(y_lo, metric.scale)),
        Line::from(format_scale(y_mid, metric.scale)),
        Line::from(format_scale(y_hi, metric.scale)),
    ];

    // X labels: total duration ago → half ago → now. Width check avoids
    // cramped middle labels when the detail pane is narrow.
    let total_secs = (metric.values.len().saturating_sub(1)) as u64 * metric.sample_interval_s;
    let x_labels = if area.width >= 36 && total_secs >= 2 {
        vec![
            Line::from(format_ago(total_secs)),
            Line::from(format_ago(total_secs / 2)),
            Line::from("now"),
        ]
    } else {
        vec![Line::from(format_ago(total_secs)), Line::from("now")]
    };

    let dataset = Dataset::default()
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::new().fg(metric.color))
        .data(&points);

    let chart = Chart::new(vec![dataset])
        .style(Style::new().bg(t.bg))
        .x_axis(
            Axis::default()
                .style(Style::new().fg(t.muted))
                .bounds([0.0, x_max.max(1.0)])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .style(Style::new().fg(t.muted))
                .bounds([y_lo, y_hi])
                .labels(y_labels)
                .labels_alignment(Alignment::Right),
        );

    frame.render_widget(chart, area);
}

fn render_charts(
    frame: &mut Frame,
    inner: Rect,
    state: &MonitorState,
    traffic: &[TrafficSample],
    traffic_baseline: Option<(u64, u64)>,
    measure_sdk: bool,
    focused: bool,
    zoomed: bool,
) {
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let metrics = build_metrics(state, traffic, traffic_baseline, measure_sdk);
    if metrics.is_empty() {
        return;
    }

    let slots = detail_slots(&metrics);
    let gap = slots.len().saturating_sub(1) as u16;
    let stacked_fits = inner.height.saturating_sub(gap) / slots.len() as u16 >= STACKED_MIN_HEIGHT;
    if !zoomed || !stacked_fits {
        render_compact_list(frame, inner, &metrics);
        return;
    }

    render_stacked_charts(frame, inner, &slots, focused);
}

enum DetailSlot<'a> {
    Single(&'a Metric),
    Pair(&'a Metric, &'a Metric),
}

/// Group metrics for the stacked detail view: consecutive RateBps metrics
/// (rx/tx) render together in one combined chart.
fn detail_slots(metrics: &[Metric]) -> Vec<DetailSlot<'_>> {
    let mut slots = Vec::new();
    let mut i = 0;
    while i < metrics.len() {
        if i + 1 < metrics.len()
            && metrics[i].scale == MetricScale::RateBps
            && metrics[i + 1].scale == MetricScale::RateBps
        {
            slots.push(DetailSlot::Pair(&metrics[i], &metrics[i + 1]));
            i += 2;
        } else {
            slots.push(DetailSlot::Single(&metrics[i]));
            i += 1;
        }
    }
    slots
}

fn render_stacked_charts(frame: &mut Frame, area: Rect, slots: &[DetailSlot<'_>], focused: bool) {
    let constraints: Vec<Constraint> = slots.iter().map(|_| Constraint::Fill(1)).collect();
    let chunks = Layout::vertical(constraints).spacing(1).split(area);
    for (i, slot) in slots.iter().enumerate() {
        match slot {
            DetailSlot::Single(metric) => render_metric_detail(frame, chunks[i], metric, focused),
            DetailSlot::Pair(rx, tx) => render_pair_detail(frame, chunks[i], rx, tx, focused),
        }
    }
}

fn render_pair_detail(
    frame: &mut Frame,
    area: Rect,
    rx: &Metric,
    tx: &Metric,
    focused: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let t = theme::current();
    let (chip_area, body) = split_chip(area);
    render_pane_chip(frame, chip_area, "Network", focused, false);

    let rx_current = *rx.values.last().unwrap_or(&0.0);
    let tx_current = *tx.values.last().unwrap_or(&0.0);
    let rx_total = rx.total_bytes.unwrap_or(0);
    let tx_total = tx.total_bytes.unwrap_or(0);
    let stats_line = Line::from(vec![
        Span::styled(
            format!("{} {}", rx.name, format_scale(rx_current, rx.scale)),
            Style::new().fg(rx.color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({})", format_bytes(rx_total)),
            Style::new().fg(t.muted),
        ),
        Span::raw("   "),
        Span::styled(
            format!("{} {}", tx.name, format_scale(tx_current, tx.scale)),
            Style::new().fg(tx.color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({})", format_bytes(tx_total)),
            Style::new().fg(t.muted),
        ),
    ])
    .alignment(Alignment::Right);
    frame.render_widget(stats_line, chip_area);

    if body.height < 3 || body.width < 8 {
        return;
    }
    render_combined_chart(frame, body, rx, tx);
}

fn render_combined_chart(frame: &mut Frame, area: Rect, rx: &Metric, tx: &Metric) {
    let t = theme::current();
    let rx_points: Vec<(f64, f64)> = rx
        .values
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();
    let tx_points: Vec<(f64, f64)> = tx
        .values
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();

    if rx_points.len() < 2 && tx_points.len() < 2 {
        let hint = Paragraph::new(Line::styled(
            " gathering samples…",
            Style::new().fg(t.muted),
        ));
        frame.render_widget(hint, area);
        return;
    }

    let rx_max = rx.values.iter().copied().fold(0.0_f64, f64::max);
    let tx_max = tx.values.iter().copied().fold(0.0_f64, f64::max);
    let max = rx_max.max(tx_max);
    let pad = max.max(1.0) * 0.1;
    let y_lo = 0.0;
    let y_hi = max + pad;
    let y_mid = y_hi / 2.0;
    let y_labels = vec![
        Line::from(format_scale(y_lo, rx.scale)),
        Line::from(format_scale(y_mid, rx.scale)),
        Line::from(format_scale(y_hi, rx.scale)),
    ];

    let sample_count = rx.values.len().max(tx.values.len());
    let x_max = sample_count.saturating_sub(1) as f64;
    let total_secs = sample_count.saturating_sub(1) as u64 * rx.sample_interval_s;
    let x_labels = if area.width >= 36 && total_secs >= 2 {
        vec![
            Line::from(format_ago(total_secs)),
            Line::from(format_ago(total_secs / 2)),
            Line::from("now"),
        ]
    } else {
        vec![Line::from(format_ago(total_secs)), Line::from("now")]
    };

    let rx_dataset = Dataset::default()
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::new().fg(rx.color))
        .data(&rx_points);
    let tx_dataset = Dataset::default()
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::new().fg(tx.color))
        .data(&tx_points);

    let chart = Chart::new(vec![rx_dataset, tx_dataset])
        .style(Style::new().bg(t.bg))
        .x_axis(
            Axis::default()
                .style(Style::new().fg(t.muted))
                .bounds([0.0, x_max.max(1.0)])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .style(Style::new().fg(t.muted))
                .bounds([y_lo, y_hi])
                .labels(y_labels)
                .labels_alignment(Alignment::Right),
        );

    frame.render_widget(chart, area);
}

pub fn render_monitor_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &MonitorState,
    traffic: &[TrafficSample],
    traffic_baseline: Option<(u64, u64)>,
    measure_sdk: bool,
    zoomed: bool,
) {
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

    render_charts(frame, inner, state, traffic, traffic_baseline, measure_sdk, focused, zoomed);
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
