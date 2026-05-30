
use tui::{
  style::{Color, Modifier, Style},
  widgets::{Block, Borders, Cell, Row, Table},
  text::{Span},
  layout::{Constraint},
};
use chrono::{DateTime, Utc};

use crate::fetch::fetch_query_log::{Query, Question};
pub fn make_query_table(data: &[Query], width: u16) -> Table<'_> {
  let rows = data.iter().map(|query| {
      let time = Cell::from(
          time_ago(query.time.as_str()).unwrap_or("unknown".to_string())
      ).style(Style::default().fg(Color::Gray));
      
      let question = Cell::from(make_request_cell(&query.question).unwrap())
          .style(Style::default().add_modifier(Modifier::BOLD));

      let client_names = query.client_info
          .as_ref()
          .and_then(|info| info.name.as_deref())
          .filter(|name| !name.is_empty())
          .map_or_else(|| query.client.as_str(), |name| name);
      
      let client = Cell::from(client_names)
          .style(Style::default().fg(Color::Blue));

      let (time_taken, elapsed_color) = make_time_taken_and_color(&query.elapsed_ms).unwrap();
      let elapsed_ms = Cell::from(time_taken).style(Style::default().fg(elapsed_color));

      let (status_txt, status_color) = block_status_text(&query.reason, query.cached);
      let status = Cell::from(status_txt).style(Style::default().fg(status_color));

      let upstream = Cell::from(query.upstream.as_str()).style(Style::default().fg(Color::Blue));

      let color = make_row_color(&query.reason);
      Row::new(vec![time, question, status, elapsed_ms, client, upstream])
          .style(Style::default().fg(color))
  }).collect::<Vec<Row>>();

  
  let title = Span::styled(
    "Query Log",
    Style::default().add_modifier(Modifier::BOLD),
  );

  let block = Block::default()
      .title(title)
      .borders(Borders::ALL);

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
fn make_request_cell(q: &Question) -> Result<String, anyhow::Error> {
  Ok(format!("[{}] {} - {}", q.class, q.question_type, q.name))
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
  } else if reason == "Rewrite" {
      Color::LightGreen
  } else {
      Color::Yellow
  }
}

// Return text and color for the status cell based on allow/ block reason
fn block_status_text(reason: &str, cached: bool) -> (String, Color) {
  let (text, color) =
  if reason == "FilteredBlackList" {
      ("Blacklisted".to_string(), Color::Red)
  } else if cached {
      ("Cached".to_string(), Color::Cyan)
  } else if reason == "Rewrite" {
      ("Rewrite".to_string(), Color::LightGreen)
  } else if reason == "NotFilteredNotFound" {
      ("Allowed".to_string(), Color::Green)
  } else {
      ("Other Block".to_string(), Color::Yellow)
  };
  (text, color)
}

