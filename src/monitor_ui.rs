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

use std::time::Instant;

use crate::monitor::{GcEvent, MonitorState};
use crate::network::TrafficSample;
use crate::panel;
use crate::theme;
use crate::ui::{panel_block, render_pane_chip, split_chip};

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
    color: Color,
    scale: MetricScale,
    /// Cumulative bytes transferred since holo started watching. Only set
    /// for network rx/tx; None for CPU/mem/disk.
    total_bytes: Option<u64>,
    /// Seconds-ago for each event to overlay on the sparkline (rightmost = now).
    /// Empty for metrics without overlays.
    tick_secs_ago: Vec<u32>,
}

/// One navigable monitor view. CPU/RSS/Disk are `Single`s; the network row
/// is a `Pair` rendered as one combined entry in the compact list and one
/// dual-line chart in the detail view.
enum MetricView {
    Single(Metric),
    Pair { rx: Metric, tx: Metric },
}

impl MetricView {
    fn pair_label(rx: &Metric, tx: &Metric) -> String {
        let rx_total = rx.total_bytes.unwrap_or(0);
        let tx_total = tx.total_bytes.unwrap_or(0);
        let rx_now = *rx.values.last().unwrap_or(&0.0);
        let tx_now = *tx.values.last().unwrap_or(&0.0);
        format!(
            " Network ↓ {} ({})  ↑ {} ({})  ",
            format_rate(rx_now.max(0.0).round() as u64),
            format_bytes(rx_total),
            format_rate(tx_now.max(0.0).round() as u64),
            format_bytes(tx_total),
        )
    }
}

fn gc_secs_ago(events: &[GcEvent]) -> Vec<u32> {
    gc_secs_ago_at(Instant::now(), events)
}

fn gc_secs_ago_at(now: Instant, events: &[GcEvent]) -> Vec<u32> {
    events
        .iter()
        .map(|e| now.saturating_duration_since(e.received_at).as_secs() as u32)
        .collect()
}

/// Build the tick-overlay row: `width` cells wide with `◆` placed at columns
/// proportional to each `secs_ago` value, matching how ratatui's Chart
/// distributes sample indices `[0, window]` across `[0, width-1]`. The
/// rightmost column is "now"; ticks older than `window` are dropped.
///
/// `window` is the visible time span in seconds, equal to `len(samples) - 1`
/// at the current 1 Hz cadence. The naive 1-col-per-sec mapping only happens
/// to align when `width == window`; otherwise marks drift.
fn build_tick_row(width: usize, window: f64, secs_ago: &[u32]) -> String {
    let mut row = vec![' '; width];
    if width == 0 || window <= 0.0 {
        return row.into_iter().collect();
    }
    let last = (width - 1) as f64;
    for &secs in secs_ago {
        let secs = secs as f64;
        if secs > window {
            continue;
        }
        let col = ((1.0 - secs / window) * last).round() as usize;
        if col < width {
            row[col] = '◆';
        }
    }
    row.into_iter().collect()
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
) -> Vec<MetricView> {
    let t = theme::current();
    let mut views = Vec::new();

    let cpu_now = state.history.last().map(|m| m.cpu_percent).unwrap_or(0.0);
    let cpu_values: Vec<f64> = state.history.iter().map(|m| m.cpu_percent.max(0.0) as f64).collect();
    let cpu_spark: Vec<SparklineBar> = state
        .history
        .iter()
        .map(|m| SparklineBar::from(Some(m.cpu_percent.max(0.0).round() as u64)))
        .collect();
    views.push(MetricView::Single(Metric {
        name: "CPU",
        label: format!(" CPU {:.1}%  ", cpu_now),
        sparkline_data: cpu_spark,
        sparkline_max: 100,
        values: cpu_values,
        color: t.spark_cpu,
        scale: MetricScale::Percent,
        total_bytes: None,
        tick_secs_ago: Vec::new(),
    }));

    let mem_samples: Vec<u64> = state.history.iter().map(|m| m.rss_kb).collect();
    let mem_now = *mem_samples.last().unwrap_or(&0);
    let (mem_spark, mem_max) = range_shifted(&mem_samples);
    views.push(MetricView::Single(Metric {
        name: "RSS",
        label: format!(" RSS {}  ", format_mb(mem_now)),
        sparkline_data: mem_spark,
        sparkline_max: mem_max,
        values: mem_samples.iter().map(|&v| v as f64).collect(),
        color: t.spark_mem,
        scale: MetricScale::MemKb,
        total_bytes: None,
        tick_secs_ago: gc_secs_ago(&state.gc_events),
    }));

    let disk_samples: Vec<u64> = state.history.iter().map(|m| m.data_kb).collect();
    let disk_now = *disk_samples.last().unwrap_or(&0);
    let (disk_spark, disk_max) = range_shifted(&disk_samples);
    views.push(MetricView::Single(Metric {
        name: "Disk",
        label: format!(" Disk {}  ", format_mb_precise(disk_now)),
        sparkline_data: disk_spark,
        sparkline_max: disk_max,
        values: disk_samples.iter().map(|&v| v as f64).collect(),
        color: t.spark_disk,
        scale: MetricScale::DiskKb,
        total_bytes: None,
        tick_secs_ago: Vec::new(),
    }));

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
        let rx = Metric {
            name: "↓",
            label: format!(" ↓ {} ({})  ", format_rate(last.rx_bps), format_bytes(rx_total)),
            sparkline_data: rx_samples,
            sparkline_max: rx_max,
            values: traffic.iter().map(|s| s.rx_bps as f64).collect(),
            color: t.spark_rx,
            scale: MetricScale::RateBps,
            total_bytes: Some(rx_total),
            tick_secs_ago: Vec::new(),
        };
        let tx = Metric {
            name: "↑",
            label: format!(" ↑ {} ({})  ", format_rate(last.tx_bps), format_bytes(tx_total)),
            sparkline_data: tx_samples,
            sparkline_max: tx_max,
            values: traffic.iter().map(|s| s.tx_bps as f64).collect(),
            color: t.spark_tx,
            scale: MetricScale::RateBps,
            total_bytes: Some(tx_total),
            tick_secs_ago: Vec::new(),
        };
        views.push(MetricView::Pair { rx, tx });
    }

    views
}

