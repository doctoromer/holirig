use std::path::PathBuf;

use anyhow::Result;
use argh::FromArgs;
use eframe::egui;
use holyrig::interfaces::jsonrpc::JsonRpcServer;
use holyrig::resources::{ResourceError, Resources};
use tracing::{error, info};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, Registry, prelude::*};
use tray_icon::{
    TrayIconBuilder,
    menu::{Menu, MenuItem},
};

use holyrig::interfaces::rigctld;
use holyrig::{gui, serial};

use serial::manager::DeviceManager;

/// HolyRig - radio rig control
#[derive(FromArgs)]
struct Cli {
    /// verbose logging (-v debug, -vv trace)
    #[argh(switch, short = 'v')]
    verbose: u8,

    #[argh(subcommand)]
    command: Option<SubCommand>,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum SubCommand {
    Console(ConsoleCommand),
    Radio(RadioCommand),
}

/// Launch the interactive JSON-RPC console
#[derive(FromArgs)]
#[argh(subcommand, name = "console")]
struct ConsoleCommand {
    /// server address
    #[argh(option, default = "\"127.0.0.1:5973\".parse().unwrap()")]
    addr: std::net::SocketAddr,
}

/// Launch the radio front-panel GUI
#[derive(FromArgs)]
#[argh(subcommand, name = "radio")]
struct RadioCommand {
    /// server address
    #[argh(option, default = "\"127.0.0.1:5973\".parse().unwrap()")]
    addr: std::net::SocketAddr,
}

fn init_tracing(
    verbosity: u8,
) -> (
    tracing_appender::non_blocking::WorkerGuard,
    tracing_appender::non_blocking::WorkerGuard,
) {
    let log_dir = dirs::state_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("holyrig")
        .join("logs");

    let debug_appender = RollingFileAppender::builder()
        .max_log_files(100)
        .rotation(Rotation::DAILY)
        .filename_prefix("debug")
        .filename_suffix(".log")
        .build(&log_dir)
        .unwrap();
    let trace_appender = RollingFileAppender::builder()
        .max_log_files(100)
        .rotation(Rotation::DAILY)
        .filename_prefix("trace")
        .filename_suffix(".log")
        .build(&log_dir)
        .unwrap();
    let (debug_writer, debug_guard) = tracing_appender::non_blocking(debug_appender);
    let (trace_writer, trace_guard) = tracing_appender::non_blocking(trace_appender);

    let trace_filter = EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
        .from_env()
        .unwrap()
        .add_directive("holyrig=trace".parse().unwrap())
        .add_directive("omnirig=trace".parse().unwrap());

    let debug_filter = EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::DEBUG.into())
        .from_env()
        .unwrap()
        .add_directive("holyrig=info".parse().unwrap())
        .add_directive("omnirig=info".parse().unwrap());

    Registry::default()
        .with(
            tracing_subscriber::fmt::layer().with_filter(match verbosity {
                0 => EnvFilter::new("info"),
                1 => debug_filter.clone(),
                _ => trace_filter.clone(),
            }),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(debug_writer)
                .with_filter(debug_filter),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(trace_writer)
                .with_filter(trace_filter),
        )
        .init();

    std::panic::set_hook(Box::new(tracing_panic::panic_hook));
    (debug_guard, trace_guard)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli: Cli = argh::from_env();

    match cli.command {
        Some(SubCommand::Console(cmd)) => return holyrig_console::run(cmd.addr).await,
        Some(SubCommand::Radio(cmd)) => return holyrig_radio::run(cmd.addr).await,
        None => {}
    }

    let _guards = init_tracing(cli.verbose);

    let resources = match Resources::load() {
        Ok(resources) => resources,
        Err(err) => {
            match err {
                ResourceError::Schema(parse_error) => {
                    error!(%parse_error, "Failed to load schema");
                }
                ResourceError::Rig(parse_errors) => {
                    for err in parse_errors {
                        error!(%err, "Failed to load rig");
                    }
                }
                err => {
                    error!(%err, "Failed to load resources");
                }
            }
            return Ok(());
        }
    };

    let mut device_manager: DeviceManager = DeviceManager::new(resources.clone());

    let initial_rigs: Vec<_> = device_manager.initial_rigs().cloned().collect();
    let gui_command_sender = device_manager.sender();
    let gui_message_receiver = device_manager.receiver();
    let rigctld_command_sender = device_manager.sender();
    let rigctld_message_receiver = device_manager.receiver();

    #[cfg(windows)]
    let _omnirig_handle = {
        use holyrig::interfaces::omnirig_provider::HolyRigProvider;
        let provider = HolyRigProvider::new(
            device_manager.sender(),
            device_manager.receiver(),
            resources.clone(),
            tokio::runtime::Handle::current(),
            &initial_rigs,
        );
        info!("Starting OmniRig server");
        omnirig::spawn_omnirig_server(provider).expect("Failed to start OmniRig COM server")
    };

    let jsonrpc_command_sender = device_manager.sender();
    let jsonrpc_command_receiver = device_manager.receiver();
    let jsonrpc_status_cache = device_manager.status_cache();
    let jsonrpc_server = JsonRpcServer::new(
        "127.0.0.1",
        5973,
        resources.clone(),
        jsonrpc_command_sender,
        jsonrpc_command_receiver,
        jsonrpc_status_cache,
        &initial_rigs,
    )
    .await?;

    tokio::spawn(async move { jsonrpc_server.run().await });

    tokio::spawn(async move {
        let result = device_manager.run().await;
        info!(?result, "Manager exited");
    });

    tokio::spawn(async move {
        if let Err(err) =
            rigctld::run_server(rigctld_command_sender, rigctld_message_receiver).await
        {
            error!(%err, "Rigctld server error");
        }
    });

    let (tray_rx, ctx_tx) = gui::spawn_tray_watcher();

    #[cfg(target_os = "linux")]
    gtk::init().expect("Failed to initialize GTK");

    let _tray_icon = {
        let show_item = MenuItem::with_id("show", "Show", true, None);
        let quit_item = MenuItem::with_id("quit", "Quit", true, None);
        let menu = Menu::new();
        menu.append(&show_item).unwrap();
        menu.append(&quit_item).unwrap();
        let icon = {
            let size = 32u32;
            let rgba: Vec<u8> = (0..size * size)
                .flat_map(|_| [70u8, 130, 180, 255])
                .collect();
            tray_icon::Icon::from_rgba(rgba, size, size).expect("Failed to create tray icon")
        };
        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip("HolyRig")
            .build()
            .expect("Failed to build tray icon")
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 430.0])
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "Holyrig",
        options,
        Box::new(|_| {
            Ok(Box::new(gui::App::new(
                gui_message_receiver,
                gui_command_sender,
                resources.rigs.keys().cloned().collect(),
                initial_rigs,
                tray_rx,
                ctx_tx,
            )))
        }),
    )
    .unwrap();

    Ok(())
}
