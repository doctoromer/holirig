use std::sync::Mutex;

use windows::Win32::System::Com::{ITypeInfo, ITypeLib};
use windows::Win32::System::Ole::{LoadTypeLibEx, REGKIND_NONE};
use windows::core::GUID;
use windows_core::PCWSTR;

// ITypeLib is a COM interface pointer — thread-safe via COM marshaling rules.
struct SyncTypeLib(ITypeLib);
unsafe impl Send for SyncTypeLib {}
unsafe impl Sync for SyncTypeLib {}

static TYPE_LIB: Mutex<Option<SyncTypeLib>> = Mutex::new(None);

pub fn init(tlb_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut slot = TYPE_LIB.lock().unwrap();
    if slot.is_some() {
        return Ok(());
    }
    tracing::info!("Loading type library from {}", tlb_path);
    let wide: Vec<u16> = tlb_path.encode_utf16().chain(std::iter::once(0)).collect();
    let type_lib = unsafe { LoadTypeLibEx(PCWSTR::from_raw(wide.as_ptr()), REGKIND_NONE)? };
    *slot = Some(SyncTypeLib(type_lib));
    Ok(())
}

fn get_type_info(iid: &GUID) -> windows::core::Result<ITypeInfo> {
    let slot = TYPE_LIB.lock().unwrap();
    let lib = &slot
        .as_ref()
        .ok_or_else(|| {
            windows::core::Error::from_hresult(windows::Win32::Foundation::E_UNEXPECTED)
        })?
        .0;
    unsafe { lib.GetTypeInfoOfGuid(iid) }
}

/// IOmniRigX dispatch interface — IID {501A2858-3331-467A-837A-989FDEDACC7D}
pub fn omnirigx_type_info() -> windows::core::Result<ITypeInfo> {
    get_type_info(&GUID::from_u128(0x501A2858_3331_467A_837A_989FDEDACC7D))
}

/// IRigX dispatch interface — IID {D30A7E51-5862-45B7-BFFA-6415917DA0CF}
pub fn rigx_type_info() -> windows::core::Result<ITypeInfo> {
    get_type_info(&GUID::from_u128(0xD30A7E51_5862_45B7_BFFA_6415917DA0CF))
}
