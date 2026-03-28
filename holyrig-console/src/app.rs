use std::collections::HashMap;

use serde_json::Value;

pub struct RigState {
    pub rig_id: usize,
    pub connected: bool,
    pub status: HashMap<String, Value>,
    pub capabilities: Option<Capabilities>,
}

pub struct Capabilities {
    pub commands: HashMap<String, Vec<CommandParam>>,
    pub status_fields: HashMap<String, String>,
}

pub struct CommandParam {
    pub name: String,
    pub param_type: String,
}

pub struct ReplEntry {
    pub kind: EntryKind,
    pub text: String,
}

pub enum EntryKind {
    Command,
    Response,
    Error,
}

pub struct App {
    pub rigs: Vec<RigState>,
    pub repl_history: Vec<ReplEntry>,
    pub input: String,
    pub cursor_pos: usize,
    pub command_history: Vec<String>,
    pub command_history_index: Option<usize>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            rigs: Vec::new(),
            repl_history: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            command_history: Vec::new(),
            command_history_index: None,
            should_quit: false,
        }
    }

    pub fn add_rig(&mut self, rig_id: usize, connected: bool) {
        self.rigs.push(RigState {
            rig_id,
            connected,
            status: HashMap::new(),
            capabilities: None,
        });
    }

    pub fn set_capabilities(&mut self, rig_id: usize, caps: Capabilities) {
        if let Some(rig) = self.rigs.iter_mut().find(|r| r.rig_id == rig_id) {
            rig.capabilities = Some(caps);
        }
    }

    pub fn update_status(&mut self, rig_id: usize, updates: HashMap<String, Value>) {
        if let Some(rig) = self.rigs.iter_mut().find(|r| r.rig_id == rig_id) {
            for (k, v) in updates {
                rig.status.insert(k, v);
            }
        }
    }

    pub fn push_command(&mut self, cmd: &str) {
        self.repl_history.push(ReplEntry {
            kind: EntryKind::Command,
            text: cmd.to_string(),
        });
        self.command_history.push(cmd.to_string());
        self.command_history_index = None;
    }

    pub fn push_response(&mut self, text: String) {
        self.repl_history.push(ReplEntry {
            kind: EntryKind::Response,
            text,
        });
    }

    pub fn push_error(&mut self, text: String) {
        self.repl_history.push(ReplEntry {
            kind: EntryKind::Error,
            text,
        });
    }

    pub fn history_up(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        let idx = match self.command_history_index {
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
            None => self.command_history.len() - 1,
        };
        self.command_history_index = Some(idx);
        self.input = self.command_history[idx].clone();
        self.cursor_pos = self.input.len();
    }

    pub fn history_down(&mut self) {
        match self.command_history_index {
            Some(i) if i + 1 < self.command_history.len() => {
                let idx = i + 1;
                self.command_history_index = Some(idx);
                self.input = self.command_history[idx].clone();
                self.cursor_pos = self.input.len();
            }
            Some(_) => {
                self.command_history_index = None;
                self.input.clear();
                self.cursor_pos = 0;
            }
            None => {}
        }
    }
}
