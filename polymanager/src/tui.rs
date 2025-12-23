use ansi_to_tui::IntoText;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;

use crate::manager::ServiceManager;

#[allow(clippy::cognitive_complexity)]
#[allow(clippy::too_many_lines)]
pub async fn launch(manager: &mut ServiceManager) -> Result<()> {
    let mut terminal = ratatui::init();

    terminal.clear()?;

    let mut service_index = 0;
    let mut preset_index = 0;
    let mut view_mode = ViewMode::Services;
    let mut service_log_lines: Vec<String> = Vec::new();
    let mut manager_log_lines: Vec<String> = Vec::new();

    let mut max_log_scroll = 0;
    let mut scroll_pos = 0;
    let mut scrollbar_state = ScrollbarState::new(max_log_scroll);

    loop {
        if view_mode == ViewMode::Services
            && let Some(service) = manager.config.services.get(service_index)
        {
            let log_path = PathBuf::from(format!("logs/{}.log", service.name));
            if log_path.exists() {
                if let Ok(log) = fs::read_to_string(log_path).await {
                    service_log_lines = log
                        .lines()
                        .rev()
                        .take(100)
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>();
                    service_log_lines.reverse();
                }
            } else {
                service_log_lines.clear();
            }
        }

        terminal.draw(|f| {
            let [
                _,
                title_layout,
                _,
                service_layout,
                preset_layout,
                log_layout,
            ] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Min(3),
            ])
            .margin(0)
            .areas(f.area());

            let title = Line::raw("Polymanager Dashboard")
                .style(Style::default().add_modifier(Modifier::BOLD))
                .centered();
            f.render_widget(title, title_layout);

            let service_items: Vec<ListItem> = manager
                .config
                .services
                .iter()
                .enumerate()
                .map(|(index, service)| {
                    let is_running = manager.is_service_running(&service.name);
                    let label = if is_running {
                        format!("[RUNNING] {}", service.name)
                    } else {
                        format!("[STOPPED] {}", service.name)
                    };
                    let (style, label) = if index == service_index {
                        (
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                            if view_mode == ViewMode::Services {
                                format!("* {label}")
                            } else {
                                label
                            },
                        )
                    } else {
                        (
                            if is_running {
                                Style::default()
                                    .fg(Color::Magenta)
                                    .add_modifier(Modifier::ITALIC)
                            } else {
                                Style::default()
                            },
                            label,
                        )
                    };
                    ListItem::new(Span::styled(label, style))
                })
                .collect();
            let service_list = List::new(service_items)
                .block(Block::default().borders(Borders::ALL).title("Services"))
                .highlight_style(
                    Style::default()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                );
            f.render_widget(service_list, service_layout);

            let preset_items: Vec<ListItem> = manager
                .config
                .presets
                .clone()
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(index, preset)| {
                    let (style, label) = if view_mode == ViewMode::Presets && index == preset_index
                    {
                        (
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                            format!("* {}", preset.name.clone()),
                        )
                    } else {
                        (Style::default(), preset.name.clone())
                    };
                    ListItem::new(Span::styled(label, style))
                })
                .collect();
            let preset_list = List::new(preset_items)
                .block(Block::default().borders(Borders::ALL).title("Presets"))
                .highlight_style(
                    Style::default()
                        .fg(Color::LightCyan)
                        .add_modifier(Modifier::BOLD),
                );
            f.render_widget(preset_list, preset_layout);

            let [manager_log_layout, service_log_layout] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
                .areas(log_layout);
            let service_log_height = service_log_layout.height as usize - 2;
            let manager_log_height = manager_log_layout.height as usize - 2;
            max_log_scroll = service_log_lines.len().saturating_sub(service_log_height);
            let service_skip =
                scroll_pos.min(service_log_lines.len().saturating_sub(service_log_height));
            let service_log_text = service_log_lines
                .iter()
                .rev()
                .skip(service_skip)
                .take(service_log_height)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .into_text()
                .unwrap_or_default();
            let service_log_widget = Paragraph::new(service_log_text)
                .block(Block::default().borders(Borders::ALL).title("Service logs"))
                .style(Style::default().fg(Color::Gray));
            f.render_widget(service_log_widget, service_log_layout);
            // Manager logs
            let manager_log_text = manager_log_lines
                .iter()
                .rev()
                .take(manager_log_height)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .into_text()
                .unwrap_or_default();
            let manager_log_widget = Paragraph::new(manager_log_text)
                .block(Block::default().borders(Borders::ALL).title("Manager logs"))
                .style(Style::default().fg(Color::Gray));
            f.render_widget(manager_log_widget, manager_log_layout);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            scrollbar_state = scrollbar_state
                .position(max_log_scroll.saturating_sub(scroll_pos))
                .content_length(max_log_scroll);
            f.render_stateful_widget(scrollbar, service_log_layout, &mut scrollbar_state);
        })?;

        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('p') => {
                        view_mode = match view_mode {
                            ViewMode::Services => ViewMode::Presets,
                            ViewMode::Presets => ViewMode::Services,
                        };
                    }
                    KeyCode::Up => match view_mode {
                        ViewMode::Services => {
                            if service_index > 0 {
                                service_index -= 1;
                            } else {
                                service_index = manager.config.services.len().saturating_sub(1);
                            }
                        }
                        ViewMode::Presets => {
                            if preset_index > 0 {
                                preset_index -= 1;
                            } else {
                                preset_index = manager
                                    .config
                                    .presets
                                    .clone()
                                    .unwrap_or_default()
                                    .len()
                                    .saturating_sub(1);
                            }
                        }
                    },
                    KeyCode::Down => match view_mode {
                        ViewMode::Services => {
                            if service_index < manager.config.services.len().saturating_sub(1) {
                                service_index += 1;
                            } else {
                                service_index = 0;
                            }
                        }
                        ViewMode::Presets => {
                            if preset_index
                                < manager
                                    .config
                                    .presets
                                    .clone()
                                    .unwrap_or_default()
                                    .len()
                                    .saturating_sub(1)
                            {
                                preset_index += 1;
                            } else {
                                preset_index = 0;
                            }
                        }
                    },
                    KeyCode::Char('K') => {
                        scroll_pos = (scroll_pos + 1).min(max_log_scroll);
                    }
                    KeyCode::Char('J') => {
                        scroll_pos = scroll_pos.saturating_sub(1).min(max_log_scroll);
                    }
                    KeyCode::Char('g') => scroll_pos = 0,
                    KeyCode::Char('G') => scroll_pos = max_log_scroll,
                    KeyCode::Char('r') => match view_mode {
                        ViewMode::Services => {
                            let service = &manager.config.services[service_index].name.clone();
                            if let Err(e) = manager.restart_service(service).await {
                                manager_log_lines.push(format!("Error restarting {service}: {e}"));
                            } else {
                                manager_log_lines.push(format!("Restarted service: {service}"));
                            }
                        }
                        ViewMode::Presets => {
                            let preset =
                                &manager.config.presets.clone().unwrap_or_default()[preset_index];
                            for service in &preset.services {
                                if let Err(e) = manager.restart_service(service).await {
                                    manager_log_lines
                                        .push(format!("Error restarting {service}: {e}"));
                                } else {
                                    manager_log_lines.push(format!("Restarted service: {service}"));
                                }
                            }
                            manager_log_lines.push(format!("Restarted preset: {}", preset.name));
                        }
                    },
                    KeyCode::Enter => match view_mode {
                        ViewMode::Services => {
                            let service = &manager.config.services[service_index].name.clone();
                            if manager.is_service_running(service) {
                                if let Err(e) = manager.stop_service(service).await {
                                    manager_log_lines
                                        .push(format!("Error stopping {service}: {e}"));
                                } else {
                                    manager_log_lines.push(format!("Stopped service: {service}"));
                                }
                            } else if let Err(e) = manager.start_service(service) {
                                manager_log_lines.push(format!("Error starting {service}: {e}"));
                            } else {
                                manager_log_lines.push(format!("Started service: {service}"));
                            }
                        }
                        ViewMode::Presets => {
                            let preset =
                                &manager.config.presets.clone().unwrap_or_default()[preset_index];
                            for service in &preset.services {
                                if !manager.is_service_running(service) {
                                    if let Err(e) = manager.start_service(service) {
                                        manager_log_lines
                                            .push(format!("Error starting {service}: {e}"));
                                    } else {
                                        manager_log_lines
                                            .push(format!("Started service: {service}"));
                                    }
                                }
                            }
                            manager_log_lines.push(format!("Activated preset: {}", preset.name));
                        }
                    },
                    _ => {}
                },
                Event::Resize(_, _) => {
                    scroll_pos = scroll_pos.min(max_log_scroll);
                }
                _ => {}
            }
        }
    }

    ratatui::restore();
    Ok(())
}

#[derive(PartialEq)]
enum ViewMode {
    Services,
    Presets,
}
