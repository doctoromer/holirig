use std::collections::HashMap;

use serde_json::Value;

use crate::app::{App, Capabilities};

pub enum Command {
    ListRigs,
    Caps {
        rig_id: usize,
    },
    Execute {
        rig_id: usize,
        command: String,
        parameters: HashMap<String, Value>,
    },
    Help,
}

pub enum ParseError {
    UnknownCommand(String),
    MissingRigId {
        command: String,
    },
    InvalidRigId {
        command: String,
        got: String,
    },
    UnknownRigCommand {
        command: String,
        rig_id: usize,
    },
    WrongParamCount {
        command: String,
        rig_id: usize,
        usage: String,
    },
    InvalidParamValue {
        param: String,
        expected_type: String,
        got: String,
    },
    UsageError {
        usage: String,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnknownCommand(cmd) => write!(
                f,
                "Unknown command: '{cmd}'. Type 'help' for available commands."
            ),
            ParseError::MissingRigId { command } => {
                write!(f, "Missing rig_id. Usage: {command} <rig_id>")
            }
            ParseError::InvalidRigId { command, got } => {
                write!(f, "Invalid rig_id: '{got}'. Usage: {command} <rig_id>")
            }
            ParseError::UnknownRigCommand { command, rig_id } => {
                write!(
                    f,
                    "Unknown command '{command}' for rig {rig_id}. Use 'caps {rig_id}' to see available commands."
                )
            }
            ParseError::WrongParamCount {
                command,
                rig_id,
                usage,
            } => {
                write!(
                    f,
                    "Wrong number of parameters. Usage: {command} {rig_id} {usage}"
                )
            }
            ParseError::InvalidParamValue {
                param,
                expected_type,
                got,
            } => {
                write!(
                    f,
                    "Invalid value for '{param}': '{got}' is not a valid {expected_type}"
                )
            }
            ParseError::UsageError { usage } => write!(f, "Usage: {usage}"),
        }
    }
}

pub fn parse_command(input: &str, app: &App) -> Result<Command, ParseError> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return Err(ParseError::UnknownCommand(String::new()));
    }

    match parts[0] {
        "help" => Ok(Command::Help),
        "list_rigs" => Ok(Command::ListRigs),
        "caps" => {
            let rig_id = parse_rig_id(parts.get(1), "caps")?;
            Ok(Command::Caps { rig_id })
        }
        command => {
            let rig_id = match parse_rig_id(parts.get(1), command) {
                Ok(id) => id,
                Err(_) => {
                    let usage = command_usage(command, app);
                    return Err(ParseError::UsageError { usage });
                }
            };
            let rig = app.rigs.iter().find(|r| r.rig_id == rig_id);
            let caps = rig.and_then(|r| r.capabilities.as_ref());

            match caps {
                Some(caps) => resolve_execute(caps, rig_id, command, &parts[2..]),
                None => Ok(Command::Execute {
                    rig_id,
                    command: command.to_string(),
                    parameters: build_positional_params(&parts[2..]),
                }),
            }
        }
    }
}

fn command_usage(command: &str, app: &App) -> String {
    for rig in &app.rigs {
        if let Some(caps) = &rig.capabilities
            && let Some(params) = caps.commands.get(command)
        {
            if params.is_empty() {
                return format!("{command} <rig_id>");
            }
            let params_str: Vec<String> = params.iter().map(|p| format!("<{}>", p.name)).collect();
            return format!("{command} <rig_id> {}", params_str.join(" "));
        }
    }
    format!("{command} <rig_id> [params..]")
}

fn parse_rig_id(arg: Option<&&str>, command: &str) -> Result<usize, ParseError> {
    let s = arg.ok_or(ParseError::MissingRigId {
        command: command.to_string(),
    })?;
    s.parse().map_err(|_| ParseError::InvalidRigId {
        command: command.to_string(),
        got: s.to_string(),
    })
}

fn resolve_execute(
    caps: &Capabilities,
    rig_id: usize,
    command: &str,
    value_args: &[&str],
) -> Result<Command, ParseError> {
    let param_defs = caps
        .commands
        .get(command)
        .ok_or_else(|| ParseError::UnknownRigCommand {
            command: command.to_string(),
            rig_id,
        })?;

    if value_args.len() != param_defs.len() {
        let usage = if param_defs.is_empty() {
            String::new()
        } else {
            param_defs
                .iter()
                .map(|p| format!("<{}>", p.name))
                .collect::<Vec<_>>()
                .join(" ")
        };
        return Err(ParseError::WrongParamCount {
            command: command.to_string(),
            rig_id,
            usage,
        });
    }

    let mut parameters = HashMap::new();
    for (param, value_str) in param_defs.iter().zip(value_args.iter()) {
        let value = coerce_value(value_str, &param.param_type).map_err(|_| {
            ParseError::InvalidParamValue {
                param: param.name.clone(),
                expected_type: param.param_type.clone(),
                got: value_str.to_string(),
            }
        })?;
        parameters.insert(param.name.clone(), value);
    }

    Ok(Command::Execute {
        rig_id,
        command: command.to_string(),
        parameters,
    })
}

fn coerce_value(s: &str, type_hint: &str) -> Result<Value, ()> {
    match type_hint {
        "number" => s
            .parse::<i64>()
            .map(Value::from)
            .or_else(|_| s.parse::<f64>().map(Value::from))
            .map_err(|_| ()),
        "bool" | "boolean" => match s {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(()),
        },
        _ => Ok(Value::String(s.to_string())),
    }
}

fn build_positional_params(args: &[&str]) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    for (i, arg) in args.iter().enumerate() {
        let value = if let Ok(n) = arg.parse::<i64>() {
            Value::from(n)
        } else if *arg == "true" || *arg == "false" {
            Value::Bool(*arg == "true")
        } else {
            Value::String(arg.to_string())
        };
        map.insert(format!("arg{i}"), value);
    }
    map
}

pub fn help_text() -> String {
    "\
Commands:
  help                          Show this help
  list_rigs                     List available rigs and connection state
  caps <rig_id>                 Show rig capabilities (commands & status fields)
  <command> <rig_id> [params..] Execute a command (e.g. 'freq 0 14250000')"
        .to_string()
}
