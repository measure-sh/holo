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

use crate::monitor::{GcEvent, MonitorState, MonitorView};
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
    /// Per-sample seconds-ago, when the source carries real timestamps
    /// (currently RSS via the agent). Same length as `values`. Empty for
    /// ADB-sourced metrics, which fall back to uniform index spacing.
    secs_ago: Vec<f64>,
    color: Color,
    scale: MetricScale,
    /// Seconds-ago for each event to overlay on the sparkline (rightmost = now).
    /// Empty for metrics without overlays.
    tick_secs_ago: Vec<u32>,
    /// Visible time span in seconds. For agent-sourced metrics this comes from
    /// the spread of `ts_ns` values; for ADB-sourced metrics it falls back to
    /// `values.len() - 1` under the assumption of 1 Hz cadence.
    window_secs: f64,
}

const NS_PER_SEC_F64: f64 = 1_000_000_000.0;

fn gc_secs_ago_from_ns(now_ns: i64, events: &[GcEvent]) -> Vec<u32> {
    events
        .iter()
        .map(|e| ((now_ns.saturating_sub(e.ts_ns)).max(0) / 1_000_000_000) as u32)
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
    let cpu_window = sample_index_window(cpu_values.len());
    metrics.push(Metric {
        name: "CPU",
        label: format!(" CPU {:.1}%  ", cpu_now),
        sparkline_data: cpu_spark,
        sparkline_max: 100,
        values: cpu_values,
        color: t.spark_cpu,
        scale: MetricScale::Percent,
        secs_ago: Vec::new(),
        tick_secs_ago: Vec::new(),
        window_secs: cpu_window,
    });

    // Java heap is the headline memory metric — it moves on GC, where RSS
    // barely budges. Native + RSS overlay in the detail view.
    let mem_samples: Vec<u64> = state.memory_history.iter().map(|s| s.java_heap_kb).collect();
    let mem_now = *mem_samples.last().unwrap_or(&0);
    let (mem_spark, mem_max) = range_shifted(&mem_samples);
    // Anchor "now" on the freshest agent timestamp across both streams. Both
    // GC events and memory samples come from the agent's CLOCK_MONOTONIC, so
    // tick positions and chart x-values use the same time origin.
    let now_ns = state.latest_agent_ts_ns().unwrap_or(0);
    let mem_secs_ago: Vec<f64> = state
        .memory_history
        .iter()
        .map(|s| (now_ns.saturating_sub(s.ts_ns).max(0) as f64) / NS_PER_SEC_F64)
        .collect();
    let mem_window_secs = mem_secs_ago.first().copied().unwrap_or(0.0);
    metrics.push(Metric {
        name: "Java",
        label: format!(" Java {}  ", format_mb(mem_now)),
        sparkline_data: mem_spark,
        sparkline_max: mem_max,
        values: mem_samples.iter().map(|&v| v as f64).collect(),
        secs_ago: mem_secs_ago,
        color: t.spark_mem,
        scale: MetricScale::MemKb,
        tick_secs_ago: gc_secs_ago_from_ns(now_ns, &state.gc_events),
        window_secs: mem_window_secs,
    });

    let disk_samples: Vec<u64> = state.history.iter().map(|m| m.data_kb).collect();
    let disk_now = *disk_samples.last().unwrap_or(&0);
    let (disk_spark, disk_max) = range_shifted(&disk_samples);
    let disk_window = sample_index_window(disk_samples.len());
    metrics.push(Metric {
        name: "Disk",
        label: format!(" Disk {}  ", format_mb_precise(disk_now)),
        sparkline_data: disk_spark,
        sparkline_max: disk_max,
        values: disk_samples.iter().map(|&v| v as f64).collect(),
        color: t.spark_disk,
        scale: MetricScale::DiskKb,
        secs_ago: Vec::new(),
        tick_secs_ago: Vec::new(),
        window_secs: disk_window,
    });

    let last = traffic.last().copied().unwrap_or_default();
    let rx_total = traffic_baseline
        .map(|(b, _)| last.rx_total.saturating_sub(b))
        .unwrap_or(0);
    let tx_total = traffic_baseline
        .map(|(_, b)| last.tx_total.saturating_sub(b))
        .unwrap_or(0);
    let rx_spark: Vec<SparklineBar> = traffic.iter().map(|s| SparklineBar::from(Some(s.rx_bps))).collect();
    let tx_spark: Vec<SparklineBar> = traffic.iter().map(|s| SparklineBar::from(Some(s.tx_bps))).collect();
    let rx_max = traffic.iter().map(|s| s.rx_bps).max().unwrap_or(0);
    let tx_max = traffic.iter().map(|s| s.tx_bps).max().unwrap_or(0);
    let rate_window = sample_index_window(traffic.len());
    metrics.push(Metric {
        name: "↓",
        label: format!(" ↓ {} ({})  ", format_rate(last.rx_bps), format_bytes(rx_total)),
        sparkline_data: rx_spark,
        sparkline_max: rx_max,
        values: traffic.iter().map(|s| s.rx_bps as f64).collect(),
        color: t.spark_rx,
        scale: MetricScale::RateBps,
        secs_ago: Vec::new(),
        tick_secs_ago: Vec::new(),
        window_secs: rate_window,
    });
    metrics.push(Metric {
        name: "↑",
        label: format!(" ↑ {} ({})  ", format_rate(last.tx_bps), format_bytes(tx_total)),
        sparkline_data: tx_spark,
        sparkline_max: tx_max,
        values: traffic.iter().map(|s| s.tx_bps as f64).collect(),
        color: t.spark_tx,
        scale: MetricScale::RateBps,
        secs_ago: Vec::new(),
        tick_secs_ago: Vec::new(),
        window_secs: rate_window,
    });

    metrics
}