fn sparkline_row(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    spark: &Metric,
    label_width: u16,
    selected: bool,
    list_active: bool,
) {
    let t = theme::current();
    let chunks = Layout::horizontal([
        Constraint::Length(label_width),
        Constraint::Min(0),
    ])
    .split(area);
    let (label_style, prefix) = if selected && list_active {
        (Style::new().fg(t.accent).add_modifier(Modifier::BOLD), "▸ ")
    } else if selected {
        (Style::new().fg(t.muted).add_modifier(Modifier::BOLD), "  ")
    } else {
        (Style::new().fg(t.fg), "  ")
    };
    let display = format!("{}{}", prefix, label.trim_start());
    frame.render_widget(
        Paragraph::new(Line::styled(display, label_style)),
        chunks[0],
    );
    let reversed: Vec<SparklineBar> = spark.sparkline_data.iter().rev().cloned().collect();
    let mut sparkline = Sparkline::default()
        .data(reversed)
        .direction(RenderDirection::RightToLeft)
        .style(Style::new().fg(spark.color))
        .bar_set(BASELINE_BARS)
        .absent_value_symbol(symbols::bar::ONE_EIGHTH)
        .absent_value_style(Style::new().fg(spark.color));
    if spark.sparkline_max > 0 {
        sparkline = sparkline.max(spark.sparkline_max);
    }
    frame.render_widget(sparkline, chunks[1]);
}

/// Render '◆' marks above a chart body at columns matching each tick's
/// position on the chart's x-axis (rightmost = "now"). `window` is the
/// visible time span in seconds, used to scale tick positions to match the
/// chart.
fn render_ticks(frame: &mut Frame, area: Rect, ticks: &[u32], window: f64, color: Color) {
    if area.width == 0 || area.height == 0 || ticks.is_empty() {
        return;
    }
    let line = build_tick_row(area.width as usize, window, ticks);
    frame.render_widget(
        Paragraph::new(Line::styled(line, Style::new().fg(color))),
        area,
    );
}

