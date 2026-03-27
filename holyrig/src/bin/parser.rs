use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use argh::FromArgs;
use tracing::{error, info};

use holyrig::runtime::{parse_and_validate_with_schema, parse_rig_file, parse_schema};

#[derive(FromArgs)]
/// Command line tool for validating rig files and schema files
struct Args {
    #[argh(option)]
    /// rig file to validate
    rig: Option<PathBuf>,
    #[argh(option)]
    /// schema file to validate
    schema: Option<PathBuf>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args: Args = argh::from_env();

    let rig = if let Some(rig) = args.rig {
        Some(std::fs::read_to_string(rig)?)
    } else {
        None
    };

    let schema = if let Some(schema) = args.schema {
        Some(std::fs::read_to_string(schema)?)
    } else {
        None
    };

    match (rig, schema) {
        (Some(rig), Some(schema)) => {
            let schema = parse_schema(&schema)?;
            let schemas = HashMap::from([(schema.name.clone(), schema)]);
            match parse_and_validate_with_schema(&rig, &schemas) {
                Ok(rig_file) => {
                    info!(
                        schema = %rig_file.impl_block.schema,
                        name = %rig_file.impl_block.name,
                        "Successfully parsed schema and rig"
                    );
                }
                Err(errors) => {
                    for err in errors {
                        error!(%err, "Parse error");
                    }
                }
            }
        }
        (None, Some(schema)) => match parse_schema(&schema) {
            Ok(schema) => {
                info!(name = %schema.name, "Successfully parsed schema");
            }
            Err(err) => {
                error!(%err, "Failed to parse schema");
            }
        },
        (Some(rig), None) => match parse_rig_file(&rig) {
            Ok(rig) => {
                info!(
                    schema = %rig.impl_block.schema,
                    name = %rig.impl_block.name,
                    "Successfully parsed rig"
                );
            }
            Err(err) => {
                error!(%err, "Failed to parse rig");
            }
        },
        (None, None) => {
            error!("You must provide rig file, schema or both");
        }
    }

    Ok(())
}
