mod app;
mod commands;
mod input;
mod ui;

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use serde_json::Value;
use tokio::sync::mpsc;

use holyrig_client::client::HolyrigClient;
use holyrig_client::protocol::{ParsedNotification, ServerMessage};

use app::App;
use commands::Command;
use input::InputAction;

pub async fn run(addr: SocketAddr) -> Result<()> {
    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(64);
    let mut client = HolyrigClient::connect(addr, msg_tx).await?;

    let mut app = App::new();
    for info in client.rigs.values() {
        app.add_rig(info.id, info.connected);
        app.set_capabilities(info.id, info.capabilities.clone());
        app.update_status(info.id, info.status.clone());
    }

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app, &mut client, &mut msg_rx).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    client: &mut HolyrigClient,
    msg_rx: &mut mpsc::Receiver<ServerMessage>,
) -> Result<()> {
    let tick_rate = Duration::from_millis(60);

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        while let Ok(msg) = msg_rx.try_recv() {
            handle_server_message(app, msg);
        }

        if event::poll(tick_rate)?
            && let Event::Key(key) = event::read()?
        {
            match input::handle_key_event(key, app) {
                InputAction::Submit(input) => {
                    handle_command(app, client, &input).await;
                }
                InputAction::Quit => {
                    app.should_quit = true;
                }
                InputAction::None => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

async fn handle_command(app: &mut App, client: &mut HolyrigClient, input: &str) {
    match commands::parse_command(input, app) {
        Ok(Command::Help) => {
            app.push_response(commands::help_text());
        }
        Ok(Command::ListRigs) => {
            let request = client.list_rigs_request();
            send_and_display(app, client, &request).await;
        }
        Ok(Command::Caps { rig_id }) => {
            if let Some(rig) = app.rigs.iter().find(|r| r.rig_id == rig_id) {
                if let Some(caps) = &rig.capabilities {
                    let mut lines = Vec::new();

                    lines.push("Commands:".to_string());
                    let mut cmd_names: Vec<&String> = caps.commands.keys().collect();
                    cmd_names.sort();
                    for name in cmd_names {
                        let params = &caps.commands[name];
                        if params.is_empty() {
                            lines.push(format!("  {name}"));
                        } else {
                            let params_str: Vec<String> = params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, p.param_type))
                                .collect();
                            lines.push(format!("  {name}({})", params_str.join(", ")));
                        }
                    }

                    lines.push("Status fields:".to_string());
                    let mut field_names: Vec<&String> = caps.status_fields.keys().collect();
                    field_names.sort();
                    for name in field_names {
                        let typ = &caps.status_fields[name];
                        lines.push(format!("  {name}: {typ}"));
                    }

                    app.push_response(lines.join("\n"));
                } else {
                    app.push_response("No capabilities available".into());
                }
            } else {
                app.push_error(format!("Unknown rig {rig_id}"));
            }
        }
        Ok(Command::Execute {
            rig_id,
            command,
            parameters,
        }) => {
            let request = client.execute_command_request(rig_id, command, parameters);
            send_and_display(app, client, &request).await;
        }
        Err(e) => {
            app.push_error(e.to_string());
        }
    }
}

async fn send_and_display(
    app: &mut App,
    client: &mut HolyrigClient,
    request: &holyrig_client::protocol::Request,
) {
    if let Err(e) = client.send_request(request).await {
        app.push_error(format!("Send failed: {e}"));
    }
}

fn handle_server_message(app: &mut App, msg: ServerMessage) {
    match msg {
        ServerMessage::Response(resp) => {
            if let Some(err) = resp.error {
                app.push_error(err.to_string());
            } else if let Some(result) = resp.result {
                app.push_response(format_value(&result));
            }
        }
        ServerMessage::Notification(notif) => match notif.parse() {
            ParsedNotification::StatusUpdate { rig_id, updates } => {
                app.update_status(rig_id, updates);
            }
            ParsedNotification::ConnectionUpdate { rig_id, connected } => {
                app.set_connected(rig_id, connected);
            }
            ParsedNotification::Unknown => {}
        },
    }
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let pairs: Vec<String> = map.iter().map(|(k, v)| format!("{k}: {v}")).collect();
            pairs.join(", ")
        }
        other => other.to_string(),
    }
}
