#![allow(non_snake_case)]

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Win32::Foundation::{DISP_E_MEMBERNOTFOUND, E_NOTIMPL};
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, CLSIDFromProgID, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, DISPATCH_FLAGS, DISPATCH_PROPERTYGET, DISPPARAMS, EXCEPINFO, IConnectionPoint,
    IConnectionPointContainer, IDispatch, IDispatch_Impl, ITypeInfo,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, REG_SAM_FLAGS, REG_SZ,
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{GUID, Interface, implement};
use windows_core::{BSTR, PCWSTR};

use omnirig::omnirig::IOmniRigX;
use omnirig::rig::IRigX;
use omnirig::{CLSID_OMNIRIG, PROG_ID};

const IID_OMNIRIG_X_EVENTS: GUID = GUID::from_u128(0x2219175F_E561_47E7_AD17_73C4D8891AA1);

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn hresult_name(hr: i32) -> &'static str {
    match hr {
        0 => "S_OK",
        0x00000001 => "S_FALSE",
        -2147221008 => "CO_E_NOTINITIALIZED",
        -2147221164 => "REGDB_E_CLASSNOTREG",
        -2147467262 => "E_NOINTERFACE",
        -2147467259 => "E_FAIL",
        -2147024891 => "E_ACCESSDENIED",
        -2147221005 => "CO_E_SERVER_EXEC_FAILURE",
        _ => "unknown",
    }
}

fn pass(msg: &str) {
    println!("[PASS] {msg}");
}

fn pass_detail(msg: &str, detail: &str) {
    println!("[PASS] {msg}");
    println!("       {detail}");
}

fn fail(msg: &str, detail: &str) {
    println!("[FAIL] {msg}");
    println!("       {detail}");
}

fn warn(msg: &str, detail: &str) {
    println!("[WARN] {msg}");
    println!("       {detail}");
}

fn read_default_value(hkey: HKEY) -> String {
    let mut buf = vec![0u8; 1024];
    let mut size = buf.len() as u32;
    let mut kind = REG_SZ;
    let result = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR::from_raw([0u16].as_ptr()),
            None,
            Some(&mut kind),
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        )
    };
    if result.is_err() {
        return "(unable to read)".to_string();
    }
    let wide: &[u16] =
        unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u16, (size as usize) / 2) };
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

fn check_registry(extra_access: REG_SAM_FLAGS, view_name: &str) {
    let clsid_path = format!(
        "SOFTWARE\\Classes\\CLSID\\{{{:?}}}\\LocalServer32",
        CLSID_OMNIRIG
    );
    let path_wide = to_wide(&clsid_path);

    let mut hkey = HKEY::default();
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(path_wide.as_ptr()),
            None,
            KEY_READ | extra_access,
            &mut hkey,
        )
    };

    if result.is_ok() {
        let exe = read_default_value(hkey);
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        pass_detail(
            &format!("Registry: CLSID found in {view_name} view (HKCU)"),
            &format!("LocalServer32: {exe}"),
        );
        return;
    }

    let mut hkey = HKEY::default();
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR::from_raw(path_wide.as_ptr()),
            None,
            KEY_READ | extra_access,
            &mut hkey,
        )
    };

    if result.is_ok() {
        let exe = read_default_value(hkey);
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        pass_detail(
            &format!("Registry: CLSID found in {view_name} view (HKLM)"),
            &format!("LocalServer32: {exe}"),
        );
        return;
    }

    warn(
        &format!("Registry: CLSID not found in {view_name} view"),
        "Not in HKCU or HKLM. Server may still work if running (CoRegisterClassObject).",
    );
}