fn render_compact_list(
    frame: &mut Frame,
    area: Rect,
    views: &[MetricView],
    selected: usize,
    list_active: bool,
) {
    if views.is_empty() || area.height == 0 {
        return;
    }
    let row_data: Vec<(String, &Metric)> = views
        .iter()
        .map(|v| match v {
            MetricView::Single(m) => (m.label.clone(), m),
            MetricView::Pair { rx, tx } => (MetricView::pair_label(rx, tx), rx),
        })
        .collect();
    // Prefer a 1-line gap between rows; if height is too tight, drop the gap
    // so more rows still fit.
    let spaced_rows = (area.height as usize).div_ceil(2).min(row_data.len());
    let (visible_rows, spacing) = if spaced_rows == row_data.len() {
        (row_data.len(), 1)
    } else {
        (row_data.len().min(area.height as usize), 0)
    };
    let label_width = row_data
        .iter()
        .map(|(label, _)| label.trim_start().chars().count())
        .max()
        .unwrap_or(0) as u16
        + 2;
    let constraints: Vec<Constraint> = (0..visible_rows).map(|_| Constraint::Length(1)).collect();
    let chunks = Layout::vertical(constraints).spacing(spacing).split(area);

    for (i, (label, spark)) in row_data.iter().take(visible_rows).enumerate() {
        sparkline_row(frame, chunks[i], label, spark, label_width, i == selected, list_active);
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
    let mut spans = vec![
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
    ];
    if !metric.tick_secs_ago.is_empty() {
        spans.push(Span::styled(
            format!("  ◆ {} GC", metric.tick_secs_ago.len()),
            Style::new().fg(metric.color),
        ));
    }
    let stats_line = Line::from(spans).alignment(Alignment::Right);
    frame.render_widget(stats_line, chip_area);

    if body.height < 3 || body.width < 8 {
        return;
    }
    let chart_area = if metric.tick_secs_ago.is_empty() || body.height < 5 {
        body
    } else {
        let split = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(body);
        // Y-axis labels eat the leftmost columns of the chart; offset the tick
        // row by the same width so '◆' marks line up with sample columns.
        let y_label_pad = chart_y_label_width(metric, min, max);
        let tick_area = Rect {
            x: split[0].x + y_label_pad,
            y: split[0].y,
            width: split[0].width.saturating_sub(y_label_pad),
            height: 1,
        };
        let window = metric.values.len().saturating_sub(1) as f64;
        render_ticks(frame, tick_area, &metric.tick_secs_ago, window, metric.color);
        split[1]
    };
    render_chart(frame, chart_area, metric, min, max);
}

/// Width reserved for y-axis labels in the chart (max label string length + 1
/// for spacing). Matches what ratatui's Chart computes from the labels we feed
/// it in render_chart.
fn chart_y_label_width(metric: &Metric, min: f64, max: f64) -> u16 {
    let (y_lo, y_hi) = match metric.scale {
        MetricScale::Percent => (0.0, 100.0),
        _ => {
            let span = (max - min).max(1.0);
            let pad = span * 0.1;
            ((min - pad).max(0.0), max + pad)
        }
    };
    let y_mid = (y_lo + y_hi) / 2.0;
    [y_lo, y_mid, y_hi]
        .iter()
        .map(|v| format_scale(*v, metric.scale).chars().count() as u16)
        .max()
        .unwrap_or(0)
        + 1
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
    let total_secs = metric.values.len().saturating_sub(1) as u64;
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

/// Number of navigable monitor entries given the current traffic. Mirrors
/// what `build_metrics` produces: the 3 always-present rows (CPU/RSS/Disk)
/// plus the combined Network row when any traffic has been recorded.
pub fn metric_count(traffic: &[TrafficSample]) -> usize {
    if traffic.is_empty() { 3 } else { 4 }
}

fn render_pair_detail(frame: &mut Frame, area: Rect, rx: &Metric, tx: &Metric, focused: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let t = theme::current();
    let (chip_area, body) = split_chip(area);
    render_pane_chip(frame, chip_area, "Network", focused, false);

    let rx_now = *rx.values.last().unwrap_or(&0.0);
    let tx_now = *tx.values.last().unwrap_or(&0.0);
    let rx_total = rx.total_bytes.unwrap_or(0);
    let tx_total = tx.total_bytes.unwrap_or(0);
    let stats_line = Line::from(vec![
        Span::styled(
            format!("{} {}", rx.name, format_scale(rx_now, rx.scale)),
            Style::new().fg(rx.color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({})", format_bytes(rx_total)),
            Style::new().fg(t.muted),
        ),
        Span::raw("   "),
        Span::styled(
            format!("{} {}", tx.name, format_scale(tx_now, tx.scale)),
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
    let total_secs = sample_count.saturating_sub(1) as u64;
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

    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let views = build_metrics(state, traffic, traffic_baseline);
    if views.is_empty() {
        return;
    }
    let selected = state.selected_metric.min(views.len() - 1);
    if state.detail_open {
        match &views[selected] {
            MetricView::Single(m) => render_metric_detail(frame, inner, m, focused),
            MetricView::Pair { rx, tx } => render_pair_detail(frame, inner, rx, tx, focused),
        }
    } else {
        render_compact_list(frame, inner, &views, selected, focused);
    }
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

    use std::time::{Duration, Instant};

    fn ev(received_at: Instant) -> GcEvent {
        GcEvent { received_at, duration_us: 0 }
    }

    #[test]
    fn gc_secs_ago_buckets_by_seconds() {
        let now = Instant::now();
        let events = [
            ev(now),
            ev(now - Duration::from_millis(1500)),
            ev(now - Duration::from_secs(30)),
        ];
        assert_eq!(gc_secs_ago_at(now, &events), vec![0, 1, 30]);
    }

    #[test]
    fn gc_secs_ago_handles_future_event_without_panic() {
        // Event timestamp in the "future" relative to `now` should saturate
        // to 0, not underflow.
        let now = Instant::now();
        let events = [ev(now + Duration::from_secs(5))];
        assert_eq!(gc_secs_ago_at(now, &events), vec![0]);
    }

    #[test]
    fn tick_row_places_marks_from_the_right() {
        // width=10, window=9, ticks at 0/3/9 secs ago → cols 9, 6, 0.
        assert_eq!(build_tick_row(10, 9.0, &[0, 3, 9]), "◆     ◆  ◆");
    }

    #[test]
    fn tick_row_drops_ticks_older_than_window() {
        // window=4 (5 samples at 1 Hz), tick at 7s ago → outside window.
        assert_eq!(build_tick_row(5, 4.0, &[7]), "     ");
    }

    #[test]
    fn tick_row_collapses_duplicate_columns() {
        // Two ticks at the same second yield one '◆' at the same column.
        assert_eq!(build_tick_row(4, 3.0, &[1, 1]), "  ◆ ");
    }

    #[test]
    fn tick_row_empty_ticks_is_blank() {
        assert_eq!(build_tick_row(6, 5.0, &[]), "      ");
    }

    #[test]
    fn tick_row_aligns_with_chart_when_width_lt_samples() {
        // Regression for the case the old `width - 1 - secs` formula got
        // wrong: with 120 samples (window=119) and a chart body of 91
        // cells, ratatui's Chart places sample index 59 at column ~45.
        // The tick for "60s ago" must land on the same column.
        let row = build_tick_row(91, 119.0, &[60]);
        let cols: Vec<usize> = row
            .chars()
            .enumerate()
            .filter(|(_, c)| *c == '◆')
            .map(|(i, _)| i)
            .collect();
        assert_eq!(cols, vec![45]);
    }

    #[test]
    fn tick_row_aligns_with_chart_when_width_gt_samples() {
        // Other direction: chart wider than the sample count. With 120
        // samples (window=119) and a 200-cell body, sample index 59 sits
        // at column round(59/119 * 199) = 99; the 60s-ago tick must too.
        let row = build_tick_row(200, 119.0, &[60]);
        let cols: Vec<usize> = row
            .chars()
            .enumerate()
            .filter(|(_, c)| *c == '◆')
            .map(|(i, _)| i)
            .collect();
        assert_eq!(cols, vec![99]);
    }
}
