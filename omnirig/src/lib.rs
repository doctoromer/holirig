use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use windows::core::GUID;
use windows::Win32::System::Com::{
    CoInitializeEx, CoRegisterClassObject, CoRevokeClassObject, CoUninitialize, IClassFactory,
    CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, REGCLS_MULTIPLEUSE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

use crate::omnirig::OmniRigXFactory;

mod enums;
mod omnirig;
mod port_bits;
pub mod provider;
mod registry;
mod rig;

pub use enums::{RigParamX, RigStatusX};
pub use provider::{
    DummyPortBits, DummyProvider, DummyRig, OmniRigProvider, PortBitsControl, RigControl,
};

const CLSID_OMNIRIG: GUID = GUID::from_u128(0x0839E8C6_ED30_4950_8087_966F970F0CAE);

pub fn run_omnirig_server(
    provider: impl OmniRigProvider,
) -> Result<(), Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_path_str = exe_path.to_str().ok_or("Invalid executable path")?;

    println!("\nRegistering HolyRig COM component...");
    registry::register_com_component(&CLSID_OMNIRIG, exe_path_str, "OmniRig.OmniRigX", "1.0")?;

    let provider = Arc::new(provider);

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;

        let factory: IClassFactory = OmniRigXFactory::new(provider).into();

        let cookie = CoRegisterClassObject(
            &CLSID_OMNIRIG,
            &factory,
            CLSCTX_LOCAL_SERVER,
            REGCLS_MULTIPLEUSE,
        )?;

        println!("\nOmniRig COM server started successfully!");
        println!("Press Ctrl+C to stop the server...");

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        ctrlc::set_handler(move || {
            println!("\nReceived Ctrl+C, shutting down...");
            running_clone.store(false, Ordering::SeqCst);
        })
        .expect("Error setting Ctrl-C handler");

        let mut msg = MSG::default();
        while running.load(Ordering::SeqCst) {
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }

        println!("\nRevoking class object...");
        CoRevokeClassObject(cookie)?;
        CoUninitialize();

        println!("Server stopped.");
    }

    Ok(())
}
