//! Mots de passe enregistrés (labs SSH, VPN).
//! Linux : trousseau du bureau via `secret-tool` s'il est installé, sinon fichier privé (0600).
//! Windows : fichier chiffré pour le compte Windows (DPAPI).
use std::collections::BTreeMap;
use std::path::PathBuf;

fn file() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "StudentCode").map(|d| d.config_dir().join("secrets.bin"))
}

fn load() -> BTreeMap<String, String> {
    let Some(f) = file() else { return BTreeMap::new() };
    let Ok(raw) = std::fs::read(&f) else { return BTreeMap::new() };
    let Some(plain) = unprotect(&raw) else { return BTreeMap::new() };
    serde_json::from_slice(&plain).unwrap_or_default()
}

fn store(m: &BTreeMap<String, String>) {
    let Some(f) = file() else { return };
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let Ok(plain) = serde_json::to_vec(m) else { return };
    let Some(data) = protect(&plain) else { return };
    let _ = std::fs::write(&f, data);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600));
    }
}

#[cfg(unix)]
fn secret_tool() -> bool {
    crate::runner::which("secret-tool").is_some()
}

pub fn get(key: &str) -> Option<String> {
    #[cfg(unix)]
    if secret_tool() {
        let out = std::process::Command::new("secret-tool").args(["lookup", "service", "StudentCode", "key", key])
            .output().ok()?;
        if out.status.success() && !out.stdout.is_empty() {
            return Some(String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string());
        }
    }
    load().get(key).cloned()
}

pub fn set(key: &str, value: &str) {
    #[cfg(unix)]
    if secret_tool() {
        use std::io::Write;
        if let Ok(mut c) = std::process::Command::new("secret-tool")
            .args(["store", "--label", &format!("StudentCode {key}"), "service", "StudentCode", "key", key])
            .stdin(std::process::Stdio::piped()).spawn() {
            if let Some(mut i) = c.stdin.take() {
                let _ = i.write_all(value.as_bytes());
            }
            if c.wait().map(|s| s.success()).unwrap_or(false) {
                return;
            }
        }
    }
    let mut m = load();
    m.insert(key.to_string(), value.to_string());
    store(&m);
}

pub fn delete(key: &str) {
    #[cfg(unix)]
    if secret_tool() {
        let _ = std::process::Command::new("secret-tool").args(["clear", "service", "StudentCode", "key", key]).status();
    }
    let mut m = load();
    if m.remove(key).is_some() {
        store(&m);
    }
}

#[cfg(not(windows))]
fn protect(d: &[u8]) -> Option<Vec<u8>> {
    Some(d.to_vec())
}
#[cfg(not(windows))]
fn unprotect(d: &[u8]) -> Option<Vec<u8>> {
    Some(d.to_vec())
}

#[cfg(windows)]
fn protect(d: &[u8]) -> Option<Vec<u8>> {
    dpapi(d, true)
}
#[cfg(windows)]
fn unprotect(d: &[u8]) -> Option<Vec<u8>> {
    dpapi(d, false)
}

#[cfg(windows)]
fn dpapi(d: &[u8], encrypt: bool) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: d.len() as u32, pbData: d.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = if encrypt {
            CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out)
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out)
        };
        if ok == 0 {
            return None;
        }
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        Some(v)
    }
}
