use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, EntryKind};

pub fn format_status_value(key: &str, value: &serde_json::Value) -> String {
    if key.contains("freq")
        && let Some(hz) = value.as_i64().or_else(|| value.as_f64().map(|f| f as i64))
    {
        return format_frequency(hz);
    }
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn format_frequency(hz: i64) -> String {
    let mhz = hz / 1_000_000;
    let khz = (hz % 1_000_000) / 1_000;
    let remainder = hz % 1_000;
    format!("{mhz}.{khz:03}.{remainder:03} MHz")
}

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(frame.area());

    draw_rig_panes(frame, app, chunks[0]);
    draw_repl(frame, app, chunks[1]);
}

fn draw_rig_panes(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.rigs.is_empty() {
        let block = Block::default()
            .title(" No rigs discovered ")
            .borders(Borders::ALL);
        frame.render_widget(block, area);
        return;
    }

    let constraints: Vec<Constraint> = app
        .rigs
        .iter()
        .map(|_| Constraint::Ratio(1, app.rigs.len() as u32))
        .collect();

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, rig) in app.rigs.iter().enumerate() {
        let status_text = if rig.connected {
            "connected"
        } else {
            "disconnected"
        };
        let status_color = if rig.connected {
            Color::Green
        } else {
            Color::Red
        };

        let title = format!(" Rig {} ({}) ", rig.rig_id, status_text);
        let border_style = Style::default().fg(status_color);

        let mut lines = Vec::new();
        if rig.status.is_empty() {
            lines.push(Line::from(Span::styled(
                "No status data",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            let mut keys: Vec<&String> = rig.status.keys().collect();
            keys.sort();
            for key in keys {
                let value = &rig.status[key];
                let value_str = format_status_value(key, value);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{key}: "),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(value_str),
                ]));
            }
        }

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, panes[i]);
    }
}

fn draw_repl(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let history_lines: Vec<Line> = app
        .repl_history
        .iter()
        .flat_map(|entry| {
            let (prefix, style) = match entry.kind {
                EntryKind::Command => ("> ", Style::default().fg(Color::Cyan)),
                EntryKind::Response => ("  ", Style::default().fg(Color::White)),
                EntryKind::Error => ("! ", Style::default().fg(Color::Red)),
            };
            entry
                .text
                .split("\n")
                .map(|line| {
                    Line::from(vec![Span::styled(prefix, style), Span::styled(line, style)])
                })
                .collect::<Vec<_>>()
        })
        .collect();

    let history_height = chunks[0].height.saturating_sub(2) as usize;
    let scroll = if history_lines.len() > history_height {
        (history_lines.len() - history_height) as u16
    } else {
        0
    };

    let history = Paragraph::new(history_lines)
        .block(Block::default().title(" Console ").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(history, chunks[0]);

    const INPUT_PREFIX: &str = "> ";

    let input = Paragraph::new(Line::from(vec![
        Span::styled(INPUT_PREFIX, Style::default().fg(Color::Yellow)),
        Span::raw(&app.input),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(input, chunks[1]);

    frame.set_cursor_position((
        chunks[1].x + (app.cursor_pos + INPUT_PREFIX.len()) as u16 + 1,
        chunks[1].y + 1,
    ));
}