/// Fallback window for metrics that don't carry per-sample timestamps. The
/// poller runs at ~1 Hz, so `len - 1` is a passable approximation when no
/// better source is available; the visible drift only matters for streams
/// that overlay other timestamped data, which is the GC-on-RSS chart and
/// that one uses real timestamps.
fn sample_index_window(len: usize) -> f64 {
    len.saturating_sub(1) as f64
}

fn sparkline_row(frame: &mut Frame, area: Rect, metric: &Metric, label_width: u16) {
    let t = theme::current();
    let chunks = Layout::horizontal([
        Constraint::Length(label_width),
        Constraint::Min(0),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::styled(metric.label.clone(), Style::new().fg(t.fg))),
        chunks[0],
    );
    let reversed: Vec<SparklineBar> = metric.sparkline_data.iter().rev().cloned().collect();
    let mut sparkline = Sparkline::default()
        .data(reversed)
        .direction(RenderDirection::RightToLeft)
        .style(Style::new().fg(metric.color))
        .bar_set(BASELINE_BARS)
        .absent_value_symbol(symbols::bar::ONE_EIGHTH)
        .absent_value_style(Style::new().fg(metric.color));
    if metric.sparkline_max > 0 {
        sparkline = sparkline.max(metric.sparkline_max);
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

fn render_compact_list(frame: &mut Frame, area: Rect, metrics: &[Metric]) {
    if metrics.is_empty() || area.height == 0 {
        return;
    }
    // Prefer a 1-line gap between rows; if height is too tight, drop the gap
    // so more rows still fit.
    let spaced_rows = (area.height as usize).div_ceil(2).min(metrics.len());
    let (visible_rows, spacing) = if spaced_rows == metrics.len() {
        (metrics.len(), 1)
    } else {
        (metrics.len().min(area.height as usize), 0)
    };
    let label_width = metrics
        .iter()
        .map(|m| m.label.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let constraints: Vec<Constraint> = (0..visible_rows).map(|_| Constraint::Length(1)).collect();
    let chunks = Layout::vertical(constraints).spacing(spacing).split(area);

    for (i, metric) in metrics.iter().take(visible_rows).enumerate() {
        sparkline_row(frame, chunks[i], metric, label_width);
    }
}

/// Memory series rendered side-by-side in the detail view. All three share
/// `secs_ago` (one entry per `MemorySample`) so points line up on the x-axis.
struct MemorySeries {
    name: &'static str,
    color: Color,
    values: Vec<f64>,
    current: u64,
}

fn collect_memory_series(state: &MonitorState) -> Vec<MemorySeries> {
    let t = theme::current();
    let java: Vec<f64> = state
        .memory_history
        .iter()
        .map(|s| s.java_heap_kb as f64)
        .collect();
    let native: Vec<f64> = state
        .memory_history
        .iter()
        .map(|s| s.native_heap_kb as f64)
        .collect();
    let rss: Vec<f64> = state.memory_history.iter().map(|s| s.rss_kb as f64).collect();
    let last = |v: &[f64]| v.last().copied().unwrap_or(0.0).round() as u64;
    vec![
        MemorySeries { name: "Java", color: t.spark_mem, current: last(&java), values: java },
        MemorySeries { name: "Native", color: t.spark_disk, current: last(&native), values: native },
        MemorySeries { name: "RSS", color: t.muted, current: last(&rss), values: rss },
    ]
}

fn memory_secs_ago(state: &MonitorState) -> (Vec<f64>, f64) {
    let now_ns = state.latest_agent_ts_ns().unwrap_or(0);
    let secs: Vec<f64> = state
        .memory_history
        .iter()
        .map(|s| (now_ns.saturating_sub(s.ts_ns).max(0) as f64) / NS_PER_SEC_F64)
        .collect();
    let window = secs.first().copied().unwrap_or(0.0);
    (secs, window)
}

fn render_memory_detail(frame: &mut Frame, area: Rect, state: &MonitorState, focused: bool) {
    let t = theme::current();
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (chip_area, body) = split_chip(area);
    render_pane_chip(frame, chip_area, "Memory", focused, false);

    let series = collect_memory_series(state);
    let java = &series[0];
    let now_ns = state.latest_agent_ts_ns().unwrap_or(0);
    let gc_ticks = gc_secs_ago_from_ns(now_ns, &state.gc_events);
    // The chip-row's right side is the legend: one colored span per series
    // (Java bold, Native + RSS regular) plus the GC count. No min/max/avg —
    // it doesn't fit three series of different magnitudes.
    let mut spans: Vec<Span<'_>> = Vec::with_capacity(series.len() * 2 + 2);
    for (i, s) in series.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ·  ", Style::new().fg(t.muted)));
        }
        let mut style = Style::new().fg(s.color);
        if i == 0 {
            style = style.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(
            format!("{} {}", s.name, format_mb(s.current)),
            style,
        ));
    }
    if !gc_ticks.is_empty() {
        spans.push(Span::styled("  ·  ", Style::new().fg(t.muted)));
        spans.push(Span::styled(
            format!("◆ {} GC", gc_ticks.len()),
            Style::new().fg(java.color),
        ));
    }
    frame.render_widget(Line::from(spans).alignment(Alignment::Right), chip_area);

    if body.height < 3 || body.width < 8 {
        return;
    }

    let (secs_ago, window_secs) = memory_secs_ago(state);
    if secs_ago.len() < 2 {
        let hint = Paragraph::new(Line::styled(
            " gathering samples…",
            Style::new().fg(t.muted),
        ));
        frame.render_widget(hint, body);
        return;
    }

    // Y bounds across all three series so each line is fully visible. Pad 10%
    // above max; floor at 0.
    let y_max_value = series
        .iter()
        .flat_map(|s| s.values.iter().copied())
        .fold(0.0_f64, f64::max);
    let y_min_value = series
        .iter()
        .flat_map(|s| s.values.iter().copied())
        .filter(|v| *v > 0.0)
        .fold(f64::INFINITY, f64::min);
    let y_min_value = if y_min_value.is_finite() { y_min_value } else { 0.0 };
    let span = (y_max_value - y_min_value).max(1.0);
    let y_lo = (y_min_value - span * 0.1).max(0.0);
    let y_hi = y_max_value + span * 0.1;

    // Vertical layout: [tick row?][chart]. Tick row needs ≥1 line and is
    // dropped when there isn't enough vertical room.
    let want_ticks = !gc_ticks.is_empty();
    let mut constraints: Vec<Constraint> = Vec::new();
    if want_ticks && body.height >= 4 {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(0));
    let chunks = Layout::vertical(constraints).split(body);
    let mut chunk_idx = 0;

    let y_labels = vec![
        Line::from(format_mb(y_lo.round() as u64)),
        Line::from(format_mb(((y_lo + y_hi) / 2.0).round() as u64)),
        Line::from(format_mb(y_hi.round() as u64)),
    ];
    let y_label_pad = y_labels
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(0)
        + 1;

    if want_ticks && body.height >= 4 {
        let row = chunks[chunk_idx];
        let tick_area = Rect {
            x: row.x + y_label_pad,
            y: row.y,
            width: row.width.saturating_sub(y_label_pad),
            height: 1,
        };
        render_ticks(frame, tick_area, &gc_ticks, window_secs, java.color);
        chunk_idx += 1;
    }

    let chart_area = chunks[chunk_idx];

    let x_max = window_secs.max(1.0);
    let datasets_data: Vec<Vec<(f64, f64)>> = series
        .iter()
        .map(|s| {
            s.values
                .iter()
                .zip(secs_ago.iter())
                .filter(|&(&v, _)| v > 0.0)
                .map(|(&v, &t)| ((x_max - t).max(0.0), v))
                .collect()
        })
        .collect();
    // No `.name()` on the datasets — that would make ratatui draw its own
    // legend box. The chip-row spans above are the legend.
    let datasets: Vec<Dataset<'_>> = series
        .iter()
        .zip(datasets_data.iter())
        .map(|(s, data)| {
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(s.color))
                .data(data)
        })
        .collect();

    let total_secs = window_secs.round() as u64;
    let x_labels = if chart_area.width >= 36 && total_secs >= 2 {
        vec![
            Line::from(format_ago(total_secs)),
            Line::from(format_ago(total_secs / 2)),
            Line::from("now"),
        ]
    } else {
        vec![Line::from(format_ago(total_secs)), Line::from("now")]
    };

    let chart = Chart::new(datasets)
        .style(Style::new().bg(t.bg))
        .x_axis(
            Axis::default()
                .style(Style::new().fg(t.muted))
                .bounds([0.0, x_max])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .style(Style::new().fg(t.muted))
                .bounds([y_lo, y_hi])
                .labels(y_labels)
                .labels_alignment(Alignment::Right),
        );
    frame.render_widget(chart, chart_area);
}

