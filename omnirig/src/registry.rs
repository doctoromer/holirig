use windows::core::GUID;
use windows::Win32::System::Registry::HKEY_CURRENT_USER;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, KEY_WOW64_32KEY,
    KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ,
};
use windows_core::PCWSTR;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

fn to_wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Clone)]
struct RegKey {
    hkey: HKEY,
}

impl RegKey {
    fn new(parent_key: HKEY, name: &str, extra_access: REG_SAM_FLAGS) -> Result<Self> {
        let mut hkey = HKEY::default();
        let name_wide = to_wide_string(name);
        let name_pcwstr = PCWSTR::from_raw(name_wide.as_ptr());
        unsafe {
            RegCreateKeyExW(
                parent_key,
                name_pcwstr,
                Some(0),
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE | extra_access,
                None,
                &mut hkey,
                None,
            )
            .ok()?;
        }
        Ok(Self { hkey })
    }

    fn set_default_value(&self, value: &str) -> Result<()> {
        let value = to_wide_string(value);

        unsafe {
            let value: &[u8] =
                std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2);

            RegSetValueExW(
                self.hkey,
                PCWSTR::from_raw([0u16].as_ptr()),
                Some(0),
                REG_SZ,
                Some(value),
            )
            .ok()?
        }

        Ok(())
    }
}

impl From<HKEY> for RegKey {
    fn from(hkey: HKEY) -> Self {
        Self { hkey }
    }
}

impl Drop for RegKey {
    fn drop(&mut self) {
        unsafe {
            // Best effort
            let _ = RegCloseKey(self.hkey).ok();
        }
    }
}

fn register_com_component_with_access(
    clsid: &GUID,
    exe_path: &str,
    prog_id: &str,
    version: &str,
    extra_access: REG_SAM_FLAGS,
) -> Result<()> {
    let clsid_path = format!("SOFTWARE\\Classes\\CLSID\\{{{:?}}}", clsid);
    let clsid_key = RegKey::new(HKEY_CURRENT_USER, &clsid_path, extra_access)?;

    let local_server_key = RegKey::new(clsid_key.hkey, "LocalServer32", extra_access)?;
    local_server_key.set_default_value(exe_path)?;

    let prog_id_key = RegKey::new(clsid_key.hkey, "ProgID", extra_access)?;
    prog_id_key.set_default_value(prog_id)?;

    let version_key = RegKey::new(clsid_key.hkey, "Version", extra_access)?;
    version_key.set_default_value(version)?;

    Ok(())
}

fn unregister_com_component_with_access(clsid: &GUID, extra_access: REG_SAM_FLAGS) -> Result<()> {
    let parent_key = RegKey::new(HKEY_CURRENT_USER, "SOFTWARE\\Classes\\CLSID", extra_access)?;
    let subkey = to_wide_string(&format!("{{{:?}}}", clsid));
    unsafe {
        let _ = RegDeleteTreeW(parent_key.hkey, PCWSTR::from_raw(subkey.as_ptr())).ok();
    }
    Ok(())
}

pub fn register_com_component(
    clsid: &GUID,
    exe_path: &str,
    prog_id: &str,
    version: &str,
) -> Result<()> {
    println!("Registering COM component {{{:?}}}", clsid);
    register_com_component_with_access(clsid, exe_path, prog_id, version, REG_SAM_FLAGS(0))?;
    register_com_component_with_access(clsid, exe_path, prog_id, version, KEY_WOW64_32KEY)?;
    Ok(())
}

pub fn unregister_com_component(clsid: &GUID) -> Result<()> {
    let _ = unregister_com_component_with_access(clsid, REG_SAM_FLAGS(0));
    let _ = unregister_com_component_with_access(clsid, KEY_WOW64_32KEY);
    Ok(())
}
