use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Row, Table, Paragraph},
    style::{Color, Modifier, Style},
};
use crate::app::App;

const CYAN: Color = Color::Rgb(0, 212, 255);

fn format_bytes(b: u64) -> String {
    if b >= 1_073_741_824 { format!("{:.1} GB", b as f64 / 1_073_741_824.0) }
    else if b >= 1_048_576 { format!("{:.1} MB", b as f64 / 1_048_576.0) }
    else if b >= 1_024 { format!("{:.1} KB", b as f64 / 1_024.0) }
    else { format!("{} B", b) }
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let stats = match &app.sync_stats {
        Some(s) => s,
        None => { frame.render_widget(Paragraph::new("Loading..."), area); return; }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(8)])
        .split(area);

    let pct = if stats.storage_available_bytes > 0 { (stats.storage_used_bytes as f64 / stats.storage_available_bytes as f64) * 100.0 } else { 0.0 };
    let overview = Paragraph::new(format!("Blobs: {}  |  Used: {} / {} ({:.1}%)", stats.total_blobs, format_bytes(stats.storage_used_bytes), format_bytes(stats.storage_available_bytes), pct))
        .block(Block::default().borders(Borders::ALL).title(" Sync Overview "))
        .style(Style::default().fg(CYAN));
    frame.render_widget(overview, chunks[0]);

    let header = Row::new(vec!["User", "Blobs", "Storage"])
        .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    let rows: Vec<Row> = stats.per_user.iter().map(|u| {
        Row::new(vec![u.user_email.clone(), u.blob_count.to_string(), format_bytes(u.storage_bytes)])
    }).collect();
    let widths = [Constraint::Min(25), Constraint::Length(10), Constraint::Length(12)];
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Per-User Storage "))
        .row_highlight_style(Style::default().bg(Color::Rgb(40, 40, 60)));
    frame.render_stateful_widget(table, chunks[1], &mut app.table_state);

    let op_header = Row::new(vec!["Time", "User", "Op", "Key", "Size"])
        .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    let op_rows: Vec<Row> = stats.recent_ops.iter().map(|o| {
        let _op_color = match o.operation.as_str() {
            "PUT" => Color::Green,
            "DELETE" => Color::Red,
            _ => Color::White,
        };
        Row::new(vec![
            o.timestamp.format("%H:%M:%S").to_string(),
            o.user_email.clone(),
            o.operation.clone(),
            o.key.clone(),
            if o.bytes > 0 { format_bytes(o.bytes) } else { "-".into() },
        ])
    }).collect();
    let op_widths = [Constraint::Length(10), Constraint::Min(20), Constraint::Length(8), Constraint::Min(25), Constraint::Length(10)];
    let op_table = Table::new(op_rows, op_widths)
        .header(op_header)
        .block(Block::default().borders(Borders::ALL).title(" Recent Operations "));
    frame.render_widget(op_table, chunks[2]);
}
