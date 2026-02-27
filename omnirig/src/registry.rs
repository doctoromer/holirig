use windows::core::GUID;
use windows::Win32::System::Registry::HKEY_CURRENT_USER;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, KEY_WOW64_32KEY, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
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
    fn new(parent_key: HKEY, name: &str) -> Result<Self> {
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
                KEY_WRITE | KEY_WOW64_32KEY,
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

pub fn register_com_component(
    clsid: &GUID,
    exe_path: &str,
    prog_id: &str,
    version: &str,
) -> Result<()> {
    let clsid_path = format!("SOFTWARE\\Classes\\CLSID\\{{{:?}}}", clsid);
    println!("CLSID path: {clsid_path}");
    let clsid_key = RegKey::new(HKEY_CURRENT_USER, &clsid_path)?;

    let local_server_key = RegKey::new(clsid_key.hkey, "LocalServer32")?;
    local_server_key.set_default_value(exe_path)?;

    let prog_id_key = RegKey::new(clsid_key.hkey, "ProgID")?;
    prog_id_key.set_default_value(prog_id)?;

    let version_key = RegKey::new(clsid_key.hkey, "Version")?;
    version_key.set_default_value(version)?;

    Ok(())
}
