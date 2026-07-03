use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use wasapi::*;

use crate::debug::debug_log_dev;

/// 列出正在播放音频的进程（使用 IAudioSessionManager2 枚举所有活跃音频会话）
#[cfg(windows)]
pub fn list_audio_processes() -> Result<Vec<(String, String, u32)>, String> {
    use std::collections::HashSet;
    use windows::core::*;
    use windows::Win32::System::Com::*;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Diagnostics::ToolHelp::*;

    // 第一步：用 ToolHelp 构建 pid → exe全路径 的映射，用于读取版本信息
    let mut pid_to_path: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    let mut pid_to_exe: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    unsafe {
        if let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..std::mem::zeroed()
            };
            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let pid = entry.th32ProcessID;
                    let exe = String::from_utf16_lossy(
                        &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0)],
                    );
                    pid_to_exe.insert(pid, exe.clone());
                    // 用 Module32First 获取 exe 全路径
                    let mut mod_entry = MODULEENTRY32W {
                        dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
                        ..std::mem::zeroed()
                    };
                    if let Ok(mod_snap) = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) {
                        if Module32FirstW(mod_snap, &mut mod_entry).is_ok() {
                            let path = String::from_utf16_lossy(
                                &mod_entry.szExePath[..mod_entry.szExePath.iter().position(|&c| c == 0).unwrap_or(0)],
                            );
                            pid_to_path.insert(pid, path);
                        }
                        let _ = windows::Win32::Foundation::CloseHandle(mod_snap);
                    }
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = windows::Win32::Foundation::CloseHandle(snapshot);
        }
    }

    // 第二步：用 IAudioSessionManager2 获取所有有音频会话的进程（包括暂停的）
    let my_pid = std::process::id();
    let mut active_pids: Vec<u32> = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();

        if let Ok(device_enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        ) {
            if let Ok(default_device) = device_enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                if let Ok(session_manager) = default_device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                    if let Ok(session_enumerator) = session_manager.GetSessionEnumerator() {
                        if let Ok(count) = session_enumerator.GetCount() {
                            for i in 0..count {
                                if let Ok(control) = session_enumerator.GetSession(i) {
                                    if let Ok(simple) = control.cast::<IAudioSessionControl2>() {
                                        if let Ok(pid) = simple.GetProcessId() {
                                            let pid = pid as u32;
                                            if pid > 0 && pid != my_pid {
                                                let state = control.GetState().unwrap_or(AudioSessionState(0));
                                                // 活跃会话直接加入；非活跃会话只加入已知播放器
                                                if state == AudioSessionState(1) {
                                                    active_pids.push(pid);
                                                } else {
                                                    // 检查 exe 名是否匹配已知播放器
                                                    let exe = pid_to_exe.get(&pid).cloned().unwrap_or_default();
                                                    let exe_lower = exe.to_lowercase();
                                                    let is_known_player = APP_NAME_MAP.iter().any(|(key, _)| exe_lower.contains(*key));
                                                    if is_known_player {
                                                        active_pids.push(pid);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 第三步：合并结果
    let mut processes = Vec::new();
    let mut seen_pids: HashSet<u32> = HashSet::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    for pid in active_pids {
        if seen_pids.contains(&pid) {
            continue;
        }
        seen_pids.insert(pid);
        let exe = pid_to_exe.get(&pid).cloned().unwrap_or_default();
        let exe_lower = exe.to_lowercase();

        // 优先级：映射表 > 窗口标题清理 > FileDescription > 注册表 > ProductName > exe名
        // 映射表：找所有匹配项，选最长的 key（最具体的）
        let friendly = APP_NAME_MAP.iter()
            .filter(|(key, _)| exe_lower.contains(*key))
            .max_by_key(|(key, _)| key.len())
            .map(|(_, name)| name.to_string())
            .or_else(|| {
                get_window_title_for_pid(pid)
                    .map(|t| clean_window_title(&t))
                    .filter(|s| !s.is_empty() && s.len() > 1)
            })
            .or_else(|| {
                pid_to_path.get(&pid).and_then(|path| get_file_description(path))
            })
            .or_else(|| {
                let exe_name = exe.strip_suffix(".exe").unwrap_or(&exe);
                get_app_display_name_from_registry(exe_name)
            })
            .or_else(|| {
                pid_to_path.get(&pid).and_then(|path| get_product_name_from_path(path))
            })
            .unwrap_or_else(|| exe.strip_suffix(".exe").unwrap_or(&exe).to_string());

        // 过滤空名称和重复名称
        if friendly.is_empty() || seen_names.contains(&friendly) {
            continue;
        }
        seen_names.insert(friendly.clone());

        processes.push((friendly, exe, pid));
    }

    Ok(processes)
}

/// 常见应用兜底映射（exe名包含匹配，长的优先）
const APP_NAME_MAP: &[(&str, &str)] = &[
    ("qqbrowser", "QQ 浏览器"),
    ("cloudmusic", "网易云音乐"),
    ("qqmusic", "QQ 音乐"),
    ("kugou", "酷狗音乐"),
    ("kwmusic", "酷我音乐"),
    ("kuwo", "酷我音乐"),
    ("musicbee", "MusicBee"),
    ("foobar", "foobar2000"),
    ("winamp", "Winamp"),
    ("potplayer", "PotPlayer"),
    ("spotify", "Spotify"),
    ("douyin", "抖音"),
    ("weixin", "微信"),
    ("wechat", "微信"),
    ("obs64", "OBS Studio"),
    ("obs32", "OBS Studio"),
    ("itunes", "iTunes"),
    ("aimp", "AIMP"),
    ("chrome", "Chrome"),
    ("msedge", "Edge"),
    ("firefox", "Firefox"),
    ("steam", "Steam"),
    ("qq", "QQ"),
];

/// 清理窗口标题，去掉动态后缀
fn clean_window_title(title: &str) -> String {
    let mut result = title.to_string();

    // 去掉 " - " 后面的内容（OBS: "OBS 32.1.2 - 配置文件: xxx - 场景: xxx"）
    if let Some(pos) = result.find(" - ") {
        let prefix = result[..pos].to_string();
        if prefix.len() >= 2 {
            result = prefix;
        }
    }

    // 去掉末尾的版本号（如 "OBS 32.1.2" → "OBS"）
    // 手动匹配：从末尾往前找 " 数字.数字(.数字)"
    if let Some(last_space) = result.rfind(' ') {
        let after_space = &result[last_space + 1..];
        let parts: Vec<&str> = after_space.split('.').collect();
        if parts.len() >= 2 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
            result = result[..last_space].to_string();
        }
    }

    result
}

/// 获取进程主窗口的标题（本地化名称）
#[cfg(windows)]
fn get_window_title_for_pid(pid: u32) -> Option<String> {
    extern "system" {
        pub fn EnumWindows(
            callback: *mut core::ffi::c_void,
            lparam: isize,
        ) -> windows::Win32::Foundation::BOOL;
        pub fn GetWindowThreadProcessId(
            hwnd: windows::Win32::Foundation::HWND,
            process_id: *mut u32,
        ) -> u32;
        pub fn GetWindowTextW(
            hwnd: windows::Win32::Foundation::HWND,
            buf: *mut u16,
            max_count: i32,
        ) -> i32;
        pub fn IsWindowVisible(hwnd: windows::Win32::Foundation::HWND) -> windows::Win32::Foundation::BOOL;
    }

    struct EnumCtx {
        target_pid: u32,
        found_title: Option<String>,
    }

    unsafe extern "system" fn enum_callback(hwnd: windows::Win32::Foundation::HWND, lparam: isize) -> windows::Win32::Foundation::BOOL {
        let ctx = &mut *(lparam as *mut EnumCtx);
        let mut window_pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut window_pid);
        if window_pid == ctx.target_pid && IsWindowVisible(hwnd).as_bool() {
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
            if len > 0 {
                let title = String::from_utf16_lossy(&buf[..len as usize]);
                if !title.is_empty() {
                    ctx.found_title = Some(title);
                }
            }
        }
        windows::Win32::Foundation::BOOL(1) // continue enumeration
    }

    unsafe {
        let mut ctx = EnumCtx { target_pid: pid, found_title: None };
        let _ = EnumWindows(
            enum_callback as *mut _,
            &mut ctx as *mut _ as isize,
        );
        ctx.found_title
    }
}

/// 从 exe 版本信息中读取 FileDescription（比 ProductName 更准确）
#[cfg(windows)]
fn get_file_description(path: &str) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        pub fn GetFileVersionInfoSizeW(filename: *const u16, dummy: *mut u32) -> u32;
        pub fn GetFileVersionInfoW(filename: *const u16, handle: u32, size: u32, data: *mut core::ffi::c_void) -> windows::Win32::Foundation::BOOL;
        pub fn VerQueryValueW(block: *const core::ffi::c_void, subblock: *const u16, buffer: *mut *mut core::ffi::c_void, size: *mut u32) -> windows::Win32::Foundation::BOOL;
    }

    let path_wide: Vec<u16> = OsStr::new(path).encode_wide().chain(std::iter::once(0)).collect();

    unsafe {
        let size = GetFileVersionInfoSizeW(path_wide.as_ptr(), std::ptr::null_mut());
        if size == 0 { return None; }

        let mut buffer = vec![0u8; size as usize];
        if !GetFileVersionInfoW(path_wide.as_ptr(), 0, size, buffer.as_mut_ptr() as *mut _).as_bool() {
            return None;
        }

        // 先查 Translation 获取实际语言码
        let mut lang_ptr: *mut u8 = std::ptr::null_mut();
        let mut lang_len: u32 = 0;
        let trans_query: Vec<u16> = OsStr::new("\\VarFileInfo\\Translation").encode_wide().chain(std::iter::once(0)).collect();

        let mut queries_to_try: Vec<String> = Vec::new();

        if VerQueryValueW(buffer.as_ptr() as *const _, trans_query.as_ptr(), &mut lang_ptr as *mut *mut _ as *mut *mut _, &mut lang_len).as_bool()
            && !lang_ptr.is_null() && lang_len >= 4
        {
            let lang_data = std::slice::from_raw_parts(lang_ptr as *const u16, lang_len as usize / 2);
            let lang = lang_data[0];
            let codepage = lang_data[1];
            queries_to_try.push(format!("\\StringFileInfo\\{:04X}{:04X}\\FileDescription", lang, codepage));
        }
        queries_to_try.push("\\StringFileInfo\\080404B0\\FileDescription".to_string());
        queries_to_try.push("\\StringFileInfo\\040904B0\\FileDescription".to_string());
        queries_to_try.push("\\StringFileInfo\\040904E4\\FileDescription".to_string());
        queries_to_try.push("\\StringFileInfo\\040404B0\\FileDescription".to_string());

        for query in &queries_to_try {
            let query_wide: Vec<u16> = OsStr::new(query).encode_wide().chain(std::iter::once(0)).collect();
            let mut value_ptr: *mut u16 = std::ptr::null_mut();
            let mut value_len: u32 = 0;

            if VerQueryValueW(buffer.as_ptr() as *const _, query_wide.as_ptr(), &mut value_ptr as *mut *mut _ as *mut *mut _, &mut value_len).as_bool()
                && !value_ptr.is_null() && value_len > 1
            {
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(value_ptr, (value_len - 1) as usize));
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
}

/// 从注册表查询应用显示名
#[cfg(windows)]
fn get_app_display_name_from_registry(exe_name: &str) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        pub fn RegOpenKeyExW(
            hkey: isize,
            subkey: *const u16,
            reserved: u32,
            sam: u32,
            phkresult: *mut isize,
        ) -> i32;
        pub fn RegQueryValueExW(
            hkey: isize,
            valuename: *const u16,
            reserved: *mut u32,
            pdwtype: *mut u32,
            pbdata: *mut u8,
            pcbdata: *mut u32,
        ) -> i32;
        pub fn RegCloseKey(hkey: isize) -> i32;
    }

    const HKEY_LOCAL_MACHINE: isize = 0x80000002;
    const KEY_READ: u32 = 0x20019;
    const REG_SZ: u32 = 1;

    // 尝试 App Paths 和 Uninstall 两个位置
    let registry_paths = [
        format!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{}.exe", exe_name),
        format!("SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}", exe_name),
        format!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}", exe_name),
    ];

    for reg_path in &registry_paths {
        let key_wide: Vec<u16> = OsStr::new(reg_path).encode_wide().chain(std::iter::once(0)).collect();
        let name_wide: Vec<u16> = OsStr::new("DisplayName").encode_wide().chain(std::iter::once(0)).collect();

        unsafe {
            let mut hkey: isize = 0;
            if RegOpenKeyExW(HKEY_LOCAL_MACHINE, key_wide.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
                let mut buf = [0u16; 256];
                let mut buf_len = (buf.len() * 2) as u32;
                let mut reg_type: u32 = 0;
                if RegQueryValueExW(hkey, name_wide.as_ptr(), std::ptr::null_mut(), &mut reg_type, buf.as_mut_ptr() as *mut u8, &mut buf_len) == 0
                    && reg_type == REG_SZ
                {
                    let len = (buf_len / 2) as usize;
                    let name = String::from_utf16_lossy(&buf[..len]);
                    let _ = RegCloseKey(hkey);
                    if !name.is_empty() {
                        return Some(name);
                    }
                }
                let _ = RegCloseKey(hkey);
            }
        }
    }

    None
}

/// 从 exe 文件的版本信息中读取 ProductName
#[cfg(windows)]
fn get_product_name_from_path(path: &str) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        pub fn GetFileVersionInfoSizeW(
            filename: *const u16,
            dummy: *mut u32,
        ) -> u32;
        pub fn GetFileVersionInfoW(
            filename: *const u16,
            handle: u32,
            size: u32,
            data: *mut core::ffi::c_void,
        ) -> windows::Win32::Foundation::BOOL;
        pub fn VerQueryValueW(
            block: *const core::ffi::c_void,
            subblock: *const u16,
            buffer: *mut *mut core::ffi::c_void,
            size: *mut u32,
        ) -> windows::Win32::Foundation::BOOL;
    }

    let path_wide: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let size = GetFileVersionInfoSizeW(path_wide.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }

        let mut buffer = vec![0u8; size as usize];
        if !GetFileVersionInfoW(path_wide.as_ptr(), 0, size, buffer.as_mut_ptr() as *mut _).as_bool() {
            return None;
        }

        // 先查询 VarFileInfo\Translation 获取实际语言码和代码页
        let mut lang_ptr: *mut u8 = std::ptr::null_mut();
        let mut lang_len: u32 = 0;
        let trans_query: Vec<u16> = OsStr::new("\\VarFileInfo\\Translation")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut queries_to_try: Vec<String> = Vec::new();

        if VerQueryValueW(
            buffer.as_ptr() as *const _,
            trans_query.as_ptr(),
            &mut lang_ptr as *mut *mut _ as *mut *mut _,
            &mut lang_len,
        ).as_bool() && !lang_ptr.is_null() && lang_len >= 4
        {
            // Translation 是 LANG + CODEPAGE 的数组，每个 4 字节
            let lang_data = std::slice::from_raw_parts(lang_ptr as *const u16, lang_len as usize / 2);
            let lang = lang_data[0];
            let codepage = lang_data[1];
            // 用实际语言码构造查询
            let query = format!("\\StringFileInfo\\{:04X}{:04X}\\ProductName", lang, codepage);
            queries_to_try.push(query);
        }

        // fallback：常见语言码
        queries_to_try.push("\\StringFileInfo\\080404B0\\ProductName".to_string());
        queries_to_try.push("\\StringFileInfo\\040904B0\\ProductName".to_string());
        queries_to_try.push("\\StringFileInfo\\040904E4\\ProductName".to_string());
        queries_to_try.push("\\StringFileInfo\\040404B0\\ProductName".to_string());

        for query in &queries_to_try {
            let query_wide: Vec<u16> = OsStr::new(query)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut value_ptr: *mut u16 = std::ptr::null_mut();
            let mut value_len: u32 = 0;

            if VerQueryValueW(
                buffer.as_ptr() as *const _,
                query_wide.as_ptr(),
                &mut value_ptr as *mut *mut _ as *mut *mut _,
                &mut value_len,
            ).as_bool() && !value_ptr.is_null() && value_len > 1
            {
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(
                    value_ptr,
                    (value_len - 1) as usize,
                ));
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }

    None
}

#[cfg(not(windows))]
pub fn list_audio_processes() -> Result<Vec<(String, String, u32)>, String> {
    Ok(vec![])
}

/// BGM 按进程捕获线程：使用 WASAPI Process Loopback API
pub fn bgm_process_loop(
    running: Arc<AtomicBool>,
    sender: Sender<Vec<i16>>,
    pid: u32,
    skip_rate: Arc<AtomicU32>,
) -> Result<(), String> {
    // 提升 BGM 线程到实时优先级，防止游戏抢占 CPU
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(hThread: isize, nPriority: i32) -> i32;
        }
        SetThreadPriority(GetCurrentThread(), 2); // THREAD_PRIORITY_HIGHEST
    }

    debug_log_dev(&format!("bgm_loop[{}]: initializing MTA", pid));
    let _ = initialize_mta().ok();

    debug_log_dev(&format!("bgm_loop[{}]: creating loopback client", pid));
    let mut client = AudioClient::new_application_loopback_client(pid, true)
        .map_err(|e| format!("Failed to create process loopback client: {}", e))?;
    if !running.load(Ordering::Acquire) {
        return Ok(()); // 被要求停止，释放 WASAPI 资源
    }

    // 进程 loopback 不支持 get_mixformat()，使用固定格式：32-bit float, 48kHz, 立体声
    let format = WaveFormat::new(32, 32, &SampleType::Float, 48000, 2, None);

    let channels = 2;
    let bytes_per_sample = 4;
    let bytes_per_frame = channels * bytes_per_sample;
    let is_float = true;

    log::info!(
        "BGM process loopback: pid={}, 2ch, 32bit, 48kHz, float=true",
        pid
    );

    // 进程 loopback 不支持 get_device_period()，用 0 让 WASAPI 用默认值
    debug_log_dev(&format!("bgm_loop[{}]: initializing client", pid));
    let bgm_mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 0,
    };
    client
        .initialize_client(&format, &Direction::Capture, &bgm_mode)
        .map_err(|e| format!("Failed to initialize BGM client: {}", e))?;
    if !running.load(Ordering::Acquire) {
        return Ok(());
    }

    debug_log_dev(&format!("bgm_loop[{}]: setting event handle", pid));
    let event_handle = client
        .set_get_eventhandle()
        .map_err(|e| format!("Failed to set BGM event handle: {}", e))?;

    debug_log_dev(&format!("bgm_loop[{}]: getting capture client", pid));
    let capture = client
        .get_audiocaptureclient()
        .map_err(|e| format!("Failed to get BGM capture client: {}", e))?;
    if !running.load(Ordering::Acquire) {
        return Ok(());
    }

    debug_log_dev(&format!("bgm_loop[{}]: starting stream", pid));
    client
        .start_stream()
        .map_err(|e| format!("Failed to start BGM stream: {}", e))?;

    let frame_size = 480;
    let mut buffer = vec![0u8; frame_size * bytes_per_frame];

    // 预分配 buffer 池，避免每帧堆分配（2 个轮换，channel 持有时用另一个）
    let max_samples = frame_size * channels;
    let mut buf_pool: Vec<Vec<i16>> = (0..2).map(|_| Vec::with_capacity(max_samples)).collect();
    let mut buf_idx: usize = 0;

    log::info!("BGM process loopback started for pid={}", pid);
    debug_log_dev(&format!("bgm_loop[{}]: stream started, entering main loop", pid));

    let mut frame_count: u64 = 0;
    let mut consecutive_errors: u32 = 0;

    while running.load(Ordering::Acquire) {
        if event_handle.wait_for_event(100).is_err() {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }

        match capture.read_from_device(&mut buffer) {
            Ok((frames_read, _flags)) => {
                consecutive_errors = 0; // 重置错误计数
                if frames_read == 0 {
                    continue;
                }

                // 自适应漂移补偿：output_pending 水位过高时，audio_loop 设置 skip_rate > 0，
                // 此处按概率跳过当前帧，从源头减缓 BGM 数据流入，避免 output_pending 堆积。
                let rate = f32::from_bits(skip_rate.load(Ordering::Relaxed));
                if rate > 0.001 {
                    // 确定性跳帧：每 N 帧跳过 1 帧（N = 1/rate，如 rate=0.01 → 每 100 帧跳 1 帧）
                    let n = ((1.0 / rate) as u64).max(2);
                    if frame_count % n == 0 {
                        frame_count += 1;
                        continue;
                    }
                }

                let bytes_read = frames_read as usize * bytes_per_frame;

                // 从 buffer 池取一个 buffer，避免每帧堆分配
                let mut samples = std::mem::take(&mut buf_pool[buf_idx]);
                samples.clear();

                if is_float && bytes_per_sample == 4 {
                    for chunk in buffer[..bytes_read].chunks_exact(4) {
                        let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        samples.push((val.clamp(-1.0, 1.0) * 32767.0) as i16);
                    }
                } else if bytes_per_sample == 4 {
                    for chunk in buffer[..bytes_read].chunks_exact(4) {
                        let val = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        samples.push((val >> 16) as i16);
                    }
                } else if bytes_per_sample == 2 {
                    for chunk in buffer[..bytes_read].chunks_exact(2) {
                        samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
                    }
                } else {
                    samples.resize(frames_read as usize, 0);
                };

                // WASAPI 格式固定为 2ch stereo，samples 已经是交错立体声，直接发送
                // （channels 始终为 2，mono 分支不会触发）

                frame_count += 1;
                if frame_count % 100 == 1 {
                    let peak = samples.iter().map(|s| s.abs()).fold(0i16, i16::max);
                    debug_log_dev(&format!("bgm_loop[{}]: frame {} ok, {} samples, peak={}", pid, frame_count, samples.len(), peak));
                }

                // 非阻塞发送，channel 满时短暂等待并检查退出标志
                let send_len = samples.len();
                let mut samples_to_send = Some(samples);
                while let Some(s) = samples_to_send.take() {
                    match sender.try_send(s) {
                        Ok(_) => {
                            // 切换到下一个 buffer（刚发送的那个被 channel 持有）
                            buf_idx = (buf_idx + 1) % buf_pool.len();
                            if frame_count % 100 == 1 {
                                debug_log_dev(&format!("bgm_loop[{}]: sent {} samples to channel", pid, send_len));
                            }
                            break;
                        }
                        Err(crossbeam_channel::TrySendError::Full(s)) => {
                            samples_to_send = Some(s);
                            if !running.load(Ordering::Acquire) {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        Err(crossbeam_channel::TrySendError::Disconnected(_)) => break,
                    }
                }
            }
            Err(e) => {
                consecutive_errors += 1;
                log::warn!("Failed to read BGM: {} (consecutive: {})", e, consecutive_errors);
                if consecutive_errors >= 10 {
                    log::warn!("BGM target process likely exited, stopping loopback for pid={}", pid);
                    debug_log_dev(&format!("bgm_loop[{}]: exiting after {} consecutive errors", pid, consecutive_errors));
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    client.stop_stream().ok();
    log::info!("BGM process loopback stopped");

    Ok(())
}
