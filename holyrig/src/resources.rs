use anyhow::Result;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::runtime::{Interpreter, SchemaFile, parse_and_validate_with_schema, parse_schema};

pub struct Resources {
    pub schemas: HashMap<String, SchemaFile>,
    pub rigs: HashMap<String, Interpreter>,
}

impl Resources {
    pub fn load() -> Result<Arc<Self>> {
        let schemas = Self::load_schemas()?;
        let rigs = Self::load_rig_files(&schemas)?;
        Ok(Arc::new(Self { schemas, rigs }))
    }

    fn load_resources<T, C, F: Fn(PathBuf, &C) -> Result<(String, T)>>(
        extension: &[u8],
        dir: &str,
        context: C,
        load_fn: F,
    ) -> Result<HashMap<String, T>> {
        let base_dir = if cfg!(debug_assertions) {
            let mut candidate = std::env::current_dir()?;
            loop {
                if candidate.join(dir).exists() {
                    break candidate;
                }
                if !candidate.pop() {
                    anyhow::bail!(
                        "Could not find '{}' in any ancestor of the current directory",
                        dir
                    );
                }
            }
        } else {
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?
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

    fn load_schemas() -> Result<HashMap<String, SchemaFile>> {
        Self::load_resources(b"schema", "schema", (), |path, _| {
            let schema = parse_schema(&std::fs::read_to_string(path)?)?;
            Ok((schema.name.clone(), schema))
        })
    }

    fn load_rig_files(
        schemas: &HashMap<String, SchemaFile>,
    ) -> Result<HashMap<String, Interpreter>> {
        Self::load_resources(b"rig", "rigs", schemas, |path, schemas| {
            let source = std::fs::read_to_string(path)?;
            // TODO: remove unwrap
            let rig_file = parse_and_validate_with_schema(&source, schemas).unwrap();
            Ok((rig_file.impl_block.name.clone(), Interpreter::new(rig_file)))
        })
    }
}
