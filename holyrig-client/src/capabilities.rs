use std::collections::HashMap;

use serde_json::Value;

#[derive(Clone, Default)]
pub struct Capabilities {
    pub commands: HashMap<String, Vec<CommandParam>>,
    pub status_fields: HashMap<String, String>,
}

#[derive(Clone)]
pub struct CommandParam {
    pub name: String,
    pub param_type: String,
}

pub fn parse_capabilities(value: &Value) -> Capabilities {
    let mut commands = HashMap::new();
    if let Some(cmds) = value.get("commands").and_then(|v| v.as_object()) {
        for (cmd_name, cmd_info) in cmds {
            let mut params = Vec::new();
            if let Some(parameters) = cmd_info.get("parameters").and_then(|v| v.as_object()) {
                for (param_name, param_type) in parameters {
                    params.push(CommandParam {
                        name: param_name.clone(),
                        param_type: param_type.as_str().unwrap_or("string").to_string(),
                    });
                }
            }
            commands.insert(cmd_name.clone(), params);
        }
    }

    let mut status_fields = HashMap::new();
    if let Some(fields) = value.get("status_fields").and_then(|v| v.as_object()) {
        for (name, type_val) in fields {
            status_fields.insert(
                name.clone(),
                type_val.as_str().unwrap_or("string").to_string(),
            );
        }
    }

    Capabilities {
        commands,
        status_fields,
    }
}