fn run_com_tests(verbose: bool) {
    unsafe {
        if let Err(err) = CoInitializeEx(None, COINIT_MULTITHREADED).ok() {
            fail(
                "COM initialization",
                &format!("CoInitializeEx failed: {err}"),
            );
            return;
        }
    }

    let unknown = unsafe {
        CoCreateInstance::<_, windows::core::IUnknown>(&CLSID_OMNIRIG, None, CLSCTX_LOCAL_SERVER)
    };

    let unknown = match unknown {
        Ok(obj) => {
            pass("CoCreateInstance with CLSID");
            obj
        }
        Err(err) => {
            fail(
                "CoCreateInstance with CLSID",
                &format!(
                    "HRESULT 0x{:08X} ({})",
                    err.code().0 as u32,
                    hresult_name(err.code().0)
                ),
            );
            unsafe {
                CoUninitialize();
            }
            return;
        }
    };

    let prog_id_wide = to_wide(PROG_ID);
    let clsid_result = unsafe { CLSIDFromProgID(PCWSTR::from_raw(prog_id_wide.as_ptr())) };
    match clsid_result {
        Ok(clsid) => {
            pass_detail(
                &format!("CLSIDFromProgID(\"{PROG_ID}\")"),
                &format!("Resolved to {{{:?}}}", clsid),
            );
        }
        Err(err) => {
            fail(
                &format!("CLSIDFromProgID(\"{PROG_ID}\")"),
                &format!(
                    "HRESULT 0x{:08X} ({})",
                    err.code().0 as u32,
                    hresult_name(err.code().0)
                ),
            );
        }
    }

    let omnirig: Option<IOmniRigX> = unknown.cast().ok();
    match &omnirig {
        Some(_) => pass("QueryInterface for IOmniRigX"),
        None => fail(
            "QueryInterface for IOmniRigX",
            &format!("IID: {:?}", IOmniRigX::IID),
        ),
    }

    let dispatch: Option<IDispatch> = unknown.cast().ok();
    match &dispatch {
        Some(_) => pass("QueryInterface for IDispatch"),
        None => fail("QueryInterface for IDispatch", "E_NOINTERFACE"),
    }

    let connection_point_container: Option<IConnectionPointContainer> = unknown.cast().ok();
    match &connection_point_container {
        Some(_) => pass("QueryInterface for IConnectionPointContainer"),
        None => fail(
            "QueryInterface for IConnectionPointContainer",
            "E_NOINTERFACE",
        ),
    }

    println!();

    if let Some(ref omnirig) = omnirig {
        run_vtable_tests(omnirig, verbose);
    }

    if let Some(ref dispatch) = dispatch {
        println!();
        run_dispatch_tests(dispatch);
    }

    if let Some(ref container) = connection_point_container {
        println!();
        run_event_tests(container);
    }

    unsafe {
        CoUninitialize();
    }
}

fn run_vtable_tests(omnirig: &IOmniRigX, verbose: bool) {
    unsafe {
        let mut value = 0i32;
        let hr = omnirig.get_InterfaceVersion(&mut value);
        if hr.0 == 0 {
            let major = (value >> 8) & 0xff;
            let minor = value & 0xff;
            pass_detail(
                &format!("IOmniRigX.InterfaceVersion = {value}"),
                &format!("({major}.{minor})"),
            );
        } else {
            fail(
                "IOmniRigX.InterfaceVersion",
                &format!("HRESULT 0x{:08X} ({})", hr.0 as u32, hresult_name(hr.0)),
            );
        }

        let mut value = 0i32;
        let hr = omnirig.get_SoftwareVersion(&mut value);
        if hr.0 == 0 {
            let major = (value >> 16) & 0xffff;
            let minor = value & 0xffff;
            pass_detail(
                &format!("IOmniRigX.SoftwareVersion = {value}"),
                &format!("({major}.{minor})"),
            );
        } else {
            fail(
                "IOmniRigX.SoftwareVersion",
                &format!("HRESULT 0x{:08X} ({})", hr.0 as u32, hresult_name(hr.0)),
            );
        }

        let mut rig1: Option<IRigX> = None;
        let hr = omnirig.get_Rig1(&mut rig1);
        if let Some(rig1) = &rig1
            && hr.0 == 0
        {
            pass("IOmniRigX.Rig1 -> IRigX");
            run_rig_vtable_tests(rig1, "Rig1", verbose);
        } else {
            fail(
                "IOmniRigX.Rig1",
                &format!("HRESULT 0x{:08X} ({})", hr.0 as u32, hresult_name(hr.0)),
            );
        }

        let mut rig2: Option<IRigX> = None;
        let hr = omnirig.get_Rig2(&mut rig2);
        if hr.0 == 0 && rig2.is_some() {
            pass("IOmniRigX.Rig2 -> IRigX");
        } else {
            fail(
                "IOmniRigX.Rig2",
                &format!("HRESULT 0x{:08X} ({})", hr.0 as u32, hresult_name(hr.0)),
            );
        }
    }
}

