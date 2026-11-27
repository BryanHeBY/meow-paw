//! Current user's WinINet LAN settings. This does not change WinHTTP,
//! VPN/dial-up connection profiles, or application-specific proxies.
use super::{Backend, NonZeroU16, ProxyError, Snapshot};
use std::{io, mem::size_of, ptr};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, GlobalFree};
use windows_sys::Win32::Networking::WinInet::*;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
};

pub(super) struct WinInet;
const FLAG_IDS: [u32; 2] = [INTERNET_PER_CONN_FLAGS, INTERNET_PER_CONN_FLAGS_UI];
const STRING_IDS: [u32; 3] = [
    INTERNET_PER_CONN_PROXY_SERVER,
    INTERNET_PER_CONN_PROXY_BYPASS,
    INTERNET_PER_CONN_AUTOCONFIG_URL,
];

impl WinInet {
    pub(super) fn discover() -> Result<Self, ProxyError> {
        let backend = Self;
        backend.read()?;
        Ok(backend)
    }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}
fn ensure_per_user() -> Result<(), ProxyError> {
    let subkey = wide("Software\\Policies\\Microsoft\\Windows\\CurrentVersion\\Internet Settings");
    let key = wide("ProxySettingsPerUser");
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let mut value = 0u32;
        let mut len = size_of::<u32>() as u32;
        let result = unsafe {
            RegGetValueW(
                hive,
                subkey.as_ptr(),
                key.as_ptr(),
                RRF_RT_REG_DWORD,
                ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut len,
            )
        };
        if result != ERROR_FILE_NOT_FOUND && !(result == ERROR_SUCCESS && value == 1) {
            return Err(ProxyError::Unsupported);
        }
    }
    Ok(())
}
fn list(options: &mut [INTERNET_PER_CONN_OPTIONW]) -> INTERNET_PER_CONN_OPTION_LISTW {
    INTERNET_PER_CONN_OPTION_LISTW {
        dwSize: size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
        pszConnection: ptr::null_mut(), // LAN/default desktop profile.
        dwOptionCount: options.len() as u32,
        dwOptionError: 0,
        pOptions: options.as_mut_ptr(),
    }
}
fn read_string(option: &INTERNET_PER_CONN_OPTIONW) -> Result<String, ProxyError> {
    // WinINet documents GlobalFree for string options allocated by the query.
    let p = unsafe { option.Value.pszValue };
    if p.is_null() {
        return Ok(String::new());
    }
    let mut count = 0;
    while count < 4_096 && unsafe { *p.add(count) } != 0 {
        count += 1;
    }
    let result = if count == 4_096 {
        Err(ProxyError::Unsupported)
    } else {
        String::from_utf16(unsafe { std::slice::from_raw_parts(p, count) })
            .map_err(|_| ProxyError::Unsupported)
    };
    unsafe { GlobalFree(p.cast()) };
    result
}
fn valid_flags(flag: &str) -> Option<u32> {
    let parsed = flag.parse::<u32>().ok()?;
    let supported =
        PROXY_TYPE_DIRECT | PROXY_TYPE_PROXY | PROXY_TYPE_AUTO_DETECT | PROXY_TYPE_AUTO_PROXY_URL;
    (parsed != 0 && parsed & !supported == 0 && parsed.to_string() == flag).then_some(parsed)
}
impl Backend for WinInet {
    fn id(&self) -> &'static str {
        "wininet-lan-v1"
    }
    fn read(&self) -> Result<Snapshot, ProxyError> {
        ensure_per_user()?;
        let mut options = FLAG_IDS
            .into_iter()
            .chain(STRING_IDS)
            .map(|id| INTERNET_PER_CONN_OPTIONW {
                dwOption: id,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let mut list = list(&mut options);
        let mut size = list.dwSize;
        let ok = unsafe {
            InternetQueryOptionW(
                ptr::null(),
                INTERNET_OPTION_PER_CONNECTION_OPTION,
                (&mut list as *mut INTERNET_PER_CONN_OPTION_LISTW).cast(),
                &mut size,
            )
        };
        let error = (ok == 0).then(io::Error::last_os_error);
        // Release all output string buffers, including any allocated before
        // the failure of another option in the same query.
        let strings = options[FLAG_IDS.len()..]
            .iter()
            .map(read_string)
            .collect::<Vec<_>>();
        if let Some(e) = error {
            return Err(e.into());
        }
        let mut values = options[..FLAG_IDS.len()]
            .iter()
            .map(|option| Some(unsafe { option.Value.dwValue }.to_string()))
            .collect::<Vec<_>>();
        for s in strings {
            values.push(Some(s?));
        }
        Ok(Snapshot(values))
    }

    fn desired(&self, original: &Snapshot, port: NonZeroU16) -> Result<Snapshot, ProxyError> {
        let [
            Some(flags),
            Some(ui_flags),
            Some(server),
            Some(bypass),
            Some(pac),
        ] = original.0.as_slice()
        else {
            return Err(ProxyError::Unsupported);
        };
        // WinINet can report different automatic-discovery flags to the
        // control panel and apps. Don't take ownership of ambiguous settings.
        if flags != ui_flags
            || valid_flags(flags).is_none()
            || valid_flags(flags)
                .is_some_and(|f| f & (PROXY_TYPE_AUTO_DETECT | PROXY_TYPE_AUTO_PROXY_URL) != 0)
            || [server, bypass, pac]
                .iter()
                .any(|s| s.contains('\0') || s.encode_utf16().count() >= 4_096)
        {
            return Err(ProxyError::Unsupported);
        }
        let mut state = original.clone();
        let mode = (PROXY_TYPE_PROXY | PROXY_TYPE_DIRECT).to_string();
        state.0[0] = Some(mode.clone());
        state.0[1] = Some(mode);
        state.0[2] = Some(format!("127.0.0.1:{port}"));
        Ok(state)
    }

    fn apply(&self, from: &Snapshot, to: &Snapshot) -> Result<(), ProxyError> {
        if self.read()? != *from {
            return Err(ProxyError::Conflict);
        }
        let flags = to.0[0]
            .as_deref()
            .and_then(valid_flags)
            .ok_or(ProxyError::InvalidJournal)?;
        let mut server = wide(to.0[2].as_deref().ok_or(ProxyError::InvalidJournal)?);
        let mut options = [
            INTERNET_PER_CONN_OPTIONW {
                dwOption: INTERNET_PER_CONN_FLAGS,
                Value: INTERNET_PER_CONN_OPTIONW_0 { dwValue: flags },
            },
            INTERNET_PER_CONN_OPTIONW {
                dwOption: INTERNET_PER_CONN_PROXY_SERVER,
                Value: INTERNET_PER_CONN_OPTIONW_0 {
                    pszValue: server.as_mut_ptr(),
                },
            },
        ];
        let list = list(&mut options);
        let ok = unsafe {
            InternetSetOptionW(
                ptr::null(),
                INTERNET_OPTION_PER_CONNECTION_OPTION,
                (&list as *const INTERNET_PER_CONN_OPTION_LISTW).cast(),
                list.dwSize,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error().into());
        }
        for option in [INTERNET_OPTION_SETTINGS_CHANGED, INTERNET_OPTION_REFRESH] {
            if unsafe { InternetSetOptionW(ptr::null(), option, ptr::null(), 0) } == 0 {
                return Err(io::Error::last_os_error().into());
            }
        }
        Ok(())
    }
}

// Unlike a normal rename this asks NTFS to commit the journal destination
// before the first WinINet write, and does not replace an existing journal.
pub(super) fn persist(temp: tempfile::NamedTempFile, path: &std::path::Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
    let (file, source) = temp.into_parts();
    drop(file);
    let native = |p: &std::path::Path| p.as_os_str().encode_wide().chain([0]).collect::<Vec<_>>();
    if unsafe {
        MoveFileExW(
            native(&source).as_ptr(),
            native(path).as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
