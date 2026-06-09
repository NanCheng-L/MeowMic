mod audio_engine;
mod denoise;
mod device_watcher;

use audio_engine::{AudioEngine, DenoiseConfig, AudioStats};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::str::FromStr;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
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
    autostart: bool,
    language: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+D".into(),
            hotkey_enabled: true,
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
        let mutex_name: Vec<u16> = OsStr::new("Global\\pico-denoise-single-instance")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let _mutex = CreateMutexW(None, true, windows::core::PCWSTR(mutex_name.as_ptr()));

        if GetLastError() == ERROR_ALREADY_EXISTS {
            let window_title: Vec<u16> = OsStr::new("pico-denoise")
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
) -> Result<(), String> {
    // 优先用 Tauri 资源目录（build 模式），fallback 到源码目录（dev 模式）
    let resource_dir = app.path().resource_dir().ok().map(|d| {
        let models_dir = d.join("models");
        if models_dir.join("enc.onnx").exists() {
            models_dir
        } else {
            // dev 模式：资源在 src-tauri/resources/models/
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("models")
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
) -> Result<(), String> {
    let engine = state.engine.lock();
    let mut config = DenoiseConfig::default();
    if let Some(e) = enabled {
        config.enabled = e;
    }
    if let Some(s) = strength {
        config.strength = s.clamp(0.0, 1.0);
    }
    engine.update_config(config);
    Ok(())
}

#[tauri::command]
fn get_audio_stats(state: State<'_, EngineState>) -> AudioStats {
    let engine = state.engine.lock();
    engine.get_stats()
}

#[tauri::command]
fn list_input_devices() -> Result<Vec<String>, String> {
    let _ = wasapi::initialize_mta().ok();
    let collection = wasapi::DeviceCollection::new(&wasapi::Direction::Capture)
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
fn list_output_devices() -> Result<Vec<String>, String> {
    let _ = wasapi::initialize_mta().ok();
    let collection = wasapi::DeviceCollection::new(&wasapi::Direction::Render)
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

    let setup_path = setup_path.ok_or("VB-Cable installer not found in resources")?;
    let setup_str = setup_path.to_str().ok_or("Invalid path")?;
    let work_dir = setup_path.parent().unwrap_or(std::path::Path::new("."));
    let work_str = work_dir.to_str().unwrap_or(".");

    log::info!("Installing VB-Cable from: {}", setup_str);

    // raw FFI: ShellExecuteW + "runas" 触发 UAC（UAC 显示 VB-Cable 安装程序）
    extern "system" {
        pub fn ShellExecuteW(
            hwnd: *mut core::ffi::c_void,
            lpoperation: *const u16,
            lpfile: *const u16,
            lpparameters: *const u16,
            lpdirectory: *const u16,
            nshowcmd: u32,
        ) -> *mut core::ffi::c_void;
    }

    let setup_wide: Vec<u16> = setup_str.encode_utf16().chain(std::iter::once(0)).collect();
    let runas_wide: Vec<u16> = "runas\0".encode_utf16().collect();
    let work_wide: Vec<u16> = work_str.encode_utf16().chain(std::iter::once(0)).collect();

    let result = unsafe {
        ShellExecuteW(
            core::ptr::null_mut(),
            runas_wide.as_ptr(),
            setup_wide.as_ptr(),
            core::ptr::null(),  // 不传 /SILENT，让用户在安装器中操作
            work_wide.as_ptr(),
            1, // SW_SHOWNORMAL
        )
    };

    if result as usize <= 32 {
        return Err(format!("Failed to launch installer (UAC denied?), code: {}", result as usize));
    }

    // 轮询检测 VB-Cable 是否出现（最多等 60 秒，用户需要手动点安装）
    let _ = wasapi::initialize_mta().ok();
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Ok(collection) = wasapi::DeviceCollection::new(&wasapi::Direction::Render) {
            let count = collection.get_nbr_devices().unwrap_or(0);
            for i in 0..count {
                if let Ok(device) = collection.get_device_at_index(i) {
                    if let Ok(name) = device.get_friendlyname() {
                        if name.to_lowercase().contains("cable") {
                            log::info!("VB-Cable installed and detected: {}", name);
                            return Ok("VB-Cable installed successfully".to_string());
                        }
                    }
                }
            }
        }
    }

    Err("timed out".to_string())
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
fn register_hotkey(app: AppHandle, hotkey: String) -> Result<(), String> {
    // 先注销旧的
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;

    let shortcut =
        Shortcut::from_str(&hotkey).map_err(|e| format!("快捷键格式错误: {}", e))?;

    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("toggle-denoise", ());
                }
            }
        })
        .map_err(|e| e.to_string())?;
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
fn start_bgm(state: State<'_, EngineState>, process_name: String, pid: u32) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.start_bgm(process_name, pid)
}

#[tauri::command]
fn stop_bgm(state: State<'_, EngineState>) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.stop_bgm();
    Ok(())
}

#[tauri::command]
fn update_bgm_config(state: State<'_, EngineState>, volume: f32) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.update_bgm_config(volume);
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
fn set_monitor_mode(state: State<'_, EngineState>, enabled: bool) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.set_monitor_enabled(enabled);
    Ok(())
}

#[tauri::command]
fn set_monitor_device(state: State<'_, EngineState>, device: Option<String>) -> Result<(), String> {
    let engine = state.engine.lock();
    engine.set_monitor_device(device);
    Ok(())
}

#[tauri::command]
fn list_denoise_models() -> Vec<&'static str> {
    denoise::list_models()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    // 单实例检查
    ensure_single_instance();

    let engine = Arc::new(Mutex::new(AudioEngine::new()));
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
            set_monitor_mode,
            set_monitor_device,
            list_denoise_models,
            install_vb_cable,
        ])
        .setup(move |app| {
            // 开机自启动时隐藏窗口，手动启动时显示
            if !is_hidden {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
            }

            let show = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
            let hide = MenuItemBuilder::with_id("hide", "隐藏窗口").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show)
                .item(&hide)
                .separator()
                .item(&quit)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("pico-denoise")
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
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
                    if s.hotkey_enabled {
                        if let Ok(shortcut) = Shortcut::from_str(&s.hotkey) {
                            let _ = app.global_shortcut().on_shortcut(
                                shortcut,
                                |app, _shortcut, event| {
                                    if event.state
                                        == tauri_plugin_global_shortcut::ShortcutState::Pressed
                                    {
                                        if let Some(window) = app.get_webview_window("main") {
                                            let _ = window.emit("toggle-denoise", ());
                                        }
                                    }
                                },
                            );
                        }
                    }
                }
            }

            // 启动设备热拔插检测
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                device_watcher::start_device_watcher(app_handle, 3);
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
