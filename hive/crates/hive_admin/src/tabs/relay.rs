use crate::app::App;
use ratatui::{
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Row, Table},
};

const CYAN: Color = Color::Rgb(0, 212, 255);

fn format_bytes(b: u64) -> String {
    if b >= 1_073_741_824 {
        format!("{:.1} GB", b as f64 / 1_073_741_824.0)
    } else if b >= 1_048_576 {
        format!("{:.1} MB", b as f64 / 1_048_576.0)
    } else if b >= 1_024 {
        format!("{:.1} KB", b as f64 / 1_024.0)
    } else {
        format!("{} B", b)
    }
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let stats = match &app.relay {
        Some(s) => s,
        None => {
            frame.render_widget(Paragraph::new("Loading..."), area);
            return;
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let summary = Paragraph::new(format!(
        "Active Rooms: {}  |  Connected Devices: {}",
        stats.active_rooms, stats.connected_devices
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Relay Status "),
    )
    .style(Style::default().fg(CYAN));
    frame.render_widget(summary, chunks[0]);

    let header = Row::new(vec![
        "Room ID",
        "Participants",
        "Created",
        "Bytes Transferred",
    ])
    .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    let rows: Vec<Row> = stats
        .rooms
        .iter()
        .map(|r| {
            Row::new(vec![
                r.room_id.clone(),
                r.participants.to_string(),
                r.created_at.format("%H:%M:%S").to_string(),
                format_bytes(r.bytes_transferred),
            ])
        })
        .collect();
    let widths = [
        Constraint::Min(15),
        Constraint::Length(14),
        Constraint::Length(12),
        Constraint::Length(18),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Rooms "))
        .row_highlight_style(Style::default().bg(Color::Rgb(40, 40, 60)));
    frame.render_stateful_widget(table, chunks[1], &mut app.table_state);
}
