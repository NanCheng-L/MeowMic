mod audio_engine;
mod audio_init;
mod audio_process;
mod audio_utils;
mod bgm;
mod debug;
mod denoise;
mod device;
mod device_watcher;
mod eq;
mod explode;
mod mmcss;
mod wasapi_capture;
mod wasapi_render;

use audio_engine::{AudioEngine, AudioStats};
use debug::debug_log;
use eq::{EqConfig, EQ_FREQUENCIES, EQ_PRESET_NAMES};
use explode::ExplodeEffect;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::str::FromStr;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_store::StoreExt;

struct EngineState {
    engine: Arc<Mutex<AudioEngine>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    hotkey: String,
    hotkey_enabled: bool,
    hotkey_explode: String,
    hotkey_explode_enabled: bool,
    hotkey_monitor: String,
    hotkey_monitor_enabled: bool,
    hotkey_bgm: String,
    hotkey_bgm_enabled: bool,
    hotkey_eq: String,
    hotkey_eq_enabled: bool,
    autostart: bool,
    language: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+D".into(),
            hotkey_enabled: true,
            hotkey_explode: String::new(),
            hotkey_explode_enabled: false,
            hotkey_monitor: String::new(),
            hotkey_monitor_enabled: false,
            hotkey_bgm: String::new(),
            hotkey_bgm_enabled: false,
            hotkey_eq: String::new(),
            hotkey_eq_enabled: false,
            autostart: false,
            language: "zh-CN".into(),
        }
    }
}

