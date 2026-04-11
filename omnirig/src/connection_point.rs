#![allow(non_snake_case)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use tracing::trace;
use windows::Win32::Foundation::E_NOTIMPL;
use windows::Win32::Foundation::S_FALSE;
use windows::Win32::System::Com::Marshal::CoMarshalInterThreadInterfaceInStream;
use windows::Win32::System::Com::StructuredStorage::CoGetInterfaceAndReleaseStream;
use windows::Win32::System::Com::{
    DISPATCH_FLAGS, DISPPARAMS, IConnectionPoint, IConnectionPoint_Impl, IConnectionPointContainer,
    IDispatch, IEnumConnectionPoints, IEnumConnectionPoints_Impl, IEnumConnections, IStream,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{GUID, IUnknown, Interface, Ref, implement};
use windows_core::HRESULT;

pub const OMNIRIG_EVENTS_IID: GUID = GUID::from_u128(0x2219175F_E561_47E7_AD17_73C4D8891AA1);

const DISPATCH_METHOD: DISPATCH_FLAGS = DISPATCH_FLAGS(1);

pub struct MarshaledSink {
    pub cookie: u32,
    pub stream: IStream,
}

unsafe impl Send for MarshaledSink {}

/// Shared list of event sinks registered by COM clients via IConnectionPoint::Advise.
pub struct EventSinks {
    sinks: Mutex<HashMap<u32, IDispatch>>,
    next_cookie: AtomicU32,
    pending: Mutex<Vec<MarshaledSink>>,
}

unsafe impl Send for EventSinks {}
unsafe impl Sync for EventSinks {}

impl EventSinks {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sinks: Mutex::new(HashMap::new()),
            next_cookie: AtomicU32::new(1),
            pending: Mutex::new(Vec::new()),
        })
    }

    /// Unmarshal any pending sinks onto the current COM thread.
    pub fn unmarshal_pending(&self) {
        let pending: Vec<MarshaledSink> = std::mem::take(&mut *self.pending.lock().unwrap());
        for item in pending {
            let result: windows::core::Result<IDispatch> =
                unsafe { CoGetInterfaceAndReleaseStream(&item.stream) };
            match result {
                Ok(disp) => {
                    trace!(
                        cookie = item.cookie,
                        thread_id = ?std::thread::current().id(),
                        "Unmarshaled event sink on COM thread"
                    );
                    self.sinks.lock().unwrap().insert(item.cookie, disp);
                }
                Err(err) => {
                    tracing::warn!(cookie = item.cookie, %err, "Failed to unmarshal event sink");
                }
            }
        }
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
        for (cookie, sink) in sinks.iter() {
            let hresult = unsafe {
                sink.Invoke(
                    dispid,
                    &GUID::zeroed(),
                    0,
                    DISPATCH_METHOD,
                    &dispparams,
                    None,
                    None,
                    None,
                )
            };
            if let Err(err) = hresult {
                tracing::warn!(cookie, dispid, %err, "Event Invoke failed");
            }
        }
    }

    pub fn fire_status_change(&self, rig_number: i32) {
        trace!(rig_number, "Firing StatusChange event");
        self.fire_one_arg(0x03, rig_number);
    }

    pub fn fire_params_change(&self, rig_number: i32, params: i32) {
        trace!(
            rig_number,
            params = format_args!("0x{params:08X}"),
            thread_id = ?std::thread::current().id(),
            "Firing ParamsChange event"
        );
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
        for (cookie, sink) in sinks.iter() {
            let hr = unsafe {
                sink.Invoke(
                    0x04,
                    &GUID::zeroed(),
                    0,
                    DISPATCH_METHOD,
                    &dispparams,
                    None,
                    None,
                    None,
                )
            };
            if let Err(err) = hr {
                tracing::warn!(cookie, %err, "ParamsChange Invoke failed");
            }
        }
    }

    pub fn fire_visible_change(&self) {
        trace!("Firing VisibleChange event");
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

        // Marshal the IDispatch into a stream for later unmarshaling on the COM thread.
        // Advise may be called on an RPC worker thread, but we need to Invoke on the COM thread.
        let stream = unsafe { CoMarshalInterThreadInterfaceInStream(&IDispatch::IID, &disp)? };
        self.sinks
            .pending
            .lock()
            .unwrap()
            .push(MarshaledSink { cookie, stream });

        trace!(
            cookie,
            thread_id = ?std::thread::current().id(),
            "Client subscribed to events via Advise (marshaled for COM thread)"
        );
        Ok(cookie)
    }

    fn Unadvise(&self, dw_cookie: u32) -> windows::core::Result<()> {
        self.sinks
            .pending
            .lock()
            .unwrap()
            .retain(|s| s.cookie != dw_cookie);
        let removed = self
            .sinks
            .sinks
            .lock()
            .unwrap()
            .remove(&dw_cookie)
            .is_some();
        trace!(
            cookie = dw_cookie,
            removed, "Client unsubscribed via Unadvise"
        );
        Ok(())
    }

    fn EnumConnections(&self) -> windows::core::Result<IEnumConnections> {
        Err(E_NOTIMPL.into())
    }
}

#[implement(IEnumConnectionPoints)]
pub struct EnumConnectionPoints {
    points: Vec<IConnectionPoint>,
    index: Mutex<usize>,
}

impl EnumConnectionPoints {
    pub fn new(points: Vec<IConnectionPoint>) -> Self {
        Self {
            points,
            index: Mutex::new(0),
        }
    }
}

impl IEnumConnectionPoints_Impl for EnumConnectionPoints_Impl {
    fn Next(
        &self,
        cconnections: u32,
        ppcp: *mut Option<IConnectionPoint>,
        pcfetched: *mut u32,
    ) -> HRESULT {
        let mut idx = self.index.lock().unwrap();
        let remaining = self.points.len().saturating_sub(*idx);
        let to_copy = (cconnections as usize).min(remaining);

        for i in 0..to_copy {
            unsafe {
                *ppcp.add(i) = Some(self.points[*idx + i].clone());
            }
        }
        *idx += to_copy;

        if !pcfetched.is_null() {
            unsafe {
                *pcfetched = to_copy as u32;
            }
        }

        if to_copy == cconnections as usize {
            HRESULT(0) // S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, cconnections: u32) -> windows::core::Result<()> {
        let mut idx = self.index.lock().unwrap();
        let remaining = self.points.len().saturating_sub(*idx);
        let to_skip = (cconnections as usize).min(remaining);
        *idx += to_skip;

        if to_skip == cconnections as usize {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }

    fn Reset(&self) -> windows::core::Result<()> {
        *self.index.lock().unwrap() = 0;
        Ok(())
    }

    fn Clone(&self) -> windows::core::Result<IEnumConnectionPoints> {
        let idx = *self.index.lock().unwrap();
        let clone = EnumConnectionPoints {
            points: self.points.clone(),
            index: Mutex::new(idx),
        };
        Ok(clone.into())
    }
}
