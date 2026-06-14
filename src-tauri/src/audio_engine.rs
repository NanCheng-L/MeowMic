use crossbeam_channel::{bounded, Receiver, Sender};
use crate::denoise::{self, FRAME_SIZE};
use crate::eq::{EqConfig, EqProcessor};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use wasapi::*;

/// 写入调试日志到文件（打包后可用）
fn debug_log(msg: &str) {
    use std::io::Write;
    let log_path = std::env::temp_dir().join("meowmic-debug.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        let elapsed = std::time::Instant::now().elapsed();
        let _ = writeln!(f, "[{:?}] {}", elapsed, msg);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenoiseConfig {
    pub enabled: bool,
    pub strength: f32,
    pub suppress_level: f32,
    pub mic_gain: f32,
}

impl Default for DenoiseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: 0.5,
            suppress_level: 0.5,
            mic_gain: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BgmConfig {
    pub process_names: Vec<String>,
    pub process_pids: Vec<u32>,
    pub bgm_gain: f32,
    pub enabled: bool,
}

impl Default for BgmConfig {
    fn default() -> Self {
        Self {
            process_names: Vec::new(),
            process_pids: Vec::new(),
            bgm_gain: 1.0,
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioStats {
    pub input_level: f32,
    pub output_level: f32,
    pub noise_reduction_db: f32,
    pub latency_ms: f32,
    pub cpu_usage: f32,
    pub frames_processed: u64,
    pub spectrum: Vec<f32>,
}

pub struct AudioEngine {
    running: Arc<AtomicBool>,
    config: Arc<RwLock<DenoiseConfig>>,
    eq_config: Arc<RwLock<EqConfig>>,
    stats: Arc<RwLock<AudioStats>>,
    bgm_running: Arc<AtomicBool>,
    bgm_config: Arc<RwLock<BgmConfig>>,
    bgm_sender: Sender<Vec<i16>>,
    bgm_receiver: Receiver<Vec<i16>>,
    explode_enabled: Arc<AtomicBool>,
    explode_intensity: Arc<AtomicU32>,
    monitor_enabled: Arc<AtomicBool>,
    thread_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    bgm_thread_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    /// 模型内部状态（归一化统计等），按模型名分别保存
    model_states: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (bgm_tx, bgm_rx) = bounded::<Vec<i16>>(2);
        Self {
            running: Arc::new(AtomicBool::new(false)),
            config: Arc::new(RwLock::new(DenoiseConfig::default())),
            eq_config: Arc::new(RwLock::new(EqConfig::default())),
            stats: Arc::new(RwLock::new(AudioStats::default())),
            bgm_running: Arc::new(AtomicBool::new(false)),
            bgm_config: Arc::new(RwLock::new(BgmConfig::default())),
            bgm_sender: bgm_tx,
            bgm_receiver: bgm_rx,
            explode_enabled: Arc::new(AtomicBool::new(false)),
            explode_intensity: Arc::new(AtomicU32::new(50)),
            monitor_enabled: Arc::new(AtomicBool::new(false)),
            thread_handle: std::sync::Mutex::new(None),
            bgm_thread_handle: std::sync::Mutex::new(None),
            model_states: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn start(
        &self,
        input_device_name: Option<String>,
        output_device_name: Option<String>,
        model_name: Option<String>,
        resource_dir: Option<std::path::PathBuf>,
        monitor_enabled_init: bool,
    ) -> Result<(), String> {
        if self.running.load(Ordering::Relaxed) {
            return Err("Engine is already running".to_string());
        }

        // 模型在音频线程内加载（避免阻塞主线程）
        let mn = model_name.clone();
        let rd = resource_dir.clone();

        let running = self.running.clone();
        let config = self.config.clone();
        let eq_config = self.eq_config.clone();
        let stats = self.stats.clone();
        let bgm_config = self.bgm_config.clone();
        let bgm_running = self.bgm_running.clone();
        let bgm_receiver = self.bgm_receiver.clone();
        let explode_enabled = self.explode_enabled.clone();
        let explode_intensity = self.explode_intensity.clone();
        let monitor_enabled = self.monitor_enabled.clone();
        let model_states = self.model_states.clone();

        // 同步初始监听状态
        self.monitor_enabled.store(monitor_enabled_init, Ordering::Relaxed);

        running.store(true, Ordering::Relaxed);

        let handle = std::thread::spawn(move || {
            if let Err(e) = audio_loop(
                running,
                config,
                eq_config,
                stats,
                bgm_running,
                bgm_config,
                Some(bgm_receiver),
                input_device_name,
                output_device_name,
                mn,
                rd,
                explode_enabled,
                explode_intensity,
                monitor_enabled,
                model_states,
            ) {
                log::error!("Audio engine error: {}", e);
            }
        });

        // 存储线程句柄，stop() 时等待退出
        *self.thread_handle.lock().unwrap() = Some(handle);

        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        self.stop_bgm();
        // 等待音频线程退出，避免旧流残留导致回音
        if let Some(handle) = self.thread_handle.lock().unwrap().take() {
            let _ = handle.join();
        }
    }

    pub fn update_config(&self, config: DenoiseConfig) {
        *self.config.write() = config;
    }

    pub fn get_config(&self) -> DenoiseConfig {
        self.config.read().clone()
    }

    pub fn get_stats(&self) -> AudioStats {
        self.stats.read().clone()
    }

    /// 列出正在播放音频的进程（用于 BGM Loopback）
    pub fn list_audio_processes(&self) -> Result<Vec<(String, String, u32)>, String> {
        list_audio_processes()
    }

    /// 启动 BGM 捕获线程（按进程 PID 列表，每个 PID 一个线程）
    pub fn start_bgm(&self, pids: Vec<u32>) -> Result<(), String> {
        // 如果旧线程还在运行，先停止并等待退出
        self.stop_bgm();

        let sender = self.bgm_sender.clone();

        // 更新配置
        {
            let mut cfg = self.bgm_config.write();
            cfg.process_pids = pids.clone();
            cfg.enabled = true;
        }

        let bgm_running = self.bgm_running.clone();
        bgm_running.store(true, Ordering::Relaxed);

        // 为每个 PID 启动一个独立的 loopback 线程
        let mut handles = Vec::new();
        for pid in pids {
            let sender_clone = sender.clone();
            let running_clone = bgm_running.clone();
            let handle = std::thread::Builder::new()
                .name(format!("bgm-loopback-{}", pid))
                .spawn(move || {
                    if let Err(e) = bgm_process_loop(running_clone, sender_clone, pid) {
                        log::error!("BGM process loopback error for pid {}: {}", pid, e);
                    }
                })
                .map_err(|e| format!("Failed to spawn BGM thread: {}", e))?;
            handles.push(handle);
        }

        // 存储所有线程句柄，stop_bgm() 时 join
        let manager_handle = std::thread::Builder::new()
            .name("bgm-manager".into())
            .spawn(move || {
                for handle in handles {
                    let _ = handle.join();
                }
            })
            .map_err(|e| format!("Failed to spawn BGM manager: {}", e))?;

        *self.bgm_thread_handle.lock().unwrap() = Some(manager_handle);

        Ok(())
    }

    /// 停止 BGM 捕获
    pub fn stop_bgm(&self) {
        self.bgm_running.store(false, Ordering::Relaxed);
        let mut cfg = self.bgm_config.write();
        cfg.enabled = false;
        drop(cfg);
        // 等待 BGM 线程退出，清空残留数据
        if let Some(handle) = self.bgm_thread_handle.lock().unwrap().take() {
            let _ = handle.join();
        }
        while self.bgm_receiver.try_recv().is_ok() {}
    }

    /// 更新 BGM 配置（增益等）
    pub fn update_bgm_config(&self, bgm_gain: f32) {
        let mut cfg = self.bgm_config.write();
        cfg.bgm_gain = bgm_gain.clamp(0.0, 10.0);
    }

    /// 设置爆炸模式
    pub fn set_explode_mode(&self, enabled: bool) {
        self.explode_enabled.store(enabled, Ordering::Relaxed);
    }

    /// 设置爆炸强度 (1-100)
    pub fn set_explode_intensity(&self, intensity: u32) {
        self.explode_intensity.store(intensity.clamp(1, 100), Ordering::Relaxed);
    }

    /// 设置监听模式
    pub fn set_monitor_enabled(&self, enabled: bool) {
        self.monitor_enabled.store(enabled, Ordering::Relaxed);
    }

    /// 更新 EQ 配置
    pub fn update_eq_config(&self, eq_config: EqConfig) {
        *self.eq_config.write() = eq_config;
    }

    /// 获取 EQ 配置
    pub fn get_eq_config(&self) -> EqConfig {
        self.eq_config.read().clone()
    }
}

/// 列出正在播放音频的进程（使用 IAudioSessionManager2 枚举所有活跃音频会话）
#[cfg(windows)]
fn list_audio_processes() -> Result<Vec<(String, String, u32)>, String> {
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
fn list_audio_processes() -> Result<Vec<(String, String, u32)>, String> {
    Ok(vec![])
}

/// BGM 按进程捕获线程：使用 WASAPI Process Loopback API
fn bgm_process_loop(
    running: Arc<AtomicBool>,
    sender: Sender<Vec<i16>>,
    pid: u32,
) -> Result<(), String> {
    let _ = initialize_mta().ok();

    let mut client = AudioClient::new_application_loopback_client(pid, true)
        .map_err(|e| format!("Failed to create process loopback client: {}", e))?;

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

    // 进程 loopback 不支持 get_periods()，用 0 让 WASAPI 用默认值
    client
        .initialize_client(&format, 0, &Direction::Capture, &ShareMode::Shared, true)
        .map_err(|e| format!("Failed to initialize BGM client: {}", e))?;

    let event_handle = client
        .set_get_eventhandle()
        .map_err(|e| format!("Failed to set BGM event handle: {}", e))?;

    let capture = client
        .get_audiocaptureclient()
        .map_err(|e| format!("Failed to get BGM capture client: {}", e))?;

    client
        .start_stream()
        .map_err(|e| format!("Failed to start BGM stream: {}", e))?;

    let frame_size = 480;
    let mut buffer = vec![0u8; frame_size * bytes_per_frame];

    log::info!("BGM process loopback started for pid={}", pid);

    let mut frame_count: u64 = 0;

    while running.load(Ordering::Relaxed) {
        if event_handle.wait_for_event(100).is_err() {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }

        match capture.read_from_device(&mut buffer) {
            Ok((frames_read, _flags)) => {
                if frames_read == 0 {
                    continue;
                }

                let bytes_read = frames_read as usize * bytes_per_frame;

                let samples: Vec<i16> = if is_float && bytes_per_sample == 4 {
                    buffer[..bytes_read]
                        .chunks_exact(4)
                        .map(|chunk| {
                            let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                            (val.clamp(-1.0, 1.0) * 32767.0) as i16
                        })
                        .collect()
                } else if bytes_per_sample == 4 {
                    buffer[..bytes_read]
                        .chunks_exact(4)
                        .map(|chunk| {
                            let val = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                            (val >> 16) as i16
                        })
                        .collect()
                } else if bytes_per_sample == 2 {
                    buffer[..bytes_read]
                        .chunks_exact(2)
                        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                        .collect()
                } else {
                    vec![0; frames_read as usize]
                };

                let stereo_samples: Vec<i16> = if channels == 1 {
                    samples
                } else {
                    samples
                        .chunks(channels)
                        .flat_map(|frame| {
                            let l = frame.first().copied().unwrap_or(0);
                            let r = frame.get(1).copied().unwrap_or(l);
                            vec![l, r]
                        })
                        .collect()
                };

                frame_count += 1;
                if frame_count % 100 == 1 {
                    log::info!("BGM frame {} ok, {} bytes", frame_count, stereo_samples.len());
                }

                if sender.send(stereo_samples).is_err() {
                    break;
                }
            }
            Err(e) => {
                log::warn!("Failed to read BGM: {}", e);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    client.stop_stream().ok();
    log::info!("BGM process loopback stopped");

    Ok(())
}

/// 主音频循环：麦克风采集 → 降噪 → EQ → 混音（可选 BGM）→ 输出
fn audio_loop(
    running: Arc<AtomicBool>,
    config: Arc<RwLock<DenoiseConfig>>,
    eq_config: Arc<RwLock<EqConfig>>,
    stats: Arc<RwLock<AudioStats>>,
    bgm_running: Arc<AtomicBool>,
    bgm_config: Arc<RwLock<BgmConfig>>,
    bgm_receiver: Option<Receiver<Vec<i16>>>,
    input_device_name: Option<String>,
    output_device_name: Option<String>,
    model_name: Option<String>,
    resource_dir: Option<std::path::PathBuf>,
    explode_enabled: Arc<AtomicBool>,
    explode_intensity: Arc<AtomicU32>,
    monitor_enabled: Arc<AtomicBool>,
    saved_model_states: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
) -> Result<(), String> {
    let _ = initialize_mta().ok();
    debug_log("=== audio_loop started ===");

    let input_device = find_device(input_device_name.as_deref(), true)
        .map_err(|e| format!("Failed to find input device: {}", e))?;
    let output_device = find_device(output_device_name.as_deref(), false)
        .map_err(|e| format!("Failed to find output device: {}", e))?;

    let input_friendly = input_device.get_friendlyname().unwrap_or_else(|_| "unknown".into());
    let output_friendly = output_device.get_friendlyname().unwrap_or_else(|_| "unknown".into());
    debug_log(&format!("Input device: '{}'", input_friendly));
    debug_log(&format!("Output device: '{}'", output_friendly));
    log::info!("Input device: '{}' (requested: {:?})", input_friendly, input_device_name);
    log::info!("Output device: '{}' (requested: {:?})", output_friendly, output_device_name);

    // 获取设备原生格式，避免 WASAPI 内部重采样增加延迟
    let fallback_input_format = WaveFormat::new(16, 16, &SampleType::Int, 48000, 1, None);
    let fallback_output_format = WaveFormat::new(16, 16, &SampleType::Int, 48000, 2, None);

    let mut input_client = input_device
        .get_iaudioclient()
        .map_err(|e| format!("Failed to get input client: {}", e))?;

    let input_format = input_client.get_mixformat().unwrap_or_else(|_| {
        log::warn!("Failed to get input mixformat, using fallback 48kHz");
        fallback_input_format
    });
    let input_sample_rate = input_format.get_samplespersec();
    let input_channels = input_format.get_nchannels() as usize;
    let input_bits = input_format.get_bitspersample();
    let input_sample_type = input_format.get_subformat().unwrap_or(SampleType::Int);
    log::info!("Input device format: {}Hz, {}ch, {}bit, {:?}",
        input_sample_rate, input_channels, input_bits, input_sample_type);

    let (def_time, _min_time) = input_client
        .get_periods()
        .map_err(|e| format!("Failed to get input periods: {}", e))?;

    input_client
        .initialize_client(
            &input_format,
            def_time,
            &Direction::Capture,
            &ShareMode::Shared,
            true,
        )
        .map_err(|e| format!("Failed to initialize input client: {}", e))?;

    let input_handle = input_client
        .set_get_eventhandle()
        .map_err(|e| format!("Failed to set input event handle: {}", e))?;

    let mut output_client = output_device
        .get_iaudioclient()
        .map_err(|e| format!("Failed to get output client: {}", e))?;

    let output_format = output_client.get_mixformat().unwrap_or_else(|_| {
        log::warn!("Failed to get output mixformat, using fallback 48kHz");
        fallback_output_format
    });
    let output_sample_rate = output_format.get_samplespersec();
    let output_channels = output_format.get_nchannels() as usize;
    log::info!("Output device format: {}Hz, {}ch, {}bit",
        output_sample_rate, output_channels,
        output_format.get_bitspersample()
    );

    let (def_time, _min_time) = output_client
        .get_periods()
        .map_err(|e| format!("Failed to get output periods: {}", e))?;

    output_client
        .initialize_client(
            &output_format,
            def_time,
            &Direction::Render,
            &ShareMode::Shared,
            true,
        )
        .map_err(|e| format!("Failed to initialize output client: {}", e))?;
    log::info!("Output client initialized on '{}'", output_friendly);

    let _output_handle = output_client
        .set_get_eventhandle()
        .map_err(|e| format!("Failed to set output event handle: {}", e))?;

    let input_capture = input_client
        .get_audiocaptureclient()
        .map_err(|e| format!("Failed to get input capture client: {}", e))?;

    let output_render = output_client
        .get_audiorenderclient()
        .map_err(|e| format!("Failed to get output render client: {}", e))?;

    let frame_size = FRAME_SIZE;

    // ====== 监听客户端：系统默认输出设备（耳机/扬声器），用于实时监听降噪效果 ======

    let mut monitor_client_opt: Option<AudioClient> = None;
    let mut monitor_render_opt: Option<AudioRenderClient> = None;
    let mut monitor_event_opt: Option<wasapi::Handle> = None;
    let monitor_format = WaveFormat::new(16, 16, &SampleType::Int, output_sample_rate as usize, 2, None);
    // 监听缓冲区：考虑重采样后可能变大（最高 192kHz → 4x）
    let monitor_max_frames = frame_size * (output_sample_rate as usize / 48000 + 1);
    let mut monitor_buffer = vec![0u8; monitor_max_frames * 2 * 2]; // stereo i16

    // 监听使用系统默认输出设备（和 OBS 一样），不需要用户手动选择
    if let Ok(monitor_output) = find_device(None, false) {
        let monitor_output_name = monitor_output.get_friendlyname().unwrap_or_default();
        if let Ok(mut m_client) = monitor_output.get_iaudioclient() {
            let (def_time, _) = m_client.get_periods().unwrap_or((0, 0));
            if m_client.initialize_client(
                &monitor_format,
                def_time,
                &Direction::Render,
                &ShareMode::Shared,
                true,
            ).is_ok()
            {
                if let Ok(render) = m_client.get_audiorenderclient() {
                    let evt = m_client.set_get_eventhandle();
                    if m_client.start_stream().is_ok() {
                        log::info!("Monitor started on default output: {} (format: 16bit int, {}Hz)", monitor_output_name, output_sample_rate);
                        monitor_render_opt = Some(render);
                        monitor_event_opt = evt.ok();
                        monitor_client_opt = Some(m_client);
                    }
                }
            }
        }
    }

    // ====== 模型加载（在音频流启动之前，避免阻塞 WASAPI 导致炸麦）======
    let load_start = std::time::Instant::now();
    let mut denoise = denoise::create_model(model_name.as_deref().unwrap_or("RNNoise"), resource_dir.as_deref());
    let load_elapsed = load_start.elapsed().as_millis();
    log::info!("Model '{}' loaded in {}ms", denoise.name(), load_elapsed);

    // 恢复模型状态
    let current_model_name = denoise.name().to_string();
    if let Ok(states_lock) = saved_model_states.lock() {
        if let Some(state) = states_lock.get(&current_model_name) {
            log::info!("Restoring model '{}' state ({} bytes)", current_model_name, state.len());
            denoise.load_state(state);
        }
    }
    log::info!("Using denoise model: {}", current_model_name);

    // ====== EQ 处理器初始化 ======
    let mut eq_processor = EqProcessor::new(48000);
    let current_eq_config = eq_config.read().clone();
    if current_eq_config.enabled {
        eq_processor.apply_config(&current_eq_config);
        log::info!("EQ enabled with preset bands");
    }

    input_client
        .start_stream()
        .map_err(|e| format!("Failed to start input: {}", e))?;
    output_client
        .start_stream()
        .map_err(|e| format!("Failed to start output: {}", e))?;

    let mut first_frame = true;
    // 输入缓冲区：按设备实际 bytes_per_frame 分配
    let input_bytes_per_frame = input_format.get_blockalign() as usize;
    let output_bytes_per_frame = output_format.get_blockalign() as usize;

    // ====== WASAPI 流启动预热：等待设备真正就绪 ======
    // 首次启动时 WASAPI 设备需要时间初始化，可能读到空数据
    // 如果预热超时，重启流重试
    debug_log("Starting warmup...");
    let warmup_buf_size = frame_size * input_bytes_per_frame;
    let mut warmup_buf = vec![0u8; warmup_buf_size];
    let max_retries = 3;
    let mut warmup_ok = false;

    for attempt in 0..max_retries {
        debug_log(&format!("Warmup attempt {}/{}", attempt + 1, max_retries));
        let warmup_start = std::time::Instant::now();
        let mut warmup_frames = 0;
        let mut got_signal = false;

        while warmup_start.elapsed().as_millis() < 300 {
            if input_handle.wait_for_event(50).is_ok() {
                if let Ok((frames_read, _)) = input_capture.read_from_device(&mut warmup_buf) {
                    if frames_read > 0 {
                        warmup_frames += 1;
                        let bytes_read = frames_read as usize * input_bytes_per_frame;
                        if warmup_buf[..bytes_read].iter().any(|&b| b != 0) {
                            got_signal = true;
                            break;
                        }
                    }
                }
            }
        }

        if got_signal {
            debug_log(&format!("Warmup OK: {} frames, {}ms", warmup_frames, warmup_start.elapsed().as_millis()));
            warmup_ok = true;
            break;
        }

        debug_log(&format!("Warmup attempt {} failed, restarting streams...", attempt + 1));
        // 重启流
        let _ = input_client.stop_stream();
        let _ = output_client.stop_stream();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = input_client.start_stream();
        let _ = output_client.start_stream();
    }

    if !warmup_ok {
        debug_log("All warmup attempts failed!");
    }
    debug_log("Entering main audio loop");

    let mut input_buffer = vec![0u8; frame_size * input_bytes_per_frame];
    let mut output_buffer = vec![0u8; frame_size * output_bytes_per_frame];
    let mut frame_count: u64 = 0;

    // 输入累积缓冲：非 48kHz 设备需要重采样，这里累积 mono f32 样本
    let mut input_acc: Vec<f32> = Vec::new();

    // BGM 缓冲：立体声 i16 样本队列
    let mut bgm_buf: Vec<i16> = Vec::new();
    let mut monitor_was_streaming = monitor_enabled.load(Ordering::Relaxed);

    let mut loop_iteration: u64 = 0;
    let mut consecutive_zero_reads: u32 = 0;
    while running.load(Ordering::Relaxed) {
        if input_handle.wait_for_event(100).is_err() {
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        }

        loop_iteration += 1;
        let current_config = config.read().clone();

        match input_capture.read_from_device(&mut input_buffer) {
            Ok((frames_read, _flags)) => {
                if frames_read == 0 {
                    consecutive_zero_reads += 1;
                    if consecutive_zero_reads >= 100 {
                        // 连续 100 次读取失败，可能是同一 USB 设备导致的死锁
                        debug_log(&format!("FATAL: {} consecutive zero reads, likely same-device conflict", consecutive_zero_reads));
                        return Err("AUDIO_DEVICE_CONFLICT: 输入输出可能是同一设备，请更换输出设备".to_string());
                    }
                    if loop_iteration <= 20 {
                        debug_log(&format!("Loop iter {}: frames_read=0", loop_iteration));
                    }
                    continue;
                }
                consecutive_zero_reads = 0; // 重置计数器

                // 前几次迭代记录详细信息
                if loop_iteration <= 5 {
                    let bytes_read_dbg = frames_read as usize * input_bytes_per_frame;
                    let has_signal = input_buffer[..bytes_read_dbg].iter().any(|&b| b != 0);
                    debug_log(&format!("Loop iter {}: frames_read={}, has_signal={}", loop_iteration, frames_read, has_signal));
                }

                // 1. 原始字节 → f32 样本（根据设备实际格式）
                let bytes_read = frames_read as usize * input_bytes_per_frame;
                let raw_samples = bytes_to_f32_samples(
                    &input_buffer[..bytes_read],
                    input_bits,
                    &input_sample_type,
                    input_channels,
                );

                // 2. 多声道 → 单声道
                let mono_samples = downmix_to_mono(&raw_samples, input_channels);

                // 3. 重采样到 48kHz（如果设备采样率不是 48kHz）
                let resampled = if input_sample_rate != 48000 {
                    resample_linear(&mono_samples, input_sample_rate, 48000)
                } else {
                    mono_samples
                };

                // 4. 累积到输入缓冲
                input_acc.extend_from_slice(&resampled);

                // 5. 每攒够 frame_size (480) 个样本就处理一帧
                while input_acc.len() >= frame_size {
                    let chunk: Vec<f32> = input_acc.drain(..frame_size).collect();

                    let mut input_frame = [0.0f32; 480];
                    for (i, &s) in chunk.iter().enumerate().take(frame_size) {
                        input_frame[i] = s;
                    }

                    let mut output_frame = input_frame;
                    if current_config.enabled {
                        denoise.process_frame(&mut output_frame, &input_frame);
                    }

                    // Skip first frame (fade-in artifact)
                    let output_samples = if first_frame {
                        first_frame = false;
                        chunk.clone()
                    } else if current_config.enabled {
                        output_frame.to_vec()
                    } else {
                        chunk.clone()
                    };

                    // Apply strength mixing
                    let mixed_samples: Vec<f32> =
                        if current_config.enabled && current_config.strength < 1.0 {
                            output_frame
                                .iter()
                                .zip(chunk.iter())
                                .map(|(&denoised, &original)| {
                                    original * (1.0 - current_config.strength)
                                        + denoised * current_config.strength
                                })
                                .collect()
                        } else {
                            output_samples
                        };

                    // 应用麦克风增益（在降噪后，避免放大噪音）
                    let mic_gain = current_config.mic_gain;
                    let mixed_samples: Vec<f32> = mixed_samples.iter().map(|s| s * mic_gain).collect();

                    // ============ EQ 均衡器 ============
                    let current_eq = eq_config.read().clone();
                    let mixed_samples = if current_eq.enabled {
                        let mut frame = mixed_samples;
                        eq_processor.apply_config(&current_eq);
                        eq_processor.process_frame(&mut frame);
                        frame
                    } else {
                        mixed_samples
                    };

                    // ============ 💥 爆炸模式：方波失真 ============
                    let mixed_samples = if explode_enabled.load(Ordering::Relaxed) {
                        let intensity = explode_intensity.load(Ordering::Relaxed) as f32;
                        // 映射 0-100 → 50-100，0% 时就有明显效果
                        let mapped = 50.0 + intensity * 0.5;
                        let gain = 1.0 + (mapped / 100.0).powf(0.6) * 49.0;
                        let clip = 32000.0 - (mapped / 100.0).powf(1.5) * 31800.0;
                        mixed_samples
                            .iter()
                            .map(|&s| {
                                let boosted = s * gain;
                                let clipped = boosted.clamp(-clip, clip);
                                clipped / clip * 32767.0
                            })
                            .collect()
                    } else {
                        mixed_samples
                    };

                    // ============ 监听输出 ============
                    let monitor_wants = monitor_enabled.load(Ordering::Relaxed);
                    if monitor_wants {
                        // 从关闭切换到开启：重新启动流
                        if !monitor_was_streaming {
                            if let Some(ref mut m_client) = monitor_client_opt {
                                let _ = m_client.start_stream();
                            }
                            monitor_was_streaming = true;
                        }
                        if let Some(ref monitor_render) = monitor_render_opt {
                            let monitor_ready = if let Some(ref evt) = monitor_event_opt {
                                evt.wait_for_event(10).is_ok()
                            } else {
                                true
                            };
                            if monitor_ready {
                                // 重采样到监听设备采样率
                                let monitor_resampled = if output_sample_rate != 48000 {
                                    resample_linear(&mixed_samples, 48000, output_sample_rate)
                                } else {
                                    mixed_samples.clone()
                                };
                                let monitor_stereo = upmix_to_stereo(&monitor_resampled, 2);
                                // monitor_stereo 已是交错立体声 [L0,R0,L1,R1,...]，直接写入
                                for (i, &sample) in monitor_stereo.iter().enumerate() {
                                    let val = (sample.clamp(-32768.0, 32767.0)) as i16;
                                    let bytes = val.to_le_bytes();
                                    monitor_buffer[i * 2] = bytes[0];
                                    monitor_buffer[i * 2 + 1] = bytes[1];
                                }
                                let monitor_frames = monitor_resampled.len();
                                let _ = monitor_render.write_to_device(
                                    monitor_frames,
                                    &monitor_buffer[..monitor_frames * 4],
                                    None,
                                );
                            }
                        }
                    } else if monitor_was_streaming {
                        // 从开启切换到关闭：停止流 + 写入静音，立即静音
                        if let Some(ref mut m_client) = monitor_client_opt {
                            let _ = m_client.stop_stream();
                        }
                        monitor_was_streaming = false;
                    }

                    // ============ BGM 混音 ============
                    let final_samples = if bgm_running.load(Ordering::Relaxed) {
                        if let Some(ref rx) = bgm_receiver {
                            while let Ok(bgm_samples) = rx.try_recv() {
                                bgm_buf.extend_from_slice(&bgm_samples);
                            }
                        }
                        let max_buf = frame_size * 2 * 5;
                        if bgm_buf.len() > max_buf {
                            let drain = bgm_buf.len() - max_buf;
                            bgm_buf.drain(..drain);
                        }

                        let bgm_gain = bgm_config.read().bgm_gain;

                        mixed_samples
                            .iter()
                            .enumerate()
                            .map(|(i, &mic_sample)| {
                                if bgm_buf.is_empty() {
                                    return mic_sample;
                                }
                                let bgm_idx = i * 2;
                                let bgm_l = if bgm_idx < bgm_buf.len() {
                                    bgm_buf[bgm_idx] as f32
                                } else {
                                    0.0
                                };
                                let bgm_r = if bgm_idx + 1 < bgm_buf.len() {
                                    bgm_buf[bgm_idx + 1] as f32
                                } else {
                                    0.0
                                };
                                let bgm_mono = (bgm_l + bgm_r) / 2.0;
                                mic_sample + bgm_mono * bgm_gain
                            })
                            .collect()
                    } else {
                        mixed_samples
                    };

                    // 消费已使用的 BGM 样本
                    let used = final_samples.len() * 2;
                    if used <= bgm_buf.len() {
                        bgm_buf.drain(..used);
                    } else {
                        bgm_buf.clear();
                    }

                    // ============ Soft limiter ============
                    let mut limited_samples = final_samples;
                    if !explode_enabled.load(Ordering::Relaxed) {
                        for sample in limited_samples.iter_mut() {
                            let level = sample.abs();
                            if level > 28000.0 {
                                let excess = level - 28000.0;
                                let compressed = 28000.0 + excess * 0.2;
                                let sign = if *sample > 0.0 { 1.0 } else { -1.0 };
                                *sample = sign * compressed.min(32767.0);
                            }
                        }
                    }

                    // 6. 重采样回设备采样率
                    let resampled_out = if output_sample_rate != 48000 {
                        resample_linear(&limited_samples, 48000, output_sample_rate)
                    } else {
                        limited_samples.clone()
                    };

                    // 7. 单声道 → 多声道
                    let stereo_out = upmix_to_stereo(&resampled_out, output_channels);

                    // 8. f32 → 设备格式字节
                    let out_frames = stereo_out.len() / output_channels;
                    let out_bytes = out_frames * output_bytes_per_frame;
                    for (i, &sample) in stereo_out.iter().enumerate() {
                        let byte_pos = i * (output_bytes_per_frame / output_channels);
                        match (input_bits, &input_sample_type) {
                            (16, SampleType::Int) | (16, _) => {
                                let val = (sample.clamp(-32768.0, 32767.0)) as i16;
                                let bytes = val.to_le_bytes();
                                if byte_pos + 1 < output_buffer.len() {
                                    output_buffer[byte_pos] = bytes[0];
                                    output_buffer[byte_pos + 1] = bytes[1];
                                }
                            }
                            (32, SampleType::Float) => {
                                let val = (sample / 32767.0).clamp(-1.0, 1.0);
                                let bytes = val.to_le_bytes();
                                if byte_pos + 3 < output_buffer.len() {
                                    output_buffer[byte_pos] = bytes[0];
                                    output_buffer[byte_pos + 1] = bytes[1];
                                    output_buffer[byte_pos + 2] = bytes[2];
                                    output_buffer[byte_pos + 3] = bytes[3];
                                }
                            }
                            _ => {
                                let val = (sample.clamp(-32768.0, 32767.0)) as i16;
                                let bytes = val.to_le_bytes();
                                if byte_pos + 1 < output_buffer.len() {
                                    output_buffer[byte_pos] = bytes[0];
                                    output_buffer[byte_pos + 1] = bytes[1];
                                }
                            }
                        }
                    }

                    // 检查输出缓冲区是否有空间，避免阻塞（同一 USB 设备时钟同步会死锁）
                    // 共享模式下缓冲区通常是 10ms，留 2 帧余量
                    let output_padding = output_client.get_current_padding().unwrap_or(0);
                    let max_padding = (output_sample_rate / 100) * 2; // 20ms 余量
                    if output_padding < max_padding {
                        if let Err(e) = output_render.write_to_device(
                            out_frames,
                            &output_buffer[..out_bytes],
                            None,
                        ) {
                            log::warn!("Output write error: {:?}", e);
                        }
                    }

                    frame_count += 1;

                    if frame_count % 5 == 0 {
                        let input_rms = calculate_rms(&input_frame);
                        let output_rms = calculate_rms(&limited_samples);
                        let input_level_db = if input_rms > 0.0 {
                            20.0 * (input_rms / 32768.0).log10()
                        } else {
                            -100.0
                        };
                        let output_level_db = if output_rms > 0.0 {
                            20.0 * (output_rms / 32768.0).log10()
                        } else {
                            -100.0
                        };

                        // 延迟测量：输入缓冲 + 输出缓冲的帧数 / 采样率
                        let input_padding = input_client.get_current_padding().unwrap_or(0);
                        let output_padding = output_client.get_current_padding().unwrap_or(0);
                        let input_latency = input_padding as f32 / input_sample_rate as f32 * 1000.0;
                        let output_latency = output_padding as f32 / output_sample_rate as f32 * 1000.0;

                        let mut current_stats = stats.write();
                        current_stats.input_level = input_level_db;
                        current_stats.output_level = output_level_db;
                        current_stats.noise_reduction_db = input_level_db - output_level_db;
                        current_stats.latency_ms = input_latency + output_latency;
                        current_stats.frames_processed = frame_count;
                        current_stats.spectrum = compute_spectrum(&input_frame, 32);
                    }
                } // end while input_acc.len() >= frame_size
            }
            Err(e) => {
                log::warn!("Failed to read input: {}", e);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    // 保存该模型的状态（归一化统计），供下次启动时恢复
    let model_name = denoise.name().to_string();
    if let Some(state) = denoise.save_state() {
        log::info!("Saving model '{}' state ({} bytes)", model_name, state.len());
        if let Ok(mut states_lock) = saved_model_states.lock() {
            states_lock.insert(model_name, state);
        }
    }

    input_client.stop_stream().ok();
    output_client.stop_stream().ok();
    if let Some(m_client) = monitor_client_opt {
        m_client.stop_stream().ok();
    }

    Ok(())
}

fn find_device(name: Option<&str>, input: bool) -> Result<Device, String> {
    let direction = if input {
        Direction::Capture
    } else {
        Direction::Render
    };
    let collection = DeviceCollection::new(&direction)
        .map_err(|e| format!("Failed to get device collection: {}", e))?;

    if let Some(target_name) = name {
        collection
            .get_device_with_name(target_name)
            .map_err(|e| format!("Device '{}' not found: {}", target_name, e))
    } else {
        get_default_device(&direction).map_err(|e| format!("Failed to get default device: {}", e))
    }
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

fn compute_spectrum(samples: &[f32], bands: usize) -> Vec<f32> {
    let n = samples.len();
    if n == 0 {
        return vec![0.0; bands];
    }

    let mut spectrum = vec![0.0f32; bands];
    let band_size = n / bands;

    for i in 0..bands {
        let start = i * band_size;
        let end = (start + band_size).min(n);
        if start >= n {
            break;
        }
        let band_samples = &samples[start..end];
        let rms = calculate_rms(band_samples);
        spectrum[i] = rms / 32768.0;
    }

    for val in spectrum.iter_mut() {
        if *val > 0.001 {
            *val = (val.log10() + 3.0) / 3.0;
        }
        *val = val.clamp(0.0, 1.0);
    }

    spectrum
}

/// 线性插值重采样：将音频从 from_rate 重采样到 to_rate
fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (input.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        let frac = src_pos - src_idx as f64;
        let sample = if src_idx + 1 < input.len() {
            input[src_idx] * (1.0 - frac as f32) + input[src_idx + 1] * frac as f32
        } else {
            input[src_idx]
        };
        output.push(sample);
    }
    output
}

/// 将原始字节样本（i16 或 f32）转换为 f32 归一化值
fn bytes_to_f32_samples(buf: &[u8], bits: u16, sample_type: &SampleType, _channels: usize) -> Vec<f32> {
    match (bits, sample_type) {
        (16, SampleType::Int) => buf
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32)
            .collect(),
        (32, SampleType::Float) => buf
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) * 32767.0)
            .collect(),
        (32, SampleType::Int) => buf
            .chunks_exact(4)
            .map(|c| {
                let val = i32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                (val >> 16) as f32
            })
            .collect(),
        _ => buf
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32)
            .collect(),
    }
}

/// 多声道转单声道（取平均）
fn downmix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// 单声道转多声道（复制到每个声道）
fn upmix_to_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .iter()
        .flat_map(|&s| vec![s; channels])
        .collect()
}