fn run_rig_vtable_tests(rig: &IRigX, name: &str, _verbose: bool) {
    unsafe {
        let mut value = 0i32;
        let hr = rig.get_Status(&mut value);
        if hr.0 == 0 {
            let status_name = match value {
                0 => "NotConfigured",
                1 => "Disabled",
                2 => "PortBusy",
                3 => "NotResponding",
                4 => "Online",
                5 => "Transceive",
                _ => "Unknown",
            };
            pass(&format!("{name}.Status = {value} ({status_name})"));
        } else {
            fail(
                &format!("{name}.Status"),
                &format!("HRESULT 0x{:08X}", hr.0 as u32),
            );
        }

        let mut value = 0i32;
        let hr = rig.get_Freq(&mut value);
        if hr.0 == 0 {
            pass(&format!("{name}.Freq = {value}"));
        } else {
            fail(
                &format!("{name}.Freq"),
                &format!("HRESULT 0x{:08X}", hr.0 as u32),
            );
        }

        let mut value = 0i32;
        let hr = rig.get_Mode(&mut value);
        if hr.0 == 0 {
            pass(&format!("{name}.Mode = {value}"));
        } else {
            fail(
                &format!("{name}.Mode"),
                &format!("HRESULT 0x{:08X}", hr.0 as u32),
            );
        }

        let mut value = BSTR::new();
        let hr = rig.get_RigType(&mut value);
        if hr.0 == 0 {
            pass(&format!("{name}.RigType = \"{value}\""));
        } else {
            fail(
                &format!("{name}.RigType"),
                &format!("HRESULT 0x{:08X}", hr.0 as u32),
            );
        }
    }
}

fn dispatch_get_i32(dispatch: &IDispatch, name: &str) -> Result<i32, String> {
    unsafe {
        let name_wide = to_wide(name);
        let name_ptr = PCWSTR::from_raw(name_wide.as_ptr());
        let mut dispid = 0i32;

        dispatch
            .GetIDsOfNames(
                &windows::core::GUID::zeroed(),
                &name_ptr,
                1,
                0x0400, // LOCALE_USER_DEFAULT
                &mut dispid,
            )
            .map_err(|err| {
                format!(
                    "GetIDsOfNames failed: HRESULT 0x{:08X}",
                    err.code().0 as u32
                )
            })?;

        let mut result = VARIANT::default();
        let mut exc_info = std::mem::zeroed();
        let mut arg_err = 0u32;
        let params = DISPPARAMS::default();

        dispatch
            .Invoke(
                dispid,
                &windows::core::GUID::zeroed(),
                0x0400,
                DISPATCH_PROPERTYGET,
                &params,
                Some(&mut result),
                Some(&mut exc_info),
                Some(&mut arg_err),
            )
            .map_err(|err| format!("Invoke failed: HRESULT 0x{:08X}", err.code().0 as u32))?;

        let val = windows::Win32::System::Variant::VariantToInt32(&result)
            .map_err(|err| format!("VariantToInt32 failed: {err}"))?;
        Ok(val)
    }
}

