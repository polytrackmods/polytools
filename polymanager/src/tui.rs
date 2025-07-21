use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use std::collections::VecDeque;
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
    let mut service_log_lines: VecDeque<String> = VecDeque::with_capacity(100);
    let mut manager_log_lines: VecDeque<String> = VecDeque::with_capacity(100);

    loop {
        if view_mode == ViewMode::Services {
            if let Some(service) = manager.config.services.get(service_index) {
                let log_path = PathBuf::from(format!("logs/{}.log", service.name));
                if log_path.exists() {
                    if let Ok(log) = fs::read_to_string(log_path).await {
                        service_log_lines = log
                            .lines()
                            .rev()
                            .take(100)
                            .map(std::string::ToString::to_string)
                            .collect::<VecDeque<_>>();
                        service_log_lines.make_contiguous().reverse();
                    }
                } else {
                    service_log_lines.clear();
                }
            }
        }

        terminal.draw(|f| {
            let layout = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Min(3),
            ])
            .margin(1)
            .split(f.area());

            // Title
            let title = Paragraph::new("Polymanager Dashboard")
                .style(Style::default().add_modifier(Modifier::BOLD))
                .centered();
            f.render_widget(title, layout[0]);

            // Services
            let service_items: Vec<ListItem> = manager
                .config
                .services
                .iter()
                .enumerate()
                .map(|(i, service)| {
                    let is_running = manager.is_service_running(&service.name);
                    let label = if is_running {
                        format!("[RUNNING] {}", service.name)
                    } else {
                        format!("[STOPPED] {}", service.name)
                    };
                    let (style, label) = if view_mode == ViewMode::Services && i == service_index {
                        (
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                            format!("* {label}"),
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
            f.render_widget(service_list, layout[2]);

            // Presets
            let preset_items: Vec<ListItem> = manager
                .config
                .presets
                .clone()
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(i, preset)| {
                    let (style, label) = if view_mode == ViewMode::Presets && i == preset_index {
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
            f.render_widget(preset_list, layout[3]);

            // Logs
            let log_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(layout[4]);
            // Service logs
            let service_log_text = service_log_lines
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let service_log_widget = Paragraph::new(service_log_text)
                .block(Block::default().borders(Borders::ALL).title("Service logs"))
                .style(Style::default().fg(Color::Gray));
            f.render_widget(service_log_widget, log_layout[0]);
            // Manager logs
            let manager_log_text = manager_log_lines
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let manager_log_widget = Paragraph::new(manager_log_text)
                .block(Block::default().borders(Borders::ALL).title("Manager logs"))
                .style(Style::default().fg(Color::Gray));
            f.render_widget(manager_log_widget, log_layout[1]);
        })?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
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
                    KeyCode::Char('r') => match view_mode {
                        ViewMode::Services => {
                            let service_name = &manager.config.services[service_index].name.clone();
                            if let Err(e) = manager.restart_service(service_name).await {
                                manager_log_lines
                                    .push_back(format!("Error restarting {service_name}: {e}"));
                            } else {
                                manager_log_lines
                                    .push_back(format!("Restarted service: {service_name}"));
                            }
                        }
                        ViewMode::Presets => {
                            let preset =
                                &manager.config.presets.clone().unwrap_or_default()[preset_index];
                            for service_name in &preset.services {
                                if let Err(e) = manager.restart_service(service_name).await {
                                    manager_log_lines.push_back(format!(
                                        "Error restarting {service_name}: {e}",
                                    ));
                                } else {
                                    manager_log_lines
                                        .push_back(format!("Restarted service: {service_name}"));
                                }
                            }
                            manager_log_lines
                                .push_back(format!("Restarted preset: {}", preset.name));
                        }
                    },
                    KeyCode::Enter => match view_mode {
                        ViewMode::Services => {
                            let service_name = &manager.config.services[service_index].name.clone();
                            if manager.is_service_running(service_name) {
                                if let Err(e) = manager.stop_service(service_name).await {
                                    manager_log_lines
                                        .push_back(format!("Error stopping {service_name}: {e}"));
                                } else {
                                    manager_log_lines
                                        .push_back(format!("Stopped service: {service_name}"));
                                }
                            } else if let Err(e) = manager.start_service(service_name) {
                                manager_log_lines
                                    .push_back(format!("Error starting {service_name}: {e}"));
                            } else {
                                manager_log_lines
                                    .push_back(format!("Started service: {service_name}"));
                            }
                        }
                        ViewMode::Presets => {
                            let preset =
                                &manager.config.presets.clone().unwrap_or_default()[preset_index];
                            for service_name in &preset.services {
                                if !manager.is_service_running(service_name) {
                                    if let Err(e) = manager.start_service(service_name) {
                                        manager_log_lines.push_back(format!(
                                            "Error starting {service_name}: {e}"
                                        ));
                                    } else {
                                        manager_log_lines
                                            .push_back(format!("Started service: {service_name}"));
                                    }
                                }
                            }
                            manager_log_lines
                                .push_back(format!("Activated preset: {}", preset.name));
                        }
                    },
                    _ => {}
                }
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
