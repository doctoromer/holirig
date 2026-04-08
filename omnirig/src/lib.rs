use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread::JoinHandle;
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, CoInitializeEx, CoRegisterClassObject,
    CoRevokeClassObject, CoUninitialize, IClassFactory, REGCLS_MULTIPLEUSE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
};
use windows::core::GUID;

use crate::omnirig::OmniRigXFactory;

pub mod connection_point;
mod enums;
pub mod omnirig;
mod port_bits;
pub mod provider;
mod registry;
pub mod rig;

pub use connection_point::EventSinks;
pub use enums::{RigParamX, RigStatusX};
pub use provider::{
    DummyPortBits, DummyProvider, DummyRig, OmniRigProvider, PortBitsControl, RigControl,
};

pub const CLSID_OMNIRIG: GUID = GUID::from_u128(0x0839E8C6_ED30_4950_8087_966F970F0CAE);
pub const PROG_ID: &str = "OmniRig.OmniRigX";

pub struct OmniRigHandle {
    shutdown_flag: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl OmniRigHandle {
    // Used as external API that consumes the handle
    pub fn shutdown(mut self) {
        self.stop_and_join();
    }

    // Internal function that is used in shutdown and in drop implementation
    fn stop_and_join(&mut self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            handle.join().ok();
        }
    }
}

impl Drop for OmniRigHandle {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// Blocks until COM registration is complete and the server is ready to accept clients.
/// Returns a shutdown handle, or an error if COM initialization failed.
pub fn spawn_omnirig_server(
    provider: impl OmniRigProvider + 'static,
) -> Result<OmniRigHandle, Box<dyn std::error::Error>> {
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_clone = shutdown_flag.clone();

    let barrier = Arc::new(Barrier::new(2));
    let barrier_clone = barrier.clone();

    let init_result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let init_result_clone = init_result.clone();

    let thread = std::thread::spawn(move || {
        match com_thread_init_and_run(provider, &shutdown_flag_clone, &barrier_clone) {
            Ok(()) => {}
            Err(e) => {
                let mut slot = init_result_clone.lock().unwrap();
                *slot = Some(e.to_string());
                barrier_clone.wait();
            }
        }
    });

    barrier.wait();

    let slot = init_result.lock().unwrap();
    if let Some(ref e) = *slot {
        thread.join().ok();
        return Err(e.clone().into());
    }

    Ok(OmniRigHandle {
        shutdown_flag,
        thread: Some(thread),
    })
}

fn com_thread_init_and_run(
    provider: impl OmniRigProvider,
    shutdown_flag: &AtomicBool,
    barrier: &Barrier,
) -> Result<(), Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_path_str = exe_path.to_str().ok_or("Invalid executable path")?;

    registry::register_com_component(&CLSID_OMNIRIG, exe_path_str, PROG_ID, "1.0")?;

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

        // Init complete — unblock caller
        barrier.wait();

        // Message loop — runs until shutdown
        let mut msg = MSG::default();
        while !shutdown_flag.load(Ordering::SeqCst) {
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }

        let _ = CoRevokeClassObject(cookie);
        let _ = registry::unregister_com_component(&CLSID_OMNIRIG);
        CoUninitialize();
    }

    Ok(())
}
