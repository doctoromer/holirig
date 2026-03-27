use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handle = omnirig::spawn_omnirig_server(omnirig::DummyProvider)?;

    println!("OmniRig COM server started. Press Ctrl+C to stop...");

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        running_clone.store(false, Ordering::SeqCst);
    })?;

    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    handle.shutdown();
    println!("Server stopped.");

    Ok(())
}