fn render_network_detail(
    frame: &mut Frame,
    area: Rect,
    traffic: &[TrafficSample],
    traffic_baseline: Option<(u64, u64)>,
    focused: bool,
) {
    let t = theme::current();
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (chip_area, body) = split_chip(area);
    render_pane_chip(frame, chip_area, "Network", focused, false);

    let last = traffic.last().copied().unwrap_or_default();
    let rx_total = traffic_baseline
        .map(|(b, _)| last.rx_total.saturating_sub(b))
        .unwrap_or(0);
    let tx_total = traffic_baseline
        .map(|(_, b)| last.tx_total.saturating_sub(b))
        .unwrap_or(0);
    let header = Line::from(vec![
        Span::styled(
            format!("↓ {} ({})", format_rate(last.rx_bps), format_bytes(rx_total)),
            Style::new().fg(t.spark_rx).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::new().fg(t.muted)),
        Span::styled(
            format!("↑ {} ({})", format_rate(last.tx_bps), format_bytes(tx_total)),
            Style::new().fg(t.spark_tx).add_modifier(Modifier::BOLD),
        ),
    ])
    .alignment(Alignment::Right);
    frame.render_widget(header, chip_area);

    if body.height < 3 || body.width < 8 || traffic.len() < 2 {
        if traffic.len() < 2 {
            frame.render_widget(
                Paragraph::new(Line::styled(" gathering samples…", Style::new().fg(t.muted))),
                body,
            );
        }
        return;
    }

    let rx_max = traffic.iter().map(|s| s.rx_bps).max().unwrap_or(0);
    let tx_max = traffic.iter().map(|s| s.tx_bps).max().unwrap_or(0);
    let combined_max = rx_max.max(tx_max).max(1);
    let pad = (combined_max as f64 * 0.1).max(1.0);
    let y_lo = 0.0;
    let y_hi = combined_max as f64 + pad;

    let last_idx = traffic.len().saturating_sub(1).max(1) as f64;
    let rx_points: Vec<(f64, f64)> = traffic
        .iter()
        .enumerate()
        .map(|(i, s)| (i as f64 / last_idx * last_idx, s.rx_bps as f64))
        .collect();
    let tx_points: Vec<(f64, f64)> = traffic
        .iter()
        .enumerate()
        .map(|(i, s)| (i as f64 / last_idx * last_idx, s.tx_bps as f64))
        .collect();

    let datasets = vec![
        Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::new().fg(t.spark_rx))
            .data(&rx_points),
        Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::new().fg(t.spark_tx))
            .data(&tx_points),
    ];

    let total_secs = last_idx as u64;
    let x_labels = if body.width >= 36 && total_secs >= 2 {
        vec![
            Line::from(format_ago(total_secs)),
            Line::from(format_ago(total_secs / 2)),
            Line::from("now"),
        ]
    } else {
        vec![Line::from(format_ago(total_secs)), Line::from("now")]
    };

    let y_labels = vec![
        Line::from(format_rate(y_lo as u64)),
        Line::from(format_rate(((y_lo + y_hi) / 2.0) as u64)),
        Line::from(format_rate(y_hi as u64)),
    ];

    let chart = Chart::new(datasets)
        .style(Style::new().bg(t.bg))
        .x_axis(
            Axis::default()
                .style(Style::new().fg(t.muted))
                .bounds([0.0, last_idx])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .style(Style::new().fg(t.muted))
                .bounds([y_lo, y_hi])
                .labels(y_labels)
                .labels_alignment(Alignment::Right),
        );
    frame.render_widget(chart, body);
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
        render_ticks(frame, tick_area, &metric.tick_secs_ago, metric.window_secs, metric.color);
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
    // Distribute samples evenly across [0, window_secs]. For agent-sourced
    // metrics window_secs comes from the actual time spread; for ADB-sourced
    // ones it's a sample-index proxy. Either way the rightmost sample lands
    // at x = window_secs ("now") and matches build_tick_row's anchoring.
    let last = metric.values.len().saturating_sub(1).max(1) as f64;
    let x_max = metric.window_secs.max(1.0);
    let points: Vec<(f64, f64)> = if metric.secs_ago.is_empty() {
        metric
            .values
            .iter()
            .enumerate()
            .filter(|&(_, &v)| !skip_zero || v > 0.0)
            .map(|(i, &v)| (i as f64 / last * x_max, v))
            .collect()
    } else {
        // Plot each sample at its real ts_ns position. The rightmost sample
        // sits at x = x_max (= "now"); GC ticks placed by the same `secs_ago
        // → x_max - secs_ago` mapping land in the same column.
        metric
            .values
            .iter()
            .zip(metric.secs_ago.iter())
            .filter(|&(&v, _)| !skip_zero || v > 0.0)
            .map(|(&v, &s)| ((x_max - s).max(0.0), v))
            .collect()
    };

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

    let y_mid = (y_lo + y_hi) / 2.0;
    let y_labels = vec![
        Line::from(format_scale(y_lo, metric.scale)),
        Line::from(format_scale(y_mid, metric.scale)),
        Line::from(format_scale(y_hi, metric.scale)),
    ];

    // X labels: total duration ago → half ago → now. Width check avoids
    // cramped middle labels when the detail pane is narrow.
    let total_secs = metric.window_secs.round() as u64;
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
                .bounds([0.0, x_max])
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

