use tui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Modifier, Style},
  symbols,
  text::Span,
  widgets::{Axis, Block, Borders, Chart, Dataset, Widget},
};

use crate::fetch::fetch_stats::StatsResponse;

pub fn make_history_chart(stats: &StatsResponse) -> impl Widget + '_ {
  // Convert datasets into vector that can be consumed by chart
  let datasets = make_history_datasets(stats);
  // Find uppermost x and y-axis bounds for chart
  let (x_bound, y_bound) = find_bounds(stats);
  // AdGuard sends hourly buckets when stats retention is 7 days or less
  let (x_title, now) = if stats.time_units == "hours" {
    ("Time (Hours ago)", "Now")
  } else {
    ("Time (Days ago)", "Today")
  };
  // Generate incremental labels from data's values, to render on axis
  let x_labels = generate_x_labels(stats.dns_queries.len() as i32, 5, now);
  let y_labels = generate_y_labels(y_bound as i32, 5);
  // Create chart. The y-axis title is drawn afterwards; see HistoryChart.
  let chart = Chart::new(datasets)
    .block(
      Block::default()
        .title(Span::styled(
          "History",
          Style::default().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL),
    )
    .x_axis(
      Axis::default()
        .title(x_title)
        .bounds([0.0, x_bound])
        .labels(x_labels.clone()),
    )
    .y_axis(
      Axis::default()
        .labels(y_labels.clone())
        .bounds([0.0, y_bound]),
    );

  HistoryChart {
    chart,
    x_labels,
    y_labels,
  }
}

const QUERY_COUNT_TITLE: &str = "Query Count";

// ratatui 0.20 paints an axis title's style across the rest of that row
// (https://github.com/ratatui-org/ratatui/issues/379). On this chart that
// turns the braille to the right of "Query Count" white. Draw the label
// ourselves so only its own cells change.
struct HistoryChart<'a> {
  chart: Chart<'a>,
  x_labels: Vec<Span<'static>>,
  y_labels: Vec<Span<'static>>,
}

impl Widget for HistoryChart<'_> {
  fn render(self, area: Rect, buf: &mut Buffer) {
    // This copies ratatui 0.20's private chart layout, so it only matches that
    // version. ratatui 0.24 keeps the dataset color on this row, and
    // Axis::title can be used again after that upgrade.
    let origin = query_count_origin(area, &self.y_labels, &self.x_labels);
    self.chart.render(area, buf);
    if let Some((x, y)) = origin {
      // Color::Reset, not Style::default(): set_style ignores a missing fg, so a
      // default style would leave the label green where it covers braille.
      buf.set_string(x, y, QUERY_COUNT_TITLE, Style::default().fg(Color::Reset));
    }
  }
}

// Where ratatui 0.20 would place the y-axis title: top row of the graph,
// immediately to the right of the vertical axis.
fn query_count_origin(area: Rect, y_labels: &[Span], x_labels: &[Span]) -> Option<(u16, u16)> {
  let area = Block::default().borders(Borders::ALL).inner(area);
  if area.height == 0 || area.width == 0 {
    return None;
  }

  let mut x = area.left();
  let mut y = area.bottom() - 1;
  if y > area.top() {
    y -= 1;
  }
  x += label_column_width(area, y_labels, x_labels);
  if y > area.top() {
    y -= 1;
  }
  if x + 1 < area.right() {
    x += 1;
  }
  if x >= area.right() || y <= 1 {
    return None;
  }

  let graph_width = area.right() - x;
  let graph_height = y - area.top() + 1;
  let title_width = QUERY_COUNT_TITLE.len() as u16;
  if title_width + 1 < graph_width && graph_height > 2 {
    Some((x, area.top()))
  } else {
    None
  }
}

// Width reserved for y-axis labels and the part of the first x-axis label
// that sits left of the vertical axis. Mirrors ratatui 0.20's chart layout.
fn label_column_width(area: Rect, y_labels: &[Span], x_labels: &[Span]) -> u16 {
  let mut max_width = y_labels.iter().map(Span::width).max().unwrap_or(0) as u16;
  if let Some(first) = x_labels.first() {
    let left_of_axis = (first.width() as u16).saturating_sub(1);
    max_width = max_width.max(left_of_axis);
  }
  max_width.min(area.width / 3)
}