/// 单实例检查：如果已有实例在运行，激活已有窗口并退出当前进程
#[cfg(windows)]
fn ensure_single_instance() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };

    unsafe {
        let mutex_name: Vec<u16> = OsStr::new("Global\\meowmic-single-instance")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let _mutex = CreateMutexW(None, true, windows::core::PCWSTR(mutex_name.as_ptr()));

        if GetLastError() == ERROR_ALREADY_EXISTS {
            let window_title: Vec<u16> = OsStr::new("MeowMic")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            if let Ok(hwnd) = FindWindowW(None, windows::core::PCWSTR(window_title.as_ptr())) {
                if !hwnd.0.is_null() {
                    if IsIconic(hwnd).as_bool() {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                    } else {
                        let _ = ShowWindow(hwnd, SW_SHOW);
                    }
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            std::process::exit(0);
        }
    }
}

#[cfg(not(windows))]
fn ensure_single_instance() {}

#[tauri::command]
fn start_denoising(
    state: State<'_, EngineState>,
    app: tauri::AppHandle,
    input_device: Option<String>,
    output_device: Option<String>,
    model: Option<String>,
    _monitor_enabled: Option<bool>,
) -> Result<(), String> {
    // 优先用 Tauri 资源目录（build 模式），fallback 到源码目录（dev 模式）
    // resource_dir 指向 resources/ 父目录（包含 models/、deepfilter/ 等子目录）
    let resource_dir = app.path().resource_dir().ok().map(|d| {
        let models_dir = d.join("models");
        if models_dir.join("denoise.onnx").exists() {
            d // build 模式：指向 resources/ 根
        } else {
            // dev 模式：资源在 src-tauri/resources/
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
        }
    });
    let engine = state.engine.lock();
    engine.start(input_device, output_device, model, resource_dir)
}

#[tauri::command]
fn stop_denoising(state: State<'_, EngineState>) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.stop();
    Ok(())
}

#[tauri::command]
fn update_denoise_config(
    state: State<'_, EngineState>,
    enabled: Option<bool>,
    strength: Option<f32>,
    mic_gain: Option<f32>,
) -> Result<(), String> {
    let engine = state.engine.lock();
    // 读取当前配置，只更新传入的字段
    let mut config = engine.get_config();
    if let Some(e) = enabled {
        config.enabled = e;
    }
    if let Some(s) = strength {
        config.strength = s.clamp(0.0, 1.0);
    }
    if let Some(g) = mic_gain {
        config.mic_gain = g.clamp(0.5, 10.0);
    }
    engine.update_config(config);
    Ok(())
}

#[tauri::command]
fn get_audio_stats(state: State<'_, EngineState>) -> AudioStats {
    let engine = state.engine.lock();
    let stats = engine.stats().read().clone();
    stats
}

#[tauri::command]
fn list_input_devices() -> Result<Vec<String>, String> {
    let _ = wasapi::initialize_mta().ok();
    let enumerator = wasapi::DeviceEnumerator::new()
        .map_err(|e| format!("Failed to create device enumerator: {}", e))?;
    let collection = enumerator
        .get_device_collection(&wasapi::Direction::Capture)
        .map_err(|e| format!("Failed to get device collection: {}", e))?;
    let mut devices = Vec::new();
    let count = collection.get_nbr_devices().unwrap_or(0);
    for i in 0..count {
        if let Ok(device) = collection.get_device_at_index(i) {
            if let Ok(name) = device.get_friendlyname() {
                // 过滤掉 VB-Cable Output，避免用户误选导致无声
                if !name.contains("CABLE Output") {
                    devices.push(name);
                }
            }
        }
    }
    Ok(devices)
}

#[tauri::command]
fn list_output_devices() -> Result<Vec<String>, String> {
    let _ = wasapi::initialize_mta().ok();
    let enumerator = wasapi::DeviceEnumerator::new()
        .map_err(|e| format!("Failed to create device enumerator: {}", e))?;
    let collection = enumerator
        .get_device_collection(&wasapi::Direction::Render)
        .map_err(|e| format!("Failed to get device collection: {}", e))?;
    let mut devices = Vec::new();
    let count = collection.get_nbr_devices().unwrap_or(0);
    for i in 0..count {
        if let Ok(device) = collection.get_device_at_index(i) {
            if let Ok(name) = device.get_friendlyname() {
                devices.push(name);
            }
        }
    }
    Ok(devices)
}

#[tauri::command]
async fn install_vb_cable(app: AppHandle) -> Result<String, String> {
    // 定位安装包
    let setup_path = if let Some(resource_dir) = app.path().resource_dir().ok() {
        let p = resource_dir.join("vb-cable").join("VBCABLE_Setup_x64.exe");
        if p.exists() { Some(p) } else { None }
    } else {
        None
    }.or_else(|| {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let p = std::path::PathBuf::from(manifest_dir).join("resources").join("vb-cable").join("VBCABLE_Setup_x64.exe");
        if p.exists() { Some(p) } else { None }
    });

    let setup_path = match setup_path {
        Some(p) => p,
        None => {
            // 兜底：从官网下载
            log::info!("VB-Cable installer not found locally, downloading...");
            let download_url = "https://vb-audio.com/Cable/VBCABLE_Setup_x64.exe";
            let tmp_dir = std::env::temp_dir().join("meowmic-vb-cable");
            let _ = std::fs::create_dir_all(&tmp_dir);
            let dest = tmp_dir.join("VBCABLE_Setup_x64.exe");

            let output = std::process::Command::new("curl.exe")
                .args(["-L", "-o", dest.to_str().unwrap(), download_url, "--progress-bar"])
                .output()
                .map_err(|e| format!("Failed to download VB-Cable: {}", e))?;

            if !output.status.success() {
                return Err("Failed to download VB-Cable installer".to_string());
            }

            if dest.exists() { dest } else {
                return Err("VB-Cable installer not found after download".to_string());
            }
        }
    };
    let setup_str = setup_path.to_str().ok_or("Invalid path")?;

    // 用 PowerShell 启动安装程序（自动提权）
    log::info!("Installing VB-Cable from: {}", setup_str);

    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args([
        "-WindowStyle", "Hidden",
        "-Command",
        &format!("Start-Process -FilePath '{}' -Verb RunAs", setup_str),
    ]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.spawn().map_err(|e| format!("Failed to run PowerShell: {}", e))?;

    Ok("VB-Cable installer launched".to_string())
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let settings = store.get("settings").and_then(|v| serde_json::from_value(v.clone()).ok());
    Ok(settings.unwrap_or_default())
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    store.set("settings", value);
    store.save().map_err(|e| e.to_string())?;

    // 更新自动启动
    let autostart = app.autolaunch();
    if settings.autostart {
        let _ = autostart.enable();
    } else {
        let _ = autostart.disable();
    }

    Ok(())
}

#[tauri::command]
fn register_hotkey(
    app: AppHandle,
    hotkey: String,
    hotkey_explode: String,
    hotkey_monitor: String,
    hotkey_bgm: String,
    hotkey_eq: String,
) -> Result<(), String> {
    // 先注销旧的
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;

    let shortcuts: Vec<(&str, &str)> = vec![
        (&hotkey, "toggle-denoise"),
        (&hotkey_explode, "toggle-explode"),
        (&hotkey_monitor, "toggle-monitor"),
        (&hotkey_bgm, "toggle-bgm"),
        (&hotkey_eq, "toggle-eq"),
    ]
    .iter()
    .filter(|(k, _)| !k.is_empty())
    .map(|(k, e)| (k.as_str(), *e))
    .collect();

    for (key, event) in shortcuts {
        let shortcut = match Shortcut::from_str(key) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Invalid hotkey '{}': {}", key, e);
                continue;
            }
        };
        let event_name = event.to_string();
        app.global_shortcut()
            .on_shortcut(shortcut, move |app, _shortcut, event| {
                if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit(&event_name, ());
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn unregister_hotkey(app: AppHandle) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn list_audio_processes(state: State<'_, EngineState>) -> Result<Vec<(String, String, u32)>, String> {
    let engine = state.engine.lock();
    engine.list_audio_processes()
}

#[tauri::command]
fn start_bgm(state: State<'_, EngineState>, pids: Vec<u32>) -> Result<(), String> {
    debug::debug_log("lib: start_bgm command received, locking engine...");
    let engine = state.engine.lock();
    debug::debug_log("lib: start_bgm engine locked, calling start_bgm...");
    let result = engine.start_bgm(pids[0]);
    debug::debug_log("lib: start_bgm done");
    result
}

#[tauri::command]
fn stop_bgm(state: State<'_, EngineState>) -> Result<(), String> {
    debug::debug_log("lib: stop_bgm command received, locking engine...");
    let engine = state.engine.lock();
    debug::debug_log("lib: stop_bgm engine locked, calling stop_bgm...");
    engine.stop_bgm();
    // 用户手动停止 BGM，取消引擎重启后的自动恢复
    engine.cancel_bgm_auto_restart();
    debug::debug_log("lib: stop_bgm done");
    Ok(())
}

#[tauri::command]
fn update_bgm_config(state: State<'_, EngineState>, bgm_gain: f32) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.update_bgm_config(bgm_gain);
    Ok(())
}

#[tauri::command]
fn set_explode_mode(state: State<'_, EngineState>, enabled: bool, intensity: Option<u32>) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.set_explode_mode(enabled);
    if let Some(i) = intensity {
        engine.set_explode_intensity(i);
    }
    Ok(())
}

#[tauri::command]
fn set_explode_effect(state: State<'_, EngineState>, effect: u32) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.set_explode_effect(ExplodeEffect::from_u32(effect));
    Ok(())
}

#[tauri::command]
fn set_monitor_mode(state: State<'_, EngineState>, enabled: bool) -> Result<(), String> {
    debug_log(&format!("set_monitor_mode: enabled={}", enabled));
    let engine = state.engine.lock();
    engine.set_monitor_enabled(enabled);
    Ok(())
}

#[tauri::command]
fn set_monitor_point(state: State<'_, EngineState>, point: u32) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.set_monitor_point(point);
    Ok(())
}

#[tauri::command]
fn update_eq_config(
    state: State<'_, EngineState>,
    enabled: Option<bool>,
    bands: Option<Vec<f32>>,
) -> Result<(), String> {
    let engine = state.engine.lock();
    let mut config = engine.get_eq_config();
    if let Some(e) = enabled {
        config.enabled = e;
    }
    if let Some(b) = bands {
        let mut arr = [0.0f32; 10];
        for (i, &v) in b.iter().enumerate().take(10) {
            arr[i] = v.clamp(-12.0, 12.0);
        }
        config.bands = arr;
    }
    engine.update_eq_config(config);
    Ok(())
}

#[tauri::command]
fn get_eq_config(state: State<'_, EngineState>) -> EqConfig {
    let engine = state.engine.lock();
    engine.get_eq_config()
}

#[tauri::command]
fn get_eq_presets() -> Vec<(String, Vec<f32>)> {
    EQ_PRESET_NAMES
        .iter()
        .map(|name| {
            let bands = eq::get_preset(name).to_vec();
            (name.to_string(), bands)
        })
        .collect()
}

#[tauri::command]
fn get_eq_frequencies() -> Vec<f32> {
    EQ_FREQUENCIES.to_vec()
}

#[tauri::command]
fn list_denoise_models() -> Vec<&'static str> {
    denoise::list_models()
}

pub fn run() {
    env_logger::init();

    // 单实例检查
    ensure_single_instance();

    let engine = Arc::new(Mutex::new(AudioEngine::new(None)));
    let is_hidden = std::env::args().any(|a| a == "--hidden");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(EngineState { engine })
        .invoke_handler(tauri::generate_handler![
            start_denoising,
            stop_denoising,
            update_denoise_config,
            get_audio_stats,
            list_input_devices,
            list_output_devices,
            list_audio_processes,
            get_settings,
            save_settings,
            register_hotkey,
            unregister_hotkey,
            start_bgm,
            stop_bgm,
            update_bgm_config,
            set_explode_mode,
            set_explode_effect,
            set_monitor_mode,
            set_monitor_point,
            update_eq_config,
            get_eq_config,
            get_eq_presets,
            get_eq_frequencies,
            list_denoise_models,
            install_vb_cable,
        ])
        .setup(move |app| {
            // 设置 AppHandle 到 AudioEngine，用于 emit 事件
            {
                let state = app.state::<EngineState>();
                let engine = state.engine.lock();
                engine.set_app_handle(app.handle().clone());
            }

            // 开机自启动时隐藏窗口，手动启动时显示
            if !is_hidden {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
            }

            // 提前读取语言设置，用于托盘菜单
            let tray_lang = app.store("settings.json").ok()
                .and_then(|store| store.get("settings"))
                .and_then(|v| serde_json::from_value::<AppSettings>(v.clone()).ok())
                .map(|s| s.language)
                .unwrap_or_else(|| "zh-CN".into());
            let (tray_show, tray_hide, tray_quit) = if tray_lang == "en" {
                ("Show Window".to_string(), "Hide Window".to_string(), "Quit".to_string())
            } else {
                ("显示窗口".to_string(), "隐藏窗口".to_string(), "退出".to_string())
            };

            let show = MenuItemBuilder::with_id("show", &tray_show).build(app)?;
            let hide = MenuItemBuilder::with_id("hide", &tray_hide).build(app)?;
            let quit = MenuItemBuilder::with_id("quit", &tray_quit).build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show)
                .item(&hide)
                .separator()
                .item(&quit)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("MeowMic")
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                    "quit" => {
                        std::process::exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // 启动时读取设置，注册全局快捷键 + 同步 autostart 状态
            let store = app.store("settings.json").ok();
            if let Some(store) = store {
                let settings: Option<AppSettings> = store
                    .get("settings")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                if let Some(s) = settings {
                    // 同步自动启动：防止 disable() 静默失败导致注册表残留
                    let autostart = app.autolaunch();
                    let autostart_enabled = autostart.is_enabled().unwrap_or(false);
                    if s.autostart && !autostart_enabled {
                        let _ = autostart.enable();
                    } else if !s.autostart && autostart_enabled {
                        let _ = autostart.disable();
                    }

                    // 注册全局快捷键
                    let hotkeys: Vec<(&str, bool, &str)> = vec![
                        (&s.hotkey, s.hotkey_enabled, "toggle-denoise"),
                        (&s.hotkey_explode, s.hotkey_explode_enabled, "toggle-explode"),
                        (&s.hotkey_monitor, s.hotkey_monitor_enabled, "toggle-monitor"),
                        (&s.hotkey_bgm, s.hotkey_bgm_enabled, "toggle-bgm"),
                        (&s.hotkey_eq, s.hotkey_eq_enabled, "toggle-eq"),
                    ];
                    for (key, enabled, event) in hotkeys {
                        if enabled && !key.is_empty() {
                            if let Ok(shortcut) = Shortcut::from_str(key) {
                                let event_name = event.to_string();
                                let _ = app.global_shortcut().on_shortcut(
                                    shortcut,
                                    move |app, _shortcut, event| {
                                        if event.state
                                            == tauri_plugin_global_shortcut::ShortcutState::Pressed
                                        {
                                            if let Some(window) = app.get_webview_window("main") {
                                                let _ = window.emit(&event_name, ());
                                            }
                                        }
                                    },
                                );
                            }
                        }
                    }
                }
            }

            // 启动设备热拔插检测
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                device_watcher::start_device_watcher(app_handle, 500);
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
