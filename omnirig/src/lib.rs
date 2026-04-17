use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex, Weak};
use std::thread::JoinHandle;
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoInitializeEx, CoRegisterClassObject,
    CoRevokeClassObject, CoUninitialize, IClassFactory, REGCLS_MULTIPLEUSE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
};
use windows::core::GUID;

use tracing::trace;

use crate::connection_point::EventSinks;
use crate::omnirig::OmniRigXFactory;

pub mod connection_point;
mod enums;
pub mod omnirig;
mod port_bits;
pub mod provider;
mod registry;
pub mod rig;
pub mod typelib;

pub use enums::{RigParamX, RigStatusX};
pub use provider::{
    DummyPortBits, DummyProvider, DummyRig, OmniRigProvider, PortBitsControl, RigControl,
};

// ---------------------------------------------------------------------------
// EventDispatcher — marshals event-firing onto the COM thread
// ---------------------------------------------------------------------------

enum ComEvent {
    RegisterSinks(Arc<EventSinks>),
    ParamsChange { rig_number: i32, params: i32 },
    StatusChange { rig_number: i32 },
}

/// Thread-safe handle for dispatching OmniRig events.
///
/// Events are queued and fired on the COM thread, ensuring that
/// `IDispatch::Invoke` on client event sinks happens in the correct
/// COM apartment.
#[derive(Clone)]
pub struct EventDispatcher {
    queue: Arc<Mutex<Vec<ComEvent>>>,
}

impl EventDispatcher {
    fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn fire_params_change(&self, rig_number: i32, params: i32) {
        trace!(
            rig_number,
            params = format_args!("0x{params:08X}"),
            "Queuing ParamsChange event"
        );
        self.queue
            .lock()
            .unwrap()
            .push(ComEvent::ParamsChange { rig_number, params });
    }

    pub fn fire_status_change(&self, rig_number: i32) {
        trace!(rig_number, "Queuing StatusChange event");
        self.queue
            .lock()
            .unwrap()
            .push(ComEvent::StatusChange { rig_number });
    }

    pub(crate) fn register_sinks(&self, sinks: Arc<EventSinks>) {
        trace!("Queuing new event sinks registration");
        self.queue
            .lock()
            .unwrap()
            .push(ComEvent::RegisterSinks(sinks));
    }

    fn drain(&self) -> Vec<ComEvent> {
        std::mem::take(&mut *self.queue.lock().unwrap())
    }
}

fn process_events(dispatcher: &EventDispatcher, sink_list: &mut Vec<Weak<EventSinks>>) {
    let before = sink_list.len();
    sink_list.retain(|w| w.strong_count() > 0);
    let pruned = before - sink_list.len();
    if pruned > 0 {
        trace!(
            pruned,
            active_sinks = sink_list.len(),
            "Pruned dead event sink entries"
        );
    }

    for weak in sink_list.iter() {
        if let Some(s) = weak.upgrade() {
            s.unmarshal_pending();
        }
    }

    for event in dispatcher.drain() {
        match event {
            ComEvent::RegisterSinks(sinks) => {
                sinks.unmarshal_pending();
                sink_list.push(Arc::downgrade(&sinks));
                trace!(
                    active_sinks = sink_list.len(),
                    "Registered new event sinks on COM thread"
                );
            }
            ComEvent::ParamsChange { rig_number, params } => {
                for weak in sink_list.iter() {
                    if let Some(s) = weak.upgrade() {
                        s.fire_params_change(rig_number, params);
                    }
                }
            }
            ComEvent::StatusChange { rig_number } => {
                for weak in sink_list.iter() {
                    if let Some(s) = weak.upgrade() {
                        s.fire_status_change(rig_number);
                    }
                }
            }
        }
    }
}

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
    // Initialize COM as STA *before* any COM API calls (LoadTypeLibEx, RegisterTypeLibForUser)
    // to avoid implicit MTA initialization that would prevent STA setup.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    tracing::info!(thread_id = ?std::thread::current().id(), "COM STA initialized");

    let exe_path = std::env::current_exe()?;
    let exe_path_str = exe_path.to_str().ok_or("Invalid executable path")?;

    registry::register_com_component(&CLSID_OMNIRIG, exe_path_str, PROG_ID, "1.0")?;

    let tlb_path = exe_path.with_file_name("OmniRig.tlb");
    let tlb_path_str = tlb_path.to_str().ok_or("Invalid TLB path")?;
    registry::register_type_library(&CLSID_OMNIRIG, tlb_path_str)?;
    typelib::init(tlb_path_str)?;

    let dispatcher = EventDispatcher::new();
    provider.set_event_dispatcher(dispatcher.clone());

    let provider = Arc::new(provider);

    unsafe {
        let factory: IClassFactory = OmniRigXFactory::new(provider, dispatcher.clone()).into();

        let cookie = CoRegisterClassObject(
            &CLSID_OMNIRIG,
            &factory,
            CLSCTX_LOCAL_SERVER,
            REGCLS_MULTIPLEUSE,
        )?;

        // Init complete — unblock caller
        barrier.wait();

        // Message loop — runs until shutdown.
        // Events queued via EventDispatcher are drained and fired here,
        // ensuring IDispatch::Invoke runs on this COM-initialized thread.
        let mut msg = MSG::default();
        let mut sink_list: Vec<Weak<EventSinks>> = Vec::new();
        while !shutdown_flag.load(Ordering::SeqCst) {
            process_events(&dispatcher, &mut sink_list);

            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        let _ = CoRevokeClassObject(cookie);
        let _ = registry::unregister_com_component(&CLSID_OMNIRIG);
        let _ = registry::unregister_type_library();
        CoUninitialize();
    }

    Ok(())
}
