use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

use crate::gui::{GuiMessage, SerialPortEntry};

#[derive(Error, Debug)]
pub enum PortEnumeratorError {
    #[error("Failed to enumerate serial ports: {0}")]
    Enumeration(#[from] serialport::Error),
    #[error("Failed to send ports to GUI: {0}")]
    Send(#[from] mpsc::error::SendError<GuiMessage>),
}

pub async fn run(gui_sender: mpsc::Sender<GuiMessage>) -> Result<(), PortEnumeratorError> {
    loop {
        let ports = serialport::available_ports()?
            .into_iter()
            .filter_map(|p| match &p.port_type {
                serialport::SerialPortType::UsbPort(usb) => {
                    let display_name = if let Some(product) = &usb.product {
                        format!("{} ({})", p.port_name, product)
                    } else {
                        p.port_name.clone()
                    };
                    Some(SerialPortEntry {
                        port_name: p.port_name,
                        display_name,
                    })
                }
                _ => None,
            })
            .collect();

        gui_sender.send(GuiMessage::AvailablePorts(ports)).await?;

        sleep(Duration::from_secs(2)).await;
    }
}
