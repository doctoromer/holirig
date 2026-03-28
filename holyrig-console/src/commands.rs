use std::collections::HashMap;

use serde_json::Value;

use crate::app::{App, Capabilities};

pub enum Command {
    ListRigs,
    Caps {
        rig_id: usize,
    },
    Status {
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
    MissingRigId,
    InvalidRigId(String),
    UnknownRigCommand {
        command: String,
        rig_id: usize,
    },
    WrongParamCount {
        expected: usize,
        got: usize,
    },
    InvalidParamValue {
        param: String,
        expected_type: String,
        got: String,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnknownCommand(cmd) => write!(
                f,
                "Unknown command: '{cmd}'. Type 'help' for available commands."
            ),
            ParseError::MissingRigId => write!(f, "Missing rig_id argument"),
            ParseError::InvalidRigId(s) => write!(f, "Invalid rig_id: '{s}'"),
            ParseError::UnknownRigCommand { command, rig_id } => {
                write!(
                    f,
                    "Unknown command '{command}' for rig {rig_id}. Use 'caps {rig_id}' to see available commands."
                )
            }
            ParseError::WrongParamCount { expected, got } => {
                write!(f, "Expected {expected} parameter(s), got {got}")
            }
            ParseError::InvalidParamValue {
                param,
                expected_type,
                got,
            } => {
                write!(
                    f,
                    "Invalid value for '{param}' (expected {expected_type}): '{got}'"
                )
            }
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
            let rig_id = parse_rig_id(parts.get(1))?;
            Ok(Command::Caps { rig_id })
        }
        "status" => {
            let rig_id = parse_rig_id(parts.get(1))?;
            Ok(Command::Status { rig_id })
        }
        cmd => {
            let rig_id = parse_rig_id(parts.get(1))?;
            let set_cmd = format!("set_{cmd}");
            let rig = app.rigs.iter().find(|r| r.rig_id == rig_id);
            let caps = rig.and_then(|r| r.capabilities.as_ref());

            match caps {
                Some(caps) => resolve_execute(caps, rig_id, &set_cmd, &parts[2..]),
                None => Ok(Command::Execute {
                    rig_id,
                    command: set_cmd,
                    parameters: build_positional_params(&parts[2..]),
                }),
            }
        }
    }
}

fn parse_rig_id(arg: Option<&&str>) -> Result<usize, ParseError> {
    let s = arg.ok_or(ParseError::MissingRigId)?;
    s.parse()
        .map_err(|_| ParseError::InvalidRigId(s.to_string()))
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
        return Err(ParseError::WrongParamCount {
            expected: param_defs.len(),
            got: value_args.len(),
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
  status <rig_id>               Show current cached status for a rig
  <command> <rig_id> [params..] Execute a command (e.g. 'freq 0 14250000')"
        .to_string()
}
