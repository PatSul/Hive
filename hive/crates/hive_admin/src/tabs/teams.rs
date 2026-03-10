use crate::app::App;
use ratatui::{
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Row, Table},
};

const CYAN: Color = Color::Rgb(0, 212, 255);

fn format_tokens(t: u64) -> String {
    if t >= 1_000_000 {
        format!("{:.1}M", t as f64 / 1_000_000.0)
    } else if t >= 1_000 {
        format!("{:.1}K", t as f64 / 1_000.0)
    } else {
        t.to_string()
    }
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(10)])
        .split(area);

    let header = Row::new(vec![
        "ID",
        "Name",
        "Members",
        "Plan",
        "Monthly Cost",
        "Usage",
    ])
    .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    let rows: Vec<Row> = app
        .teams
        .iter()
        .map(|t| {
            Row::new(vec![
                t.id.clone(),
                t.name.clone(),
                t.member_count.to_string(),
                t.plan.clone(),
                format!("${:.0}", t.monthly_cost),
                format_tokens(t.usage_tokens),
            ])
        })
        .collect();
    let widths = [
        Constraint::Length(10),
        Constraint::Min(20),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(14),
        Constraint::Length(10),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Teams "))
        .row_highlight_style(Style::default().bg(Color::Rgb(40, 40, 60)));
    frame.render_stateful_widget(table, chunks[0], &mut app.table_state);

    let selected_idx = app.table_state.selected();
    let detail = if let Some(idx) = selected_idx {
        if let Some(team) = app.teams.get(idx) {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}", team.name),
                        Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("  ({} members, {})", team.member_count, team.plan)),
                ]),
                Line::from(""),
            ];
            for m in &team.members {
                let role_color = match m.role.as_str() {
                    "Owner" => Color::Yellow,
                    "Admin" => Color::Cyan,
                    _ => Color::White,
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("  [{:6}]", m.role), Style::default().fg(role_color)),
                    Span::raw(format!(
                        " {} (joined {})",
                        m.email,
                        m.joined_at.format("%Y-%m-%d")
                    )),
                ]));
            }
            Paragraph::new(lines)
        } else {
            Paragraph::new("Select a team to view details")
        }
    } else {
        Paragraph::new("Select a team to view details")
    };
    frame.render_widget(
        detail.block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Team Details "),
        ),
        chunks[1],
    );
}