fn run_dispatch_tests(dispatch: &IDispatch) {
    println!("--- IDispatch tests ---");

    match dispatch_get_i32(dispatch, "InterfaceVersion") {
        Ok(value) => pass(&format!("IDispatch InterfaceVersion = {value}")),
        Err(err) => fail("IDispatch InterfaceVersion", &err),
    }

    match dispatch_get_i32(dispatch, "SoftwareVersion") {
        Ok(value) => pass(&format!("IDispatch SoftwareVersion = {value}")),
        Err(err) => fail("IDispatch SoftwareVersion", &err),
    }
}

#[derive(Debug, Clone)]
enum EventRecord {
    VisibleChange,
    RigTypeChange { rig_number: i32 },
    StatusChange { rig_number: i32 },
    ParamsChange { rig_number: i32, params: i32 },
    CustomReply { rig_number: i32 },
    Unknown { dispid: i32, n_args: u32 },
}

#[implement(IDispatch)]
struct EventSink {
    events: Arc<Mutex<Vec<EventRecord>>>,
}

impl IDispatch_Impl for EventSink_Impl {
    fn GetTypeInfoCount(&self) -> windows::core::Result<u32> {
        Ok(0)
    }

    fn GetTypeInfo(&self, _itinfo: u32, _lcid: u32) -> windows::core::Result<ITypeInfo> {
        Err(E_NOTIMPL.into())
    }

    fn GetIDsOfNames(
        &self,
        _riid: *const GUID,
        _rgsznames: *const PCWSTR,
        _cnames: u32,
        _lcid: u32,
        _rgdispid: *mut i32,
    ) -> windows::core::Result<()> {
        Err(DISP_E_MEMBERNOTFOUND.into())
    }

    fn Invoke(
        &self,
        dispidmember: i32,
        _riid: *const GUID,
        _lcid: u32,
        _wflags: DISPATCH_FLAGS,
        pdispparams: *const DISPPARAMS,
        _pvarresult: *mut VARIANT,
        _pexcepinfo: *mut EXCEPINFO,
        _puargerr: *mut u32,
    ) -> windows::core::Result<()> {
        let record = unsafe { decode_event(dispidmember, pdispparams) };
        self.events.lock().unwrap().push(record);
        Ok(())
    }
}

/// Decode an event invocation into an `EventRecord`. DISPPARAMS arguments are stored in
/// reverse order: the last formal parameter is at index 0.
unsafe fn decode_event(dispid: i32, pdispparams: *const DISPPARAMS) -> EventRecord {
    let n_args = if pdispparams.is_null() {
        0
    } else {
        unsafe { (*pdispparams).cArgs }
    };

    let read_arg = |reverse_index: u32| -> Option<i32> {
        if pdispparams.is_null() {
            return None;
        }
        let params = unsafe { &*pdispparams };
        if params.rgvarg.is_null() || reverse_index >= params.cArgs {
            return None;
        }
        let variant = unsafe { &*params.rgvarg.add(reverse_index as usize) };
        unsafe { windows::Win32::System::Variant::VariantToInt32(variant).ok() }
    };

    match dispid {
        0x01 => EventRecord::VisibleChange,
        0x02 => EventRecord::RigTypeChange {
            rig_number: read_arg(0).unwrap_or(-1),
        },
        0x03 => EventRecord::StatusChange {
            rig_number: read_arg(0).unwrap_or(-1),
        },
        0x04 => EventRecord::ParamsChange {
            rig_number: read_arg(1).unwrap_or(-1),
            params: read_arg(0).unwrap_or(0),
        },
        0x05 => EventRecord::CustomReply {
            rig_number: read_arg(2).unwrap_or(-1),
        },
        other => EventRecord::Unknown {
            dispid: other,
            n_args,
        },
    }
}

