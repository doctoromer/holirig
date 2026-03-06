use anyhow::Result;
use eframe::egui;
use holyrig::interfaces::jsonrpc::JsonRpcServer;
use holyrig::resources::{ResourceError, Resources};
use holyrig::serial::port_enumerator::PortEnumeratorError;
use tokio::sync::mpsc;

use holyrig::interfaces::{rigctld, udp_server};
use holyrig::{gui, serial};

use gui::GuiMessage;
use serial::manager::DeviceManager;

#[tokio::main]
async fn main() -> Result<()> {
    let resources = match Resources::load() {
        Ok(resources) => resources,
        Err(err) => {
            match err {
                ResourceError::Schema(parse_error) => {
                    eprintln!("{parse_error}");
                }
                ResourceError::Rig(parse_errors) => {
                    for err in parse_errors {
                        eprintln!("{err}");
                    }
                }
                err => {
                    eprintln!("{err}")
                }
            }
            return Ok(());
        }
    };

    let (gui_sender, gui_receiver) = mpsc::channel::<GuiMessage>(10);
    let mut device_manager: DeviceManager = DeviceManager::new(resources.clone());

    let gui_command_sender = device_manager.sender();
    let udp_command_sender = device_manager.sender();
    let rigctld_command_sender = device_manager.sender();
    let udp_message_receiver = device_manager.receiver();
    let rigctld_message_receiver = device_manager.receiver();

    #[cfg(windows)]
    let _omnirig_handle = {
        use holyrig::interfaces::omnirig_provider::HolyRigProvider;
        let provider = HolyRigProvider::new(
            device_manager.sender(),
            device_manager.receiver(),
            tokio::runtime::Handle::current(),
        );
        println!("Starting OmniRig server");
        omnirig::spawn_omnirig_server(provider).expect("Failed to start OmniRig COM server")
    };

    let jsonrpc_command_sender = device_manager.sender();
    let jsonrpc_command_receiver = device_manager.receiver();
    let jsonrpc_server = JsonRpcServer::new(
        "127.0.0.1",
        5973,
        resources.clone(),
        jsonrpc_command_sender,
        jsonrpc_command_receiver,
    )?;

    tokio::spawn(async move { jsonrpc_server.run().await });

    let device_gui_sender = gui_sender.clone();
    tokio::spawn(async move {
        let result = device_manager.run(device_gui_sender).await;
        println!("Manager exited with: {result:?}");
    });

    let udp_resources = resources.clone();
    tokio::spawn(async move {
        if let Err(err) =
            udp_server::run_server(udp_resources, udp_command_sender, udp_message_receiver).await
        {
            eprintln!("UDP server error: {err}");
        }
    });

    tokio::spawn(async move {
        if let Err(err) =
            rigctld::run_server(rigctld_command_sender, rigctld_message_receiver).await
        {
            eprintln!("Rigctld server error: {err}");
        }
    });

    let port_gui_sender = gui_sender.clone();
    tokio::spawn(async move {
        if let Err(err) = serial::port_enumerator::run(port_gui_sender).await {
            match err {
                PortEnumeratorError::Enumeration(err) => {
                    eprintln!("{}", err.description);
                }
                PortEnumeratorError::Send(_) => {
                    eprintln!("Failed to send ports to GUI task");
                }
            }
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([450.0, 440.0])
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "Holyrig",
        options,
        Box::new(|_| {
            Ok(Box::new(gui::App::new(
                gui_receiver,
                gui_command_sender,
                resources.rigs.keys().cloned().collect(),
            )))
        }),
    )
    .unwrap();

    Ok(())
}
