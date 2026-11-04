use std::{collections::BTreeSet, ffi::OsStr, fs, io::Write, os::windows::{ffi::OsStrExt, process::CommandExt}, path::{Path, PathBuf}, process::{Command, Stdio}, ptr, thread, time::Duration};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::{Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, WAIT_ABANDONED, WAIT_OBJECT_0}, System::{Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS}, Registry::{RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ}, RemoteDesktop::ProcessIdToSessionId, Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject}}};
use crate::client_start_state::ClientStartState;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_NAME: &str = "LinkinNotesClientWatcher";

#[derive(Serialize, Deserialize)]
struct WatchConfig { enabled: bool, executable: PathBuf }

struct Handle(HANDLE);
impl Drop for Handle { fn drop(&mut self) { unsafe { CloseHandle(self.0); } } }
struct OwnedMutex(Handle);
impl Drop for OwnedMutex { fn drop(&mut self) { unsafe { ReleaseMutex(self.0.0); } } }

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> { value.as_ref().encode_wide().chain(Some(0)).collect() }
fn win_error(code: u32) -> String { std::io::Error::from_raw_os_error(code as i32).to_string() }

fn read_registration() -> Result<Option<String>, String> {
    let mut data = vec![0u16; 32768];
    let mut size = (data.len() * 2) as u32;
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(RUN_NAME).as_ptr(), RRF_RT_REG_SZ, ptr::null_mut(), data.as_mut_ptr().cast(), &mut size) };
    if status == ERROR_FILE_NOT_FOUND { return Ok(None); }
    if status != 0 { return Err(win_error(status)); }
    Ok(Some(String::from_utf16_lossy(&data[..(size as usize / 2).saturating_sub(1)])))
}

fn write_registration(value: Option<&str>) -> Result<(), String> {
    let mut key = ptr::null_mut();
    let status = unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, ptr::null(), 0, KEY_SET_VALUE, ptr::null(), &mut key, ptr::null_mut()) };
    if status != 0 { return Err(win_error(status)); }
    let status = unsafe {
        let status = if let Some(value) = value {
            let data = wide(value);
            RegSetValueExW(key, wide(RUN_NAME).as_ptr(), 0, REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32)
        } else { RegDeleteValueW(key, wide(RUN_NAME).as_ptr()) };
        RegCloseKey(key);
        status
    };
    if status == 0 || (value.is_none() && status == ERROR_FILE_NOT_FOUND) { Ok(()) } else { Err(win_error(status)) }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    fs::rename(&temporary, path).map_err(|e| e.to_string())
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(data) => Ok(Some(data)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn registration_command(executable: &Path, config: &Path) -> Result<String, String> {
    if !executable.is_absolute() || !config.is_absolute() { return Err("启动路径必须是绝对路径".into()); }
    let command = format!("\"{}\" --watch-client \"{}\"", executable.display(), config.display());
    if command.encode_utf16().count() > 260 { return Err("程序目录过长，请将完整程序目录移到更短的路径后开启联动启动".into()); }
    Ok(command)
}

pub fn configure(enabled: bool, folder: &Path) -> Result<(), String> {
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let path = folder.join("follow-client.json");
    let previous_config = read_optional(&path)?;
    let previous_registration = read_registration()?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let command = if enabled { Some(registration_command(&executable, &path)?) } else { None };
    let update = (|| {
        write_registration(command.as_deref())?;
        atomic_write(&path, &serde_json::to_vec(&WatchConfig { enabled, executable: executable.clone() }).map_err(|e| e.to_string())?)?;
        if enabled {
            Command::new(&executable).arg("--watch-client").arg(&path).creation_flags(0x08000000)
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })();
    if let Err(error) = update {
        let registration = write_registration(previous_registration.as_deref());
        let config = if let Some(data) = previous_config { atomic_write(&path, &data) } else {
            match fs::remove_file(&path) { Ok(()) => Ok(()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(e) => Err(e.to_string()) }
        };
        return Err(format!("联动启动设置失败：{error}{}{}", registration.err().map(|e| format!("；启动项恢复失败：{e}")).unwrap_or_default(), config.err().map(|e| format!("；监听设置恢复失败：{e}")).unwrap_or_default()));
    }
    Ok(())
}

fn mutex_name(path: &Path) -> Vec<u16> {
    let hash = path.as_os_str().encode_wide().fold(0xcbf29ce484222325u64, |hash, value| (hash ^ value as u64).wrapping_mul(0x100000001b3));
    wide(format!("Local\\LinkinNotes.ClientWatcher.{hash:016x}"))
}

fn acquire_watcher(path: &Path) -> Result<Option<OwnedMutex>, String> {
    let raw = unsafe { CreateMutexW(ptr::null(), 1, mutex_name(path).as_ptr()) };
    if raw.is_null() { return Err(std::io::Error::last_os_error().to_string()); }
    let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let handle = Handle(raw);
    if exists {
        let status = unsafe { WaitForSingleObject(handle.0, 3000) };
        if status != WAIT_OBJECT_0 && status != WAIT_ABANDONED { return Ok(None); }
    }
    Ok(Some(OwnedMutex(handle)))
}

fn session_id(pid: u32) -> Option<u32> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(pid, &mut session) } == 0 { None } else { Some(session) }
}

fn client_processes() -> Result<BTreeSet<u32>, String> {
    let session = session_id(std::process::id()).ok_or("无法读取当前登录会话")?;
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE { return Err(std::io::Error::last_os_error().to_string()); }
    let handle = Handle(raw);
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = BTreeSet::new();
    let mut ok = unsafe { Process32FirstW(handle.0, &mut entry) };
    while ok != 0 {
        let end = entry.szExeFile.iter().position(|v| *v == 0).unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case("haoyongbianqian.exe") && session_id(entry.th32ProcessID) == Some(session) {
            found.insert(entry.th32ProcessID);
        }
        ok = unsafe { Process32NextW(handle.0, &mut entry) };
    }
    let status = unsafe { GetLastError() };
    if status != ERROR_NO_MORE_FILES { return Err(win_error(status)); }
    Ok(found)
}

fn log_error(path: &Path, error: &str) {
    let path = path.with_extension("log");
    let append = fs::metadata(&path).map(|m| m.len() < 65536).unwrap_or(true);
    if let Ok(mut file) = fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(path) {
        let _ = writeln!(file, "{error}");
    }
}

fn watch(path: &Path) -> Result<(), String> {
    let Some(_mutex) = acquire_watcher(path)? else { return Ok(()); };
    let mut state = ClientStartState::default();
    let mut read_failures = 0;
    loop {
        let config = fs::read(path).map_err(|e| e.to_string()).and_then(|data| serde_json::from_slice::<WatchConfig>(&data).map_err(|e| e.to_string()));
        match config {
            Ok(config) => {
                read_failures = 0;
                if !config.enabled { return Ok(()); }
                let result = client_processes().and_then(|ids| state.observe(ids, || {
                    Command::new(&config.executable).arg("--client-started").creation_flags(0x08000000)
                        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ()).map_err(|e| e.to_string())
                }));
                if let Err(error) = result { log_error(path, &error); }
            }
            Err(error) => {
                read_failures += 1;
                if read_failures >= 5 { return Err(error); }
            }
        }
        thread::sleep(Duration::from_secs(2));
    }
}