/// `◂ view:<name> ▸` chevrons that mirror logcat's level cycle hint.
/// Rendered at the bottom of the panel block when focused.
fn view_hint(view: MonitorView) -> Line<'static> {
    let t = theme::current();
    let muted = Style::new().fg(t.muted);
    let accent = Style::new().fg(t.accent);
    Line::from(vec![
        Span::styled(" \u{25C2}", accent),
        Span::styled(format!("view:{}", view.label()), muted),
        Span::styled("\u{25B8} ", accent),
    ])
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
    let mut block = panel_block(panel::MONITOR, focused);
    if focused {
        block = block.title_bottom(view_hint(state.view));
    }
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
    match state.view {
        MonitorView::Memory => {
            render_memory_detail(frame, inner, state, focused);
            return;
        }
        MonitorView::Network => {
            render_network_detail(frame, inner, traffic, traffic_baseline, focused);
            return;
        }
        _ => {}
    }
    let metrics = build_metrics(state, traffic, traffic_baseline);
    let detail = match state.view {
        MonitorView::All => None,
        MonitorView::Cpu => Some(0),
        MonitorView::Memory | MonitorView::Network => unreachable!(),
        MonitorView::Disk => Some(2),
    };
    match detail {
        Some(i) if i < metrics.len() => render_metric_detail(frame, inner, &metrics[i], focused),
        _ => render_compact_list(frame, inner, &metrics),
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

    fn ev(ts_ns: i64) -> GcEvent {
        GcEvent { ts_ns, duration_us: 0 }
    }

    const NS: i64 = 1_000_000_000;

    #[test]
    fn gc_secs_ago_buckets_by_seconds() {
        let now = 100 * NS;
        let events = [
            ev(now),
            ev(now - 1_500_000_000), // 1.5 s ago → 1
            ev(now - 30 * NS),
        ];
        assert_eq!(gc_secs_ago_from_ns(now, &events), vec![0, 1, 30]);
    }

    #[test]
    fn gc_secs_ago_handles_future_event_without_panic() {
        // Event timestamp in the "future" relative to `now` should saturate
        // to 0, not underflow.
        let now = 100 * NS;
        let events = [ev(now + 5 * NS)];
        assert_eq!(gc_secs_ago_from_ns(now, &events), vec![0]);
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

    #[test]
    fn memory_metric_aligns_gc_tick_with_matching_memory_sample() {
        use crate::monitor::MonitorState;

        // Memory samples spaced unevenly on the agent clock. A GC event
        // shares the ts_ns of the middle memory sample. The GC tick column
        // must equal the column where that sample is plotted on the chart.
        let mut state = MonitorState::new();
        state.push_memory(0, 100, 50, 10);
        state.push_memory(1_500_000_000, 110, 55, 11); // 1.5 s in
        state.push_memory(5_000_000_000, 120, 60, 12); // 5.0 s in
        state.push_gc(1_500_000_000, 42);

        let metrics = build_metrics(&state, &[], None);
        let mem = metrics.iter().find(|m| m.name == "Java").unwrap();

        // window_secs is the spread of the memory samples (5 s).
        assert!((mem.window_secs - 5.0).abs() < 1e-9);

        // Per-sample secs_ago: 5, 3.5, 0 (oldest → newest).
        assert_eq!(mem.secs_ago.len(), 3);
        assert!((mem.secs_ago[0] - 5.0).abs() < 1e-9);
        assert!((mem.secs_ago[1] - 3.5).abs() < 1e-9);
        assert!((mem.secs_ago[2] - 0.0).abs() < 1e-9);

        // GC tick rounds 3.5 s to 3 (u32 floor at second granularity). The
        // tick row column for "3 s ago" with window=5 in a 21-cell strip:
        let width = 21;
        let row = build_tick_row(width, mem.window_secs, &mem.tick_secs_ago);
        let tick_col = row.chars().position(|c| c == '◆').unwrap();
        // (1 - 3/5) * 20 = 8.
        assert_eq!(tick_col, 8);
    }

    #[test]
    fn memory_metric_uses_java_heap() {
        use crate::monitor::MonitorState;

        let mut state = MonitorState::new();
        state.push_memory(0, 300_000, 42_000, 8_000);
        state.push_memory(1_000_000_000, 310_000, 50_000, 9_000);

        let metrics = build_metrics(&state, &[], None);
        let mem = metrics.iter().find(|m| m.name == "Java").unwrap();
        assert_eq!(mem.values, vec![42_000.0, 50_000.0]);
        assert!(mem.label.contains("Java"));
    }
}