impl fmt::Display for EventRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventRecord::VisibleChange => write!(f, "VisibleChange()"),
            EventRecord::RigTypeChange { rig_number } => {
                write!(f, "RigTypeChange(rig={rig_number})")
            }
            EventRecord::StatusChange { rig_number } => {
                write!(f, "StatusChange(rig={rig_number})")
            }
            EventRecord::ParamsChange { rig_number, params } => write!(
                f,
                "ParamsChange(rig={rig_number}, params=0x{:08X})",
                *params as u32
            ),
            EventRecord::CustomReply { rig_number } => {
                write!(f, "CustomReply(rig={rig_number})")
            }
            EventRecord::Unknown { dispid, n_args } => {
                write!(f, "Unknown(dispid=0x{dispid:02X}, args={n_args})")
            }
        }
    }
}

fn run_event_tests(container: &IConnectionPointContainer) {
    println!("--- IOmniRigXEvents tests ---");

    let connection_point: IConnectionPoint = unsafe {
        match container.FindConnectionPoint(&IID_OMNIRIG_X_EVENTS) {
            Ok(connection_point) => {
                pass("IConnectionPointContainer.FindConnectionPoint(IOmniRigXEvents)");
                connection_point
            }
            Err(err) => {
                fail(
                    "IConnectionPointContainer.FindConnectionPoint(IOmniRigXEvents)",
                    &format!("HRESULT 0x{:08X}", err.code().0 as u32),
                );
                return;
            }
        }
    };

    unsafe {
        match connection_point.GetConnectionInterface() {
            Ok(iid) if iid == IID_OMNIRIG_X_EVENTS => {
                pass("IConnectionPoint.GetConnectionInterface() = IOmniRigXEvents");
            }
            Ok(iid) => {
                fail(
                    "IConnectionPoint.GetConnectionInterface()",
                    &format!("Returned unexpected IID: {iid:?}"),
                );
            }
            Err(err) => {
                warn(
                    "IConnectionPoint.GetConnectionInterface()",
                    &format!("HRESULT 0x{:08X}", err.code().0 as u32),
                );
            }
        }
    }

    let events: Arc<Mutex<Vec<EventRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = EventSink {
        events: Arc::clone(&events),
    };
    let sink_dispatch: IDispatch = sink.into();
    let sink_unknown: windows::core::IUnknown = sink_dispatch.cast().unwrap();

    let cookie = unsafe {
        match connection_point.Advise(&sink_unknown) {
            Ok(c) => {
                pass_detail(
                    "IConnectionPoint.Advise(eventSink)",
                    &format!("cookie = {c}"),
                );
                c
            }
            Err(err) => {
                fail(
                    "IConnectionPoint.Advise(eventSink)",
                    &format!("HRESULT 0x{:08X}", err.code().0 as u32),
                );
                return;
            }
        }
    };

    let listen_window = Duration::from_secs(2);
    println!(
        "       Listening for events for {} seconds...",
        listen_window.as_secs()
    );

    // We're MTA, so the OmniRig server can call our sink directly on its worker
    // thread without marshaling. No message pump is needed — just wait.
    std::thread::sleep(listen_window);

    let captured = events.lock().unwrap().clone();
    if captured.is_empty() {
        warn(
            "Event delivery",
            "No events received during listening window (this may be normal if no rig state changed)",
        );
    } else {
        pass_detail(
            &format!("Event delivery: received {} event(s)", captured.len()),
            &captured
                .iter()
                .map(EventRecord::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    unsafe {
        match connection_point.Unadvise(cookie) {
            Ok(()) => pass("IConnectionPoint.Unadvise()"),
            Err(err) => fail(
                "IConnectionPoint.Unadvise()",
                &format!("HRESULT 0x{:08X}", err.code().0 as u32),
            ),
        }
    }
}

pub fn run(verbose: bool) {
    println!("OmniRig COM Diagnostic");
    println!("======================");
    println!();

    check_registry(REG_SAM_FLAGS(0), "64-bit");
    check_registry(KEY_WOW64_32KEY, "32-bit");

    println!();

    run_com_tests(verbose);
}
