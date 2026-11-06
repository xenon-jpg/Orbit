use crate::error::{Error, Result};
use zeroize::Zeroizing;

// A freshly created job object has no CPU rate control enabled.
#[cfg(any(windows, test))]
fn initial_cpu_cap_requested(percent: u32) -> bool {
    percent > 0
}

#[cfg(not(windows))]
pub fn protect(_data: &[u8]) -> Result<Vec<u8>> {
    Err(Error::DpapiUnavailable)
}
#[cfg(not(windows))]
pub fn unprotect(_data: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    Err(Error::DpapiUnavailable)
}
#[cfg(not(windows))]
pub fn hello_available() -> bool {
    false
}
#[cfg(not(windows))]
pub async fn verify_hello() -> Result<()> {
    Err(Error::HelloRequired)
}

#[cfg(windows)]
pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            windows::core::PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|_| Error::Internal)?;
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(result)
    }
}
#[cfg(windows)]
pub fn unprotect(data: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|_| Error::BadPassword)?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let key: [u8; 32] = bytes.try_into().map_err(|_| Error::BadVault)?;
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(Zeroizing::new(key))
    }
}
#[cfg(windows)]
pub fn hello_available() -> bool {
    use windows::Security::Credentials::UI::{
        UserConsentVerifier, UserConsentVerifierAvailability,
    };
    UserConsentVerifier::CheckAvailabilityAsync()
        .ok()
        .and_then(|op| op.join().ok())
        .is_some_and(|v| v == UserConsentVerifierAvailability::Available)
}
#[cfg(windows)]
pub async fn verify_hello() -> Result<()> {
    tokio::task::spawn_blocking(|| -> Result<()> {
        use windows::Security::Credentials::UI::{
            UserConsentVerificationResult, UserConsentVerifier,
        };
        if !hello_available() {
            return Err(Error::HelloRequired);
        }
        let result = UserConsentVerifier::RequestVerificationAsync(&windows::core::HSTRING::from(
            "Verify your identity to reveal an AltDeck account session",
        ))
        .map_err(|_| Error::HelloRequired)?
        .join()
        .map_err(|_| Error::HelloRequired)?;
        if result == UserConsentVerificationResult::Verified {
            Ok(())
        } else {
            Err(Error::HelloRequired)
        }
    })
    .await
    .map_err(|_| Error::Internal)?
}

