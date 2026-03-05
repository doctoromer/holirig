use std::{collections::HashMap, path::PathBuf, sync::Arc};
use thiserror::Error;

use crate::runtime::parser_errors::ParseError;
use crate::runtime::{Interpreter, SchemaFile, parse_and_validate_with_schema, parse_schema};

#[derive(Error, Debug)]
pub enum ResourceError {
    #[error("Could not find '{0}' in any ancestor of the current directory")]
    DirNotFound(String),
    #[error("Could not find config directory")]
    ConfigDirNotFound,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Schema(#[from] ParseError),
    #[error("Rig parse errors:\n{}", .0.iter().map(|e: &ParseError| e.to_string()).collect::<Vec<_>>().join("\n"))]
    Rig(Vec<ParseError>),
}

pub struct Resources {
    pub schemas: HashMap<String, SchemaFile>,
    pub rigs: HashMap<String, Interpreter>,
}

impl Resources {
    pub fn load() -> Result<Arc<Self>, ResourceError> {
        let schemas = Self::load_schemas()?;
        let rigs = Self::load_rig_files(&schemas)?;
        Ok(Arc::new(Self { schemas, rigs }))
    }

    fn load_resources<T, C, F: Fn(PathBuf, &C) -> Result<(String, T), ResourceError>>(
        extension: &[u8],
        dir: &str,
        context: C,
        load_fn: F,
    ) -> Result<HashMap<String, T>, ResourceError> {
        let base_dir = if cfg!(debug_assertions) {
            let mut candidate = std::env::current_dir()?;
            loop {
                if candidate.join(dir).exists() {
                    break candidate;
                }
                if !candidate.pop() {
                    return Err(ResourceError::DirNotFound(dir.to_string()));
                }
            }
        } else {
            dirs::config_dir().ok_or(ResourceError::ConfigDirNotFound)?
        };

        base_dir
            .join(dir)
            .read_dir()?
            .filter_map(|entry| {
                if entry.is_err() {
                    return None;
                }
                let path = entry.unwrap().path();
                let is_extension_matching = path
                    .extension()
                    .map(|ext| ext.as_encoded_bytes() == extension)
                    .unwrap_or(false);
                if path.is_file() && is_extension_matching {
                    Some(path)
                } else {
                    None
                }
            })
            .map(|path| load_fn(path, &context))
            .collect()
    }

    fn load_schemas() -> Result<HashMap<String, SchemaFile>, ResourceError> {
        Self::load_resources(b"schema", "schema", (), |path, _| {
            let schema = parse_schema(&std::fs::read_to_string(path)?)?;
            Ok((schema.name.clone(), schema))
        })
    }

    fn load_rig_files(
        schemas: &HashMap<String, SchemaFile>,
    ) -> Result<HashMap<String, Interpreter>, ResourceError> {
        Self::load_resources(b"rig", "rigs", schemas, |path, schemas| {
            let source = std::fs::read_to_string(path)?;
            let rig_file =
                parse_and_validate_with_schema(&source, schemas).map_err(ResourceError::Rig)?;
            Ok((rig_file.impl_block.name.clone(), Interpreter::new(rig_file)))
        })
    }
}
