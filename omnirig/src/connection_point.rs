#![allow(non_snake_case)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::E_NOTIMPL;
use windows::Win32::System::Com::{
    DISPATCH_FLAGS, DISPPARAMS, IConnectionPoint, IConnectionPoint_Impl, IConnectionPointContainer,
    IDispatch, IEnumConnections,
};
use windows::Win32::System::Ole::CONNECT_E_NOCONNECTION;
use windows::Win32::System::Variant::VARIANT;
use windows::core::{GUID, IUnknown, Interface, Ref, implement};

pub const OMNIRIG_EVENTS_IID: GUID = GUID::from_u128(0x2219175F_E561_47E7_AD17_73C4D8891AA1);

const DISPATCH_METHOD: DISPATCH_FLAGS = DISPATCH_FLAGS(1);

/// Shared list of event sinks registered by COM clients via IConnectionPoint::Advise.
/// Held as Arc by OmniRigX (for IConnectionPointContainer) and as Weak by the provider
/// (for firing events when rig state changes).
pub struct EventSinks {
    sinks: Mutex<HashMap<u32, IDispatch>>,
    next_cookie: AtomicU32,
}

// Safety: IDispatch is Send + Sync in windows-rs; COM marshals cross-apartment calls transparently.
unsafe impl Send for EventSinks {}
unsafe impl Sync for EventSinks {}

impl EventSinks {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sinks: Mutex::new(HashMap::new()),
            next_cookie: AtomicU32::new(1),
        })
    }

    fn fire_one_arg(&self, dispid: i32, rig_number: i32) {
        let sinks = self.sinks.lock().unwrap();
        if sinks.is_empty() {
            return;
        }
        let mut arg: VARIANT = rig_number.into();
        let dispparams = DISPPARAMS {
            rgvarg: &mut arg,
            rgdispidNamedArgs: std::ptr::null_mut(),
            cArgs: 1,
            cNamedArgs: 0,
        };
        for sink in sinks.values() {
            unsafe {
                let _ = sink.Invoke(
                    dispid,
                    &GUID::zeroed(),
                    0,
                    DISPATCH_METHOD,
                    &dispparams,
                    None,
                    None,
                    None,
                );
            }
        }
    }

    /// Fire `StatusChange(RigNumber)` — DISPID 0x03
    pub fn fire_status_change(&self, rig_number: i32) {
        self.fire_one_arg(0x03, rig_number);
    }

    /// Fire `ParamsChange(RigNumber, Params)` — DISPID 0x04
    pub fn fire_params_change(&self, rig_number: i32, params: i32) {
        let sinks = self.sinks.lock().unwrap();
        if sinks.is_empty() {
            return;
        }
        // DISPPARAMS args are in reverse order: last param first
        let mut args: [VARIANT; 2] = [params.into(), rig_number.into()];
        let dispparams = DISPPARAMS {
            rgvarg: args.as_mut_ptr(),
            rgdispidNamedArgs: std::ptr::null_mut(),
            cArgs: 2,
            cNamedArgs: 0,
        };
        for sink in sinks.values() {
            unsafe {
                let _ = sink.Invoke(
                    0x04,
                    &GUID::zeroed(),
                    0,
                    DISPATCH_METHOD,
                    &dispparams,
                    None,
                    None,
                    None,
                );
            }
        }
    }

    /// Fire `VisibleChange()` — DISPID 0x01
    pub fn fire_visible_change(&self) {
        let sinks = self.sinks.lock().unwrap();
        if sinks.is_empty() {
            return;
        }
        let dispparams = DISPPARAMS {
            rgvarg: std::ptr::null_mut(),
            rgdispidNamedArgs: std::ptr::null_mut(),
            cArgs: 0,
            cNamedArgs: 0,
        };
        for sink in sinks.values() {
            unsafe {
                let _ = sink.Invoke(
                    0x01,
                    &GUID::zeroed(),
                    0,
                    DISPATCH_METHOD,
                    &dispparams,
                    None,
                    None,
                    None,
                );
            }
        }
    }
}

/// COM object implementing IConnectionPoint for the IOmniRigXEvents dispinterface.
#[implement(IConnectionPoint)]
pub struct OmniRigEventsConnectionPoint {
    pub sinks: Arc<EventSinks>,
}

impl IConnectionPoint_Impl for OmniRigEventsConnectionPoint_Impl {
    fn GetConnectionInterface(&self) -> windows::core::Result<GUID> {
        Ok(OMNIRIG_EVENTS_IID)
    }

    fn GetConnectionPointContainer(&self) -> windows::core::Result<IConnectionPointContainer> {
        Err(E_NOTIMPL.into())
    }

    fn Advise(&self, punk_sink: Ref<IUnknown>) -> windows::core::Result<u32> {
        let unk = punk_sink.cloned().ok_or_else(|| {
            windows::core::Error::from_hresult(windows::Win32::Foundation::E_POINTER)
        })?;
        let disp: IDispatch = unk.cast()?;
        let cookie = self.sinks.next_cookie.fetch_add(1, Ordering::Relaxed);
        self.sinks.sinks.lock().unwrap().insert(cookie, disp);
        Ok(cookie)
    }

    fn Unadvise(&self, dw_cookie: u32) -> windows::core::Result<()> {
        self.sinks.sinks.lock().unwrap().remove(&dw_cookie);
        Ok(())
    }

    fn EnumConnections(&self) -> windows::core::Result<IEnumConnections> {
        Err(E_NOTIMPL.into())
    }
}

/// Returns `CONNECT_E_NOCONNECTION` as a windows error for use in FindConnectionPoint.
pub fn connect_e_noconnection() -> windows::core::Error {
    windows::core::Error::from_hresult(CONNECT_E_NOCONNECTION)
}