pub fn run_if_requested() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new("--watch-client")) { return false; }
    if let Some(path) = args.next() {
        let path = PathBuf::from(path);
        if let Err(error) = watch(&path) { log_error(&path, &error); }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watcher_config_preserves_unicode_and_spaces() {
        let config = WatchConfig { enabled: true, executable: PathBuf::from("D:\\工具 目录\\LinkinNotes.exe") };
        let loaded: WatchConfig = serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(loaded.executable, config.executable);
        assert!(loaded.enabled);
    }
    #[test]
    fn startup_command_quotes_unicode_paths_and_rejects_unusable_paths() {
        let executable = Path::new("D:\\工具 目录\\LinkinNotes.exe");
        let config = Path::new("C:\\用户 目录\\follow-client.json");
        assert_eq!(registration_command(executable, config).unwrap(), "\"D:\\工具 目录\\LinkinNotes.exe\" --watch-client \"C:\\用户 目录\\follow-client.json\"");
        assert!(registration_command(Path::new("LinkinNotes.exe"), config).is_err());
        assert!(registration_command(&PathBuf::from(format!("D:\\{}\\LinkinNotes.exe", "x".repeat(250))), config).is_err());
    }
    #[test]
    fn disabled_watcher_exits_and_atomic_updates_replace_previous_config() {
        let folder = std::env::temp_dir().join(format!("linkin-startup-test-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("follow-client.json");
        atomic_write(&path, b"old content").unwrap();
        let config = WatchConfig { enabled: false, executable: folder.join("must-not-launch.exe") };
        atomic_write(&path, &serde_json::to_vec(&config).unwrap()).unwrap();
        watch(&path).unwrap();
        assert!(!folder.join("follow-client.tmp").exists());
        fs::remove_file(path).unwrap();
        fs::remove_dir(folder).unwrap();
    }
    #[test]
    fn process_snapshot_and_mutex_do_not_launch_apps_or_register_startup() {
        client_processes().unwrap();
        let path = std::env::temp_dir().join(format!("linkin-watcher-test-{}", std::process::id()));
        let first = acquire_watcher(&path).unwrap().unwrap();
        let path2 = path.clone();
        assert!(thread::spawn(move || acquire_watcher(&path2).unwrap().is_none()).join().unwrap());
        drop(first);
        assert!(acquire_watcher(&path).unwrap().is_some());
    }
}