// Returns a dataset that's consumable by the chart widget
fn make_history_datasets(stats: &StatsResponse) -> Vec<Dataset<'_>> {
  let dns_queries_dataset = Dataset::default()
    .name("DNS Queries")
    .marker(symbols::Marker::Braille)
    .style(Style::default().fg(Color::Green))
    .data(&stats.dns_queries_chart);

  let blocked_filtering_dataset = Dataset::default()
    .name("Blocked Filtering")
    .marker(symbols::Marker::Braille)
    .style(Style::default().fg(Color::Red))
    .data(&stats.blocked_filtering_chart);

  let datasets = vec![dns_queries_dataset, blocked_filtering_dataset];

  datasets
}

// Determine the uppermost bounds for the x and y axis
fn find_bounds(stats: &StatsResponse) -> (f64, f64) {
  let mut max_length = 0;
  // Floor at 0 so empty data can't leave this at f64::MIN (which would later
  // overflow when cast to an axis-label step)
  let mut max_value = 0.0;

  for dataset in &[&stats.dns_queries_chart, &stats.blocked_filtering_chart] {
    let length = dataset.len();
    if length > max_length {
      max_length = length;
    }

    let max_in_dataset = dataset.iter().map(|&(_, y)| y).fold(f64::MIN, f64::max);
    if max_in_dataset > max_value {
      max_value = max_in_dataset;
    }
  }
  (max_length as f64, max_value)
}

// Generate periodic labels to render on the y-axis (query count)
fn generate_y_labels(max: i32, count: usize) -> Vec<Span<'static>> {
  let step = max / (count - 1) as i32;

  (0..count)
    .map(|x| Span::raw(format!("{}", x * step as usize)))
    .collect::<Vec<Span<'static>>>()
}

