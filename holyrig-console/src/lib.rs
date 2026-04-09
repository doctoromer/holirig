mod app;
mod commands;
mod input;
mod ui;

use holyrig_client::capabilities::parse_capabilities;
use holyrig_client::net;
use holyrig_client::protocol;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Result, bail};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use serde_json::Value;
use tokio::sync::mpsc;

use app::App;
use commands::Command;
use input::InputAction;
use net::TcpClient;
use protocol::ServerMessage;

pub async fn run(addr: SocketAddr) -> Result<()> {
    let mut client = TcpClient::connect(addr).await?;
    let mut app = App::new();

    let list_req = protocol::list_rigs_request();
    let resp = client.send_and_wait(&list_req).await;
    let rigs_value = match resp {
        Ok(resp) => resp.result.unwrap_or(Value::Null),
        Err(e) => bail!("Cannot reach server at {addr}: {e}"),
    };

    if let Value::Object(rigs) = &rigs_value {
        for (id_str, connected) in rigs {
            let rig_id: usize = id_str.parse().unwrap_or(0);
            app.add_rig(rig_id, connected.as_bool().unwrap_or(false));
        }
    }

    for i in 0..app.rigs.len() {
        let rig_id = app.rigs[i].rig_id;
        let caps_request = protocol::get_capabilities_request(rig_id);
        if let Ok(response) = client.send_and_wait(&caps_request).await
            && let Some(result) = response.result
        {
            let caps = parse_capabilities(&result);
            let fields: Vec<String> = caps.status_fields.keys().cloned().collect();

            if !fields.is_empty() {
                let request = protocol::subscribe_status_request(rig_id, fields);
                let _ = client.send_and_wait(&request).await;
            }

            app.set_capabilities(rig_id, caps);

            let status_request = protocol::get_status_request(rig_id);
            if let Ok(response) = client.send_and_wait(&status_request).await
                && let Some(Value::Object(values)) = response.result
            {
                let updates: HashMap<String, Value> = values.into_iter().collect();
                app.update_status(rig_id, updates);
            }
        }
    }

    let (mut sender, receiver) = client.into_split();
    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(64);

    tokio::spawn(async move {
        let _ = receiver.run(msg_tx).await;
    });

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app, &mut sender, &mut msg_rx).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    sender: &mut net::TcpSender,
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
                    handle_command(app, sender, &input).await;
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

async fn handle_command(app: &mut App, sender: &mut net::TcpSender, input: &str) {
    match commands::parse_command(input, app) {
        Ok(Command::Help) => {
            app.push_response(commands::help_text());
        }
        Ok(Command::ListRigs) => {
            let request = protocol::list_rigs_request();
            send_and_display(app, sender, &request).await;
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
            let request = protocol::execute_command_request(rig_id, command, parameters);
            send_and_display(app, sender, &request).await;
        }
        Err(e) => {
            app.push_error(e.to_string());
        }
    }
}

async fn send_and_display(app: &mut App, sender: &mut net::TcpSender, request: &protocol::Request) {
    if let Err(e) = sender.send_request(request).await {
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
        ServerMessage::Notification(notification) => {
            let rig_id = notification.params.get("rig_id").and_then(|v| v.as_u64());
            match (notification.method.as_str(), rig_id) {
                ("status_update", Some(rig_id)) => {
                    let rig_id = rig_id as usize;
                    if let Some(updates) = notification
                        .params
                        .get("updates")
                        .and_then(|v| v.as_object())
                    {
                        let updates: HashMap<String, Value> = updates
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        app.update_status(rig_id, updates);
                    }
                }
                ("connection_update", Some(rig_id)) => {
                    let rig_id = rig_id as usize;
                    if let Some(connected) = notification
                        .params
                        .get("connected")
                        .and_then(|v| v.as_bool())
                    {
                        app.set_connected(rig_id, connected);
                    }
                }
                _ => {}
            }
        }
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
