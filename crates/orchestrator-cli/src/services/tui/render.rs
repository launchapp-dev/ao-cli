use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::services::tui::app_state::{ActivePane, AppState, TuiMode, PALETTE_ITEMS};

pub(crate) fn render(frame: &mut Frame<'_>, app: &AppState) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(5),
        ])
        .split(frame.area());

    let selected = app
        .selected_profile()
        .map(|profile| format!("{} / {}", profile.tool, profile.model_id))
        .unwrap_or_else(|| "none".to_string());
    let header = Paragraph::new(format!(
        "AO Agent Console (MCP locked to ao mcp serve)\nMCP endpoint: {}\nSelected: {}",
        app.mcp_endpoint, selected
    ))
    .block(Block::default().borders(Borders::ALL).title("Session"));
    frame.render_widget(header, root[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .split(root[1]);

    let models_focused = matches!(app.active_pane, ActivePane::Models);
    let filtered_profiles = app.filtered_profiles();
    let model_items: Vec<ListItem<'_>> = filtered_profiles
        .iter()
        .map(|(orig_idx, profile)| {
            let marker = if *orig_idx == app.selected_profile_idx { ">" } else { " " };
            let detail = profile
                .details
                .as_deref()
                .map(|value| format!(" ({value})"))
                .unwrap_or_default();
            let item = ListItem::new(format!("{marker} {}{detail}", profile.label()));
            if models_focused && *orig_idx == app.selected_profile_idx {
                item.style(Style::default().add_modifier(Modifier::BOLD))
            } else {
                item
            }
        })
        .collect();
    let models_title = if !app.search_filter.is_empty() && models_focused {
        format!("Models [/{}]", app.search_filter)
    } else {
        "Models (j/k)".to_string()
    };
    let models_block = Block::default()
        .borders(Borders::ALL)
        .title(models_title)
        .border_style(if models_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });
    let model_list = List::new(model_items).block(models_block);
    frame.render_widget(model_list, body[0]);

    let output_focused = matches!(app.active_pane, ActivePane::Output);
    let visible_output_lines = (body[1].height as usize).saturating_sub(2);
    let output_lines = app
        .history_lines_scrolled(visible_output_lines)
        .into_iter()
        .map(ListItem::new)
        .collect::<Vec<_>>();
    let output_title = if output_focused {
        format!(
            "Agent Output [scroll:{}]",
            app.output_scroll
        )
    } else {
        "Agent Output".to_string()
    };
    let output_block = Block::default()
        .borders(Borders::ALL)
        .title(output_title)
        .border_style(if output_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });
    let output_list = List::new(output_lines).block(output_block);
    frame.render_widget(output_list, body[1]);

    let tasks_focused = matches!(app.active_pane, ActivePane::Tasks);
    let filtered_tasks = app.filtered_tasks();
    let task_items: Vec<ListItem<'_>> = filtered_tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let item = ListItem::new(task.label());
            if tasks_focused && i == app.task_selected_idx {
                item.style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                item
            }
        })
        .collect();
    let tasks_title = if !app.search_filter.is_empty() {
        format!("Tasks [/{}]", app.search_filter)
    } else if tasks_focused {
        "Tasks (active)".to_string()
    } else {
        "Prioritized Tasks".to_string()
    };
    let tasks_block = Block::default()
        .borders(Borders::ALL)
        .title(tasks_title)
        .border_style(if tasks_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });
    let task_list = List::new(task_items).block(tasks_block);
    frame.render_widget(task_list, body[2]);

    let mode_line = match &app.mode {
        TuiMode::Search { query } => format!("/{query}_"),
        TuiMode::CreatingTask { title } => format!("New task: {title}_"),
        _ => format!("Prompt: {}", app.prompt),
    };
    let footer = Paragraph::new(format!(
        "Status: {}\nMode: {}\n{}\nCtrl+K=palette  ?=help  /=search  H/L=pane  gg/G=jump  r=refresh  q=quit",
        app.status_line,
        if app.print_mode { "print/raw" } else { "summary" },
        mode_line,
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"))
    .wrap(Wrap { trim: false });
    frame.render_widget(footer, root[2]);

    match &app.mode {
        TuiMode::CommandPalette { selected_idx } => {
            render_palette_overlay(frame, *selected_idx, frame.area());
        }
        TuiMode::HelpOverlay => {
            render_help_overlay(frame, frame.area());
        }
        _ => {}
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn render_palette_overlay(frame: &mut Frame<'_>, selected_idx: usize, area: Rect) {
    let popup_height = PALETTE_ITEMS.len() as u16 + 2;
    let popup_width = 40u16;
    let popup_area = centered_rect(popup_width, popup_height, area);
    frame.render_widget(Clear, popup_area);

    let items: Vec<ListItem<'_>> = PALETTE_ITEMS
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let item = ListItem::new(format!("  {label}"));
            if i == selected_idx {
                item.style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                item
            }
        })
        .collect();
    let palette = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Command Palette ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(palette, popup_area);
}

fn render_help_overlay(frame: &mut Frame<'_>, area: Rect) {
    let lines = [
        " Navigation",
        "  j / J / Down   Move down in active pane",
        "  k / K / Up     Move up in active pane",
        "  H              Move focus to left pane",
        "  L              Move focus to right pane",
        "  gg             Jump to top of pane",
        "  G              Jump to bottom of pane",
        "",
        " Search",
        "  /              Start search (Enter to apply)",
        "  Esc            Clear active search filter",
        "",
        " Actions",
        "  Ctrl+K         Open command palette",
        "  Enter          Run selected agent with prompt",
        "  r              Refresh models and tasks",
        "  p              Toggle print/summary mode",
        "  Ctrl+L         Clear output pane",
        "",
        "  ?              Toggle this help overlay",
        "  q / Ctrl+C     Quit",
        "",
        "  Press any key to close",
    ];
    let popup_height = (lines.len() + 2) as u16;
    let popup_width = 54u16;
    let popup_area = centered_rect(popup_width, popup_height, area);
    frame.render_widget(Clear, popup_area);

    let paragraph = Paragraph::new(lines.join("\n"))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .border_style(Style::default().fg(Color::Green)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup_area);
}
