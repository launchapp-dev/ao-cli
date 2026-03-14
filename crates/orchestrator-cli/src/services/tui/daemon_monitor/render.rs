use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::state::DaemonSnapshot;

pub(super) fn render(frame: &mut Frame<'_>, state: &DaemonSnapshot) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, root[0]);
    render_body(frame, state, root[1]);
    render_footer(frame, state, root[2]);
}

fn render_header(frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    let title = " DAEMON STATUS DASHBOARD ";
    let header = Paragraph::new(title)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL).title(" AO "));
    frame.render_widget(header, area);
}

fn render_body(frame: &mut Frame<'_>, state: &DaemonSnapshot, area: ratatui::layout::Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    render_daemon_health(frame, state, columns[0]);
    render_task_queue(frame, state, columns[1]);
    render_active_agents(frame, state, columns[2]);
    render_recent_errors(frame, state, columns[3]);
}

fn render_daemon_health(
    frame: &mut Frame<'_>,
    state: &DaemonSnapshot,
    area: ratatui::layout::Rect,
) {
    let status = state.daemon_status();
    let status_color = match status {
        "Running" => Color::Green,
        "Paused" => Color::Yellow,
        "Starting" | "Stopping" => Color::Blue,
        "Stopped" => Color::DarkGray,
        "Crashed" => Color::Red,
        _ => Color::White,
    };

    let daemon_pid = state
        .daemon_pid()
        .map(|p| p.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    let runner_pid = state
        .runner_pid()
        .map(|p| p.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    let runner_status = if state.is_runner_connected() {
        "Connected"
    } else {
        "Disconnected"
    };
    let runner_color = if state.is_runner_connected() {
        Color::Green
    } else {
        Color::Red
    };

    let items = vec![
        ListItem::new(Line::from(vec![
            Span::raw("Status: "),
            Span::styled(
                status,
                Style::default()
                    .fg(status_color)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("Daemon PID: "),
            Span::raw(&daemon_pid),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("Runner: "),
            Span::styled(runner_status, Style::default().fg(runner_color)),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("Runner PID: "),
            Span::raw(&runner_pid),
        ])),
    ];

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Daemon Health"),
    );
    frame.render_widget(list, area);
}

fn render_task_queue(frame: &mut Frame<'_>, state: &DaemonSnapshot, area: ratatui::layout::Rect) {
    let total = state.task_total();
    let ready = state.task_ready();
    let in_progress = state.task_in_progress();
    let blocked = state.task_blocked();
    let on_hold = state.task_on_hold();

    let items = vec![
        ListItem::new(Line::from(vec![
            Span::raw("Total: "),
            Span::styled(total.to_string(), Style::default().fg(Color::White)),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("Ready: "),
            Span::styled(ready.to_string(), Style::default().fg(Color::Cyan)),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("In Progress: "),
            Span::styled(in_progress.to_string(), Style::default().fg(Color::Yellow)),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("Blocked: "),
            Span::styled(blocked.to_string(), Style::default().fg(Color::Red)),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("On Hold: "),
            Span::styled(on_hold.to_string(), Style::default().fg(Color::DarkGray)),
        ])),
    ];

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Task Queue"));
    frame.render_widget(list, area);
}

fn render_active_agents(
    frame: &mut Frame<'_>,
    state: &DaemonSnapshot,
    area: ratatui::layout::Rect,
) {
    let active = state.active_agents();
    let max = state.max_agents().unwrap_or(0);
    let agent_count = format!("{active}/{max}");

    let count_color = if active > 0 {
        Color::Green
    } else {
        Color::DarkGray
    };

    let items = vec![ListItem::new(Line::from(vec![
        Span::raw("Active: "),
        Span::styled(
            agent_count,
            Style::default()
                .fg(count_color)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
    ]))];

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Active Agents"),
    );
    frame.render_widget(list, area);
}

fn render_recent_errors(
    frame: &mut Frame<'_>,
    state: &DaemonSnapshot,
    area: ratatui::layout::Rect,
) {
    let error_items: Vec<ListItem<'_>> = if state.recent_errors.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No recent errors",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        state
            .recent_errors
            .iter()
            .take(5)
            .map(|err| {
                let level_color = match err.level.as_str() {
                    "error" => Color::Red,
                    "warn" | "warning" => Color::Yellow,
                    "info" => Color::Blue,
                    _ => Color::White,
                };
                let ts = err.timestamp.format("%H:%M:%S");
                let line = format!("[{}] {}", ts, truncate(&err.message, 35));
                ListItem::new(Line::from(Span::styled(
                    line,
                    Style::default().fg(level_color),
                )))
            })
            .collect()
    };

    let list = List::new(error_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Recent Errors"),
    );
    frame.render_widget(list, area);
}

fn render_footer(frame: &mut Frame<'_>, state: &DaemonSnapshot, area: ratatui::layout::Rect) {
    let secs_ago = (chrono::Utc::now() - state.last_refresh).num_seconds();
    let help = format!(
        "d=toggle daemon  p=pause/resume  r=refresh  q=quit | {} ({}s ago)",
        state.status_line, secs_ago
    );
    let para = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(para, area);
}

fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => format!("{}...", &s[..i]),
        None => s.to_string(),
    }
}