#[cfg(windows)]
pub mod win {
    use super::*;
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::PathBuf, sync::mpsc};
    use windows::{
        core::{w, BOOL},
        Win32::{
            Foundation::*,
            Graphics::Gdi::{
                GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
            },
            Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS},
            System::{
                Diagnostics::ToolHelp::*, JobObjects::*, SystemInformation::GetTickCount,
                Threading::*,
            },
            UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
        },
    };

    pub struct Handle(pub HANDLE);
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                unsafe {
                    let _ = CloseHandle(self.0);
                }
            }
        }
    }
    pub struct Singleton {
        shutdown: Option<mpsc::Sender<()>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Singleton {
        pub fn acquire(allow_existing: bool) -> Result<Self> {
            let (tx, rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            let thread = std::thread::spawn(move || {
                let mut handles = Vec::new();
                let mut success = true;
                for name in [w!("ROBLOX_singletonEvent")] {
                    unsafe {
                        match CreateMutexW(None, false, name) {
                            Ok(handle) => {
                                if GetLastError() == ERROR_ALREADY_EXISTS && !allow_existing {
                                    success = false;
                                }
                                handles.push(Handle(handle));
                            }
                            Err(_) => success = false,
                        }
                    }
                }
                let _ = ready_tx.send(success);
                if success {
                    let _ = rx.recv();
                }
                // Non-owned handles only reserve the name; never ReleaseMutex or SetEvent.
            });
            if !ready_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap_or(false)
            {
                let _ = thread.join();
                return Err(Error::ExistingClients);
            }
            Ok(Self {
                shutdown: Some(tx),
                thread: Some(thread),
            })
        }
    }
    impl Drop for Singleton {
        fn drop(&mut self) {
            drop(self.shutdown.take());
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    pub fn roblox_path() -> Result<PathBuf> {
        let local = std::env::var_os("LOCALAPPDATA").ok_or(Error::NotInstalled)?;
        let root = PathBuf::from(local).join("Roblox").join("Versions");
        let registry = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey("Software\\ROBLOX Corporation\\Environments\\roblox-player")
            .ok();
        let candidate: Option<PathBuf> = registry
            .and_then(|key| key.get_value::<String, _>("clientExe").ok())
            .map(PathBuf::from);
        let path = if let Some(path) = candidate {
            path
        } else {
            std::fs::read_dir(&root)
                .map_err(|_| Error::NotInstalled)?
                .flatten()
                .map(|entry| entry.path().join("RobloxPlayerBeta.exe"))
                .filter(|p| p.is_file())
                .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
                .ok_or(Error::NotInstalled)?
        };
        let canonical = path.canonicalize().map_err(|_| Error::NotInstalled)?;
        let root = root.canonicalize().map_err(|_| Error::NotInstalled)?;
        if !canonical.starts_with(root)
            || !canonical
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("RobloxPlayerBeta.exe"))
        {
            return Err(Error::NotInstalled);
        }
        Ok(canonical)
    }

    /// Verify Authenticode and inspect the actual leaf signer from WinTrust's
    /// verified chain, rather than an arbitrary certificate in the PKCS#7 store.
    pub fn verify_signature(path: &std::path::Path) -> Result<()> {
        use windows::core::PCWSTR;
        use windows::Win32::Security::Cryptography::{
            CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE,
        };
        use windows::Win32::Security::WinTrust::*;

        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: PCWSTR(wide.as_ptr()),
            ..Default::default()
        };
        let mut data = WINTRUST_DATA {
            cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
            dwStateAction: WTD_STATEACTION_VERIFY,
            ..Default::default()
        };
        data.dwUnionChoice = WINTRUST_DATA_UNION_CHOICE(1); // WTD_CHOICE_FILE
        data.Anonymous.pFile = &mut file;
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let status = unsafe {
            WinVerifyTrust(
                HWND::default(),
                &mut action,
                &mut data as *mut _ as *mut c_void,
            )
        };

        let mut publisher_ok = false;
        if status == 0 {
            // The signer and certificate pointers belong to WinTrust state and are
            // only valid until WTD_STATEACTION_CLOSE below.
            unsafe {
                let provider = WTHelperProvDataFromStateData(data.hWVTStateData);
                if !provider.is_null() {
                    let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
                    if !signer.is_null() {
                        let leaf = WTHelperGetProvCertFromChain(signer, 0);
                        if !leaf.is_null() && !(*leaf).pCert.is_null() {
                            let mut name = [0u16; 512];
                            let len = CertGetNameStringW(
                                (*leaf).pCert,
                                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                                0,
                                None,
                                Some(&mut name),
                            );
                            if len > 1 && (len as usize) <= name.len() {
                                let publisher = String::from_utf16_lossy(&name[..len as usize - 1]);
                                publisher_ok = publisher.eq_ignore_ascii_case("Roblox Corporation");
                            }
                        }
                    }
                }
            }
        }
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        unsafe {
            let _ = WinVerifyTrust(
                HWND::default(),
                &mut action,
                &mut data as *mut _ as *mut c_void,
            );
        }
        if status != 0 {
            return Err(Error::SignatureStatus(status as u32));
        }
        if !publisher_ok {
            return Err(Error::UntrustedClient);
        }
        Ok(())
    }

    pub fn spawn_client(uri: &str, cpu_percent: u32) -> Result<(u32, Handle)> {
        use std::os::windows::io::IntoRawHandle;
        use std::os::windows::process::CommandExt;
        let exe = roblox_path()?;
        // raw_arg receives our encoded URI, never a user-provided shell command.
        let child = std::process::Command::new(&exe)
            .raw_arg(format!("\"{uri}\""))
            .current_dir(exe.parent().ok_or(Error::NotInstalled)?)
            .creation_flags(CREATE_SUSPENDED.0 | CREATE_NEW_PROCESS_GROUP.0)
            .spawn()
            .map_err(|e| {
                Error::LaunchSystem("process-create", e.raw_os_error().unwrap_or(0) as u32)
            })?;
        let pid = child.id();
        let process = Handle(HANDLE(child.into_raw_handle()));
        struct SuspendedGuard(HANDLE);
        impl Drop for SuspendedGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = TerminateProcess(self.0, 1);
                }
            }
        }
        let guard = SuspendedGuard(process.0);
        let job = Handle(unsafe {
            CreateJobObjectW(None, None)
                .map_err(|e| Error::LaunchSystem("job-create", e.code().0 as u32))?
        });
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_BREAKAWAY_OK | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const c_void,
                std::mem::size_of_val(&limits) as u32,
            )
            .map_err(|e| Error::LaunchSystem("job-limits", e.code().0 as u32))?;
            if let Err(error) = AssignProcessToJobObject(job.0, process.0) {
                let _ = TerminateProcess(process.0, 1);
                return Err(Error::LaunchSystem("job-assign", error.code().0 as u32));
            }
        }
        // A new job has no CPU cap. Windows rejects a zero CpuRate on some
        // systems; only configure rate control when a cap was requested.
        if super::initial_cpu_cap_requested(cpu_percent) {
            set_job_cpu(&job, cpu_percent)?;
        }
        // Find the initial thread without reading client memory.
        let snapshot = Handle(unsafe {
            CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
                .map_err(|e| Error::LaunchSystem("thread-snapshot", e.code().0 as u32))?
        });
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut resumed = false;
        unsafe {
            if Thread32First(snapshot.0, &mut entry).is_ok() {
                loop {
                    if entry.th32OwnerProcessID == pid {
                        if let Ok(thread) =
                            OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
                        {
                            let thread = Handle(thread);
                            if ResumeThread(thread.0) != u32::MAX {
                                resumed = true;
                                break;
                            }
                            return Err(Error::LaunchSystem("thread-resume", GetLastError().0));
                        }
                    }
                    if Thread32Next(snapshot.0, &mut entry).is_err() {
                        break;
                    }
                }
            }
            if !resumed {
                let _ = TerminateProcess(process.0, 1);
                return Err(Error::LaunchStage("thread-not-found"));
            }
        }
        std::mem::forget(guard); // The initial thread is now running.
        Ok((pid, job))
    }
    pub fn set_job_cpu(job: &Handle, percent: u32) -> Result<()> {
        let mut info = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION::default();
        if percent > 0 {
            info.ControlFlags =
                JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
            info.Anonymous.CpuRate = percent.clamp(1, 100) * 100;
        }
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectCpuRateControlInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of_val(&info) as u32,
            )
        }
        .map_err(|e| Error::LaunchSystem("cpu-rate", e.code().0 as u32))
    }
    pub fn process_birth(pid: u32) -> Result<u64> {
        let handle = process_handle(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) }
            .map_err(|_| Error::ClientGone)?;
        Ok((creation.dwHighDateTime as u64) << 32 | creation.dwLowDateTime as u64)
    }
    pub fn process_handle(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Result<Handle> {
        let handle = unsafe { OpenProcess(access, false, pid) }.map_err(|_| Error::ClientGone)?;
        // Callers must validate identity_matches before acting on this handle.
        Ok(Handle(handle))
    }
    pub fn identity_matches(_pid: u32, created_at: u64, handle: HANDLE) -> bool {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe {
            GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user).is_ok()
                && ((creation.dwHighDateTime as u64) << 32 | creation.dwLowDateTime as u64)
                    == created_at
        }
    }
    pub fn memory(pid: u32, created_at: u64) -> Result<(u64, u64)> {
        use windows::Win32::System::ProcessStatus::*;
        let handle = process_handle(
            pid,
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_QUERY_INFORMATION,
        )?;
        if !identity_matches(pid, created_at, handle.0) {
            return Err(Error::ClientGone);
        }
        let mut info = PROCESS_MEMORY_COUNTERS_EX::default();
        unsafe {
            GetProcessMemoryInfo(
                handle.0,
                &mut info as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                std::mem::size_of_val(&info) as u32,
            )
        }
        .map_err(|_| Error::ClientGone)?;
        Ok((
            info.WorkingSetSize as u64 / 1024 / 1024,
            info.PrivateUsage as u64 / 1024 / 1024,
        ))
    }
    pub fn trim_memory(pid: u32, created_at: u64) -> Result<()> {
        use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
        let handle = process_handle(pid, PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA)?;
        if !identity_matches(pid, created_at, handle.0) {
            return Err(Error::ClientGone);
        }
        unsafe { EmptyWorkingSet(handle.0) }.map_err(|_| Error::ClientGone)
    }
    pub fn terminate(pid: u32, created_at: u64) -> Result<()> {
        let handle = process_handle(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE)?;
        if !identity_matches(pid, created_at, handle.0) {
            return Err(Error::ClientGone);
        }
        unsafe { TerminateProcess(handle.0, 0) }.map_err(|_| Error::ClientGone)
    }

    struct EnumState {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = unsafe { &mut *(lparam.0 as *mut EnumState) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid == state.pid
            && unsafe { IsWindowVisible(hwnd).as_bool() }
            && unsafe { GetWindowTextLengthW(hwnd) } > 0
        {
            state.windows.push(hwnd);
        }
        TRUE
    }
    pub fn windows_for(pid: u32) -> Vec<HWND> {
        let mut state = EnumState {
            pid,
            windows: Vec::new(),
        };
        unsafe {
            let _ = EnumWindows(Some(enum_window), LPARAM(&mut state as *mut _ as isize));
        }
        state.windows
    }
    fn validated_window(pid: u32, created_at: u64) -> Result<HWND> {
        let handle = process_handle(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        if !identity_matches(pid, created_at, handle.0) {
            return Err(Error::ClientGone);
        }
        windows_for(pid).into_iter().next().ok_or(Error::ClientGone)
    }
    pub fn window_action(pid: u32, created_at: u64, action: &str) -> Result<()> {
        let hwnd = validated_window(pid, created_at)?;
        unsafe {
            match action {
                "focus" => {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
                "minimize" => {
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                }
                "close" => {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                "small" => {
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        0,
                        0,
                        400,
                        300,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                _ => return Err(Error::Invalid("Unknown window action.")),
            }
        }
        Ok(())
    }
    pub fn tile(clients: &[(u32, u64)], columns: u32) -> Result<()> {
        let count = clients.len().max(1) as u32;
        let columns = columns.max(1).min(count);
        let rows = count.div_ceil(columns);
        for (index, (pid, creation)) in clients.iter().enumerate() {
            let hwnd = validated_window(*pid, *creation)?;
            let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) };
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
                return Err(Error::Internal);
            }
            let area = info.rcWork;
            let width = (area.right - area.left) / columns as i32;
            let height = (area.bottom - area.top) / rows as i32;
            let x = area.left + (index as u32 % columns) as i32 * width;
            let y = area.top + (index as u32 / columns) as i32 * height;
            unsafe {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    x,
                    y,
                    width,
                    height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
        Ok(())
    }
    pub fn system_idle_seconds() -> u64 {
        let mut input = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if unsafe { GetLastInputInfo(&mut input) }.as_bool() {
            (unsafe { GetTickCount() }).wrapping_sub(input.dwTime) as u64 / 1000
        } else {
            0
        }
    }
    /// Real foreground input, never a posted message masquerading as activity.
    pub fn anti_afk(pid: u32, created_at: u64) -> Result<bool> {
        if system_idle_seconds() < 5 {
            return Ok(false);
        }
        let hwnd = validated_window(pid, created_at)?;
        let previous = unsafe { GetForegroundWindow() };
        let minimized = unsafe { IsIconic(hwnd) }.as_bool();
        let mut inputs = [INPUT::default(), INPUT::default()];
        for (i, input) in inputs.iter_mut().enumerate() {
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki.wVk = VIRTUAL_KEY(0x7C); // F13: no standard game action.
            if i == 1 {
                input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
            }
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
            if GetForegroundWindow() != hwnd {
                if minimized {
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                }
                let _ = SetForegroundWindow(previous);
                return Ok(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(80));
            let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            std::thread::sleep(std::time::Duration::from_millis(50));
            if minimized {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            let _ = SetForegroundWindow(previous);
            Ok(sent == 2)
        }
    }

    /// Bounded, occlusion-tolerant PrintWindow capture. This is a 1 fps preview,
    /// not Windows Graphics Capture; minimized windows keep their last frame.
    pub fn preview(pid: u32, created_at: u64) -> Result<String> {
        use windows::Win32::Graphics::Gdi::*;
        let hwnd = validated_window(pid, created_at)?;
        if unsafe { IsIconic(hwnd) }.as_bool() {
            return Err(Error::ClientGone);
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|_| Error::ClientGone)?;
        let width = (rect.right - rect.left).clamp(1, 4096);
        let height = (rect.bottom - rect.top).clamp(1, 4096);
        struct Dc(HDC);
        impl Drop for Dc {
            fn drop(&mut self) {
                unsafe {
                    let _ = DeleteDC(self.0);
                }
            }
        }
        struct Bitmap(HBITMAP);
        impl Drop for Bitmap {
            fn drop(&mut self) {
                unsafe {
                    let _ = DeleteObject(self.0.into());
                }
            }
        }
        let screen = Dc(unsafe { GetDC(Some(hwnd)) });
        let dc = Dc(unsafe { CreateCompatibleDC(Some(screen.0)) });
        let bitmap = Bitmap(unsafe { CreateCompatibleBitmap(screen.0, width, height) });
        let old = unsafe { SelectObject(dc.0, bitmap.0.into()) };
        let ok = unsafe { PrintWindow(hwnd, dc.0, PRINT_WINDOW_FLAGS(2)) }.as_bool();
        let mut bytes = vec![0u8; (width * height * 4) as usize];
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = 0;
        let lines = unsafe {
            GetDIBits(
                dc.0,
                bitmap.0,
                0,
                height as u32,
                Some(bytes.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        unsafe {
            SelectObject(dc.0, old);
        }
        if !ok || lines == 0 {
            return Err(Error::ClientGone);
        }
        let mut img = image::RgbaImage::from_raw(width as u32, height as u32, bytes)
            .ok_or(Error::Internal)?;
        for pixel in img.pixels_mut() {
            pixel.0.swap(0, 2);
            pixel.0[3] = 255;
        }
        let img = image::DynamicImage::ImageRgba8(img).thumbnail(640, 400);
        let mut output = std::io::Cursor::new(Vec::new());
        img.write_to(&mut output, image::ImageFormat::Jpeg)
            .map_err(|_| Error::Internal)?;
        use base64::{engine::general_purpose::STANDARD, Engine};
        Ok(format!(
            "data:image/jpeg;base64,{}",
            STANDARD.encode(output.into_inner())
        ))
    }
}

#[cfg(test)]
mod launch_tests {
    #[test]
    fn initial_job_cpu_cap_is_only_requested_for_nonzero_percent() {
        assert!(!super::initial_cpu_cap_requested(0));
        assert!(super::initial_cpu_cap_requested(1));
        assert!(super::initial_cpu_cap_requested(100));
    }
}
