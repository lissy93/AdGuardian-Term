use tui::{
  style::{Color, Modifier, Style},
  symbols,
  text::Span,
  widgets::{Axis, Block, Borders, Chart, Dataset},
};

use crate::fetch::fetch_stats::StatsResponse;

pub fn make_history_chart(stats: &StatsResponse) -> Chart<'_> {
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
  // Create chart
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
        .labels(x_labels),
    )
    .y_axis(
      Axis::default()
        .title("Query Count")
        .labels(y_labels)
        .bounds([0.0, y_bound]),
    );

  chart
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
}