// Generate periodic labels to render on the x-axis (hours or days ago)
fn generate_x_labels(len: i32, num_labels: i32, now: &'static str) -> Vec<Span<'static>> {
  // The newest bucket is the current one, so the oldest is `len - 1` units ago
  let oldest = (len - 1).max(0) as f64;
  (0..num_labels)
    .map(|i| {
      if i == num_labels - 1 {
        Span::styled(now, Style::default().add_modifier(Modifier::BOLD))
      } else {
        let ago = oldest * (num_labels - 1 - i) as f64 / (num_labels - 1) as f64;
        Span::raw((ago.round() as i32).to_string())
      }
    })
    .collect()
}

// Formats vector data into a format that can be consumed by the chart widget
fn convert_to_chart_data(data: Vec<f64>) -> Vec<(f64, f64)> {
  data
    .iter()
    .enumerate()
    .map(|(i, &v)| (i as f64, v))
    .collect()
}

// Interpolates data, adding n number of points, to make the chart look smoother
fn interpolate(input: &[f64], points_between: usize) -> Vec<f64> {
  // Nothing to interpolate between - avoids a panic on the `last()` below
  let Some(&last) = input.last() else {
    return Vec::new();
  };

  let mut output = Vec::new();
  for window in input.windows(2) {
    let start = window[0];
    let end = window[1];
    let step = (end - start) / (points_between as f64 + 1.0);

    output.push(start);
    for i in 1..=points_between {
      output.push(start + step * i as f64);
    }
  }

  output.push(last);
  output
}

// Adds data formatted for the time-series chart to the stats object
pub fn prepare_chart_data(stats: &mut StatsResponse) {
  let dns_queries = stats
    .dns_queries
    .iter()
    .map(|&v| v as f64)
    .collect::<Vec<_>>();
  let interpolated_dns_queries = interpolate(&dns_queries, 3);
  stats.dns_queries_chart = convert_to_chart_data(interpolated_dns_queries);

  let blocked_filtering: Vec<f64> = stats
    .blocked_filtering
    .iter()
    .zip(&stats.replaced_safebrowsing)
    .zip(&stats.replaced_parental)
    .map(|((&b, &s), &p)| (b + s + p) as f64)
    .collect();

  let interpolated_blocked_filtering = interpolate(&blocked_filtering, 3);
  let blocked_filtering_chart: Vec<(f64, f64)> =
    convert_to_chart_data(interpolated_blocked_filtering);

  stats.blocked_filtering_chart = blocked_filtering_chart;
}

#[cfg(test)]
mod tests {
  use super::*;
  use tui::{buffer::Buffer, layout::Rect, symbols::line, widgets::Widget};

  #[test]
  fn x_labels_count_back_from_the_newest_bucket() {
    let labels = |len| -> Vec<String> {
      let spans = generate_x_labels(len, 5, "Now");
      spans.into_iter().map(|s| s.content.into_owned()).collect()
    };
    assert_eq!(labels(24), ["23", "17", "12", "6", "Now"]);
    assert_eq!(labels(30), ["29", "22", "15", "7", "Now"]);
    assert_eq!(labels(0), ["0", "0", "0", "0", "Now"]);
  }

  #[test]
  fn query_count_label_leaves_the_top_braille_row_in_the_dataset_color() {
    // Green is the reported bug. Red covers a peak that shares that same row,
    // so a fix that paints the whole row green would fail this case.
    assert_top_row_keeps_color(100, 0, Color::Green);
    assert_top_row_keeps_color(100, 100, Color::Red);
  }

  fn assert_top_row_keeps_color(queries: u64, blocked: u64, color: Color) {
    let mut stats = StatsResponse {
      time_units: "hours".to_string(),
      dns_queries: vec![queries; 24],
      blocked_filtering: vec![blocked; 24],
      replaced_safebrowsing: vec![0; 24],
      replaced_parental: vec![0; 24],
      ..StatsResponse::default()
    };
    prepare_chart_data(&mut stats);

    let area = Rect::new(0, 0, 60, 14);
    let mut buf = Buffer::empty(area);
    make_history_chart(&stats).render(area, &mut buf);

    let (x, y) = find_label(&buf).unwrap_or_else(|| panic!("label missing:\n{}", dump(&buf)));
    assert_eq!(
      buf.get(x - 1, y).symbol,
      line::VERTICAL,
      "label not against the axis:\n{}",
      dump(&buf)
    );
    assert_eq!(
      buf.get(x, y).fg,
      Color::Reset,
      "label should stay the default foreground"
    );

    let mut braille = 0;
    for col in (x + QUERY_COUNT_TITLE.len() as u16)..area.width {
      let cell = buf.get(col, y);
      if !is_braille(&cell.symbol) {
        continue;
      }
      braille += 1;
      assert_eq!(
        cell.fg,
        color,
        "braille at ({col}, {y}) is {:?}, row:\n{}",
        cell.fg,
        dump(&buf)
      );
    }
    assert!(
      braille > 0,
      "no braille to the right of the label:\n{}",
      dump(&buf)
    );
  }

  fn find_label(buf: &Buffer) -> Option<(u16, u16)> {
    let title = QUERY_COUNT_TITLE;
    let width = title.len() as u16;
    (0..buf.area.height).find_map(|y| {
      (0..buf.area.width.saturating_sub(width)).find_map(|x| {
        let text: String = (0..width)
          .map(|i| buf.get(x + i, y).symbol.clone())
          .collect();
        (text == title).then_some((x, y))
      })
    })
  }

  fn is_braille(symbol: &str) -> bool {
    symbol
      .chars()
      .any(|c| ('\u{2800}'..='\u{28FF}').contains(&c))
  }

  fn dump(buf: &Buffer) -> String {
    (0..buf.area.height)
      .map(|y| {
        (0..buf.area.width)
          .map(|x| buf.get(x, y).symbol.as_str())
          .collect::<String>()
      })
      .collect::<Vec<_>>()
      .join("\n")
  }
}
