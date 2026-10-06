use chrono::{DateTime, Utc};
use tui::{
  layout::Constraint,
  style::{Color, Modifier, Style},
  text::Span,
  widgets::{Block, Borders, Cell, Row, Table},
};

use crate::fetch::fetch_query_log::{Query, Question};
pub fn make_query_table(data: &[Query], width: u16) -> Table<'_> {
  let rows = data
    .iter()
    .map(|query| {
      let time = Cell::from(time_ago(query.time.as_str()).unwrap_or("unknown".to_string()))
        .style(Style::default().fg(Color::Gray));

      let question = Cell::from(make_request_cell(&query.question))
        .style(Style::default().add_modifier(Modifier::BOLD));

      let client = Cell::from(client_label(query)).style(Style::default().fg(Color::Blue));

      let (time_taken, elapsed_color) = make_time_taken_and_color(&query.elapsed_ms)
        .unwrap_or_else(|_| ("? ms".to_string(), Color::Gray));
      let elapsed_ms = Cell::from(time_taken).style(Style::default().fg(elapsed_color));

      let (status_txt, status_color) = block_status_text(&query.reason, query.cached);
      let status = Cell::from(status_txt).style(Style::default().fg(status_color));

      let upstream = Cell::from(query.upstream.as_str()).style(Style::default().fg(Color::Blue));

      let color = make_row_color(&query.reason);
      Row::new(vec![time, question, status, elapsed_ms, client, upstream])
        .style(Style::default().fg(color))
    })
    .collect::<Vec<Row>>();

  let title = Span::styled("Query Log", Style::default().add_modifier(Modifier::BOLD));

  let block = Block::default().title(title).borders(Borders::ALL);

  let mut headers = vec![
    Cell::from(Span::raw("Time")),
    Cell::from(Span::raw("Request")),
    Cell::from(Span::raw("Status")),
    Cell::from(Span::raw("Time Taken")),
  ];

  if width > 120 {
    headers.extend(vec![
      Cell::from(Span::raw("Client")),
      Cell::from(Span::raw("Upstream DNS")),
    ]);

    let widths = &[
      Constraint::Percentage(15),
      Constraint::Percentage(35),
      Constraint::Percentage(10),
      Constraint::Percentage(10),
      Constraint::Percentage(15),
      Constraint::Percentage(15),
    ];

    Table::new(rows)
      .header(Row::new(headers))
      .widths(widths)
      .block(block)
  } else {
    let widths = &[
      Constraint::Percentage(20),
      Constraint::Percentage(40),
      Constraint::Percentage(20),
      Constraint::Percentage(20),
    ];

    Table::new(rows)
      .header(Row::new(headers))
      .widths(widths)
      .block(block)
  }
}

// Given a timestamp, return a string representing how long ago that was
fn time_ago(timestamp: &str) -> Result<String, anyhow::Error> {
  let datetime = DateTime::parse_from_rfc3339(timestamp)?;
  let datetime_utc = datetime.with_timezone(&Utc);
  let now = Utc::now();

  let duration = now - datetime_utc;

  if duration.num_minutes() < 1 {
    Ok(format!("{} sec ago", duration.num_seconds()))
  } else {
    Ok(format!("{} min ago", duration.num_minutes()))
  }
}

// Return cell showing info about the request made in a given query
fn make_request_cell(q: &Question) -> String {
  format!("[{}] {} - {}", q.class, q.question_type, q.name)
}

// Return the client's name if AdGuard knows it, otherwise its IP
fn client_label(q: &Query) -> &str {
  match &q.client_info {
    Some(info) if !info.name.is_empty() => &info.name,
    _ => &q.client,
  }
}

// Return a cell showing the time taken for a query, and a color based on time
fn make_time_taken_and_color(elapsed: &str) -> Result<(String, Color), anyhow::Error> {
  let elapsed_f64 = elapsed.parse::<f64>()?;
  let rounded_elapsed = (elapsed_f64 * 100.0).round() / 100.0;
  let time_taken = format!("{:.2} ms", rounded_elapsed);
  let color = if elapsed_f64 < 1.0 {
    Color::Green
  } else if (1.0..=20.0).contains(&elapsed_f64) {
    Color::Yellow
  } else {
    Color::Red
  };
  Ok((time_taken, color))
}

// Return color for a row, based on the allow/block reason
fn make_row_color(reason: &str) -> Color {
  if reason == "NotFilteredNotFound" {
    Color::Green
  } else if reason == "FilteredBlackList" {
    Color::Red
  } else {
    Color::Yellow
  }
}

// Return text and color for the status cell based on allow/ block reason
fn block_status_text(reason: &str, cached: bool) -> (String, Color) {
  let (text, color) = if reason == "FilteredBlackList" {
    ("Blacklisted".to_string(), Color::Red)
  } else if cached {
    ("Cached".to_string(), Color::Cyan)
  } else if reason == "NotFilteredNotFound" {
    ("Allowed".to_string(), Color::Green)
  } else {
    ("Other Block".to_string(), Color::Yellow)
  };
  (text, color)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn client_label_prefers_name_and_falls_back_to_ip() {
    let label = |json: &str| client_label(&serde_json::from_str(json).unwrap()).to_string();
    let named = r#"{"client":"1.2.3.4","client_info":{"name":"laptop"}}"#;
    assert_eq!(label(named), "laptop");
    for unnamed in [
      r#"{"client":"1.2.3.4","client_info":{"name":""}}"#,
      r#"{"client":"1.2.3.4","client_info":null}"#,
      r#"{"client":"1.2.3.4"}"#,
    ] {
      assert_eq!(label(unnamed), "1.2.3.4");
    }
  }
}
