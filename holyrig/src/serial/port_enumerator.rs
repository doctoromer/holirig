use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

use crate::gui::{GuiMessage, SerialPortEntry};

pub async fn run(gui_sender: mpsc::Sender<GuiMessage>) {
    loop {
        let ports = serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let display_name = match &p.port_type {
                    serialport::SerialPortType::UsbPort(usb) => {
                        if let Some(product) = &usb.product {
                            format!("{} ({})", p.port_name, product)
                        } else {
                            p.port_name.clone()
                        }
                    }
                    _ => p.port_name.clone(),
                };
                SerialPortEntry {
                    port_name: p.port_name,
                    display_name,
                }
            })
            .collect();

        if gui_sender.send(GuiMessage::AvailablePorts(ports)).await.is_err() {
            break;
        }

        sleep(Duration::from_secs(2)).await;
    }
}
