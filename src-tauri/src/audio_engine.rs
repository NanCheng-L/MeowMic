use crossbeam_channel::{bounded, Receiver, Sender};
use crate::denoise::{self, FRAME_SIZE};
use crate::eq::EqConfig;
use crate::explode::ExplodeState;
use crate::audio_init::init_monitor;
use crate::explode::ExplodeAudioState;
use crate::wasapi_capture::{self, Frame};
use crate::wasapi_render;
use parking_lot::RwLock;
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use crate::debug::{debug_log, debug_log_dev, flush_debug_log};
use crate::audio_utils::{calculate_rms, compute_spectrum_into, write_to_monitor};

/// Ring buffer 帧容量：8 帧 ≈ 80ms headroom（参考 noisegate）
const RING_FRAMES: usize = 8;

/// 监听点写入宏：检查条件后写入监听设备
macro_rules! monitor_write {
    ($monitor:expr, $target:expr, $current:expr, $enabled:expr, $samples:expr, $resample:expr) => {
        if $enabled && $current == $target && $monitor.was_streaming && $monitor.render.is_some() {
            write_to_monitor(
                $samples,
                &$monitor.render,
                &$monitor.event,
                &mut $monitor.buffer,
                $monitor.sample_rate,
                $resample,
            );
        }
    };
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
        Self { enabled: true, strength: 0.5, suppress_level: 0.5, mic_gain: 1.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BgmConfig {
    pub process_names: Vec<String>,
    pub process_pids: Vec<u32>,
}

impl Default for BgmConfig {
    fn default() -> Self {
        Self { process_names: Vec::new(), process_pids: Vec::new() }
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
    pub spectrum: [f32; 32],
    pub frames_dropped: u64,
}

pub type MonitorPoint = u32;

pub struct AudioEngine {
    running: Arc<AtomicBool>,
    config: Arc<RwLock<DenoiseConfig>>,
    eq_config: Arc<RwLock<EqConfig>>,
    eq_config_dirty: Arc<AtomicBool>,
    stats: Arc<RwLock<AudioStats>>,
    bgm_running: Arc<AtomicBool>,
    bgm_gain: Arc<AtomicU32>,
    bgm_skip_rate: Arc<AtomicU32>,
    #[allow(dead_code)]
    bgm_config: Arc<RwLock<BgmConfig>>,
    bgm_sender: Sender<Vec<i16>>,
    bgm_receiver: Receiver<Vec<i16>>,
    bgm_thread_running: parking_lot::Mutex<Arc<AtomicBool>>,
    bgm_was_active: Arc<AtomicBool>,
    explode_enabled: Arc<AtomicBool>,
    explode_state: Arc<ExplodeState>,
    monitor_enabled: Arc<AtomicBool>,
    monitor_point: Arc<AtomicU32>,
    thread_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    bgm_thread_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    model_states: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    /// 模型热切换 channel（发送端，传递已创建好的模型）
    model_switch_sender: Sender<(String, Box<dyn denoise::DenoiseModel>)>,
    /// 模型热切换 channel（接收端，传递给 DSP 线程）
    model_switch_receiver: Receiver<(String, Box<dyn denoise::DenoiseModel>)>,
    /// 资源目录路径（用于加载模型）
    resource_dir: std::sync::Mutex<Option<std::path::PathBuf>>,
    app_handle: std::sync::Mutex<Option<AppHandle>>,
    lifecycle_lock: std::sync::Mutex<()>,
}

impl AudioEngine {
    pub fn new(app_handle: Option<AppHandle>) -> Self {
        let (bgm_sender, bgm_receiver) = bounded::<Vec<i16>>(10);
        let (model_switch_sender, model_switch_receiver) = bounded::<(String, Box<dyn denoise::DenoiseModel>)>(1);
        Self {
            running: Arc::new(AtomicBool::new(false)),
            config: Arc::new(RwLock::new(DenoiseConfig::default())),
            eq_config: Arc::new(RwLock::new(EqConfig::default())),
            eq_config_dirty: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(RwLock::new(AudioStats::default())),
            bgm_running: Arc::new(AtomicBool::new(false)),
            bgm_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            bgm_skip_rate: Arc::new(AtomicU32::new(0)),
            bgm_config: Arc::new(RwLock::new(BgmConfig::default())),
            bgm_sender,
            bgm_receiver,
            bgm_thread_running: parking_lot::Mutex::new(Arc::new(AtomicBool::new(false))),
            bgm_was_active: Arc::new(AtomicBool::new(false)),
            explode_enabled: Arc::new(AtomicBool::new(false)),
            explode_state: Arc::new(ExplodeState::new()),
            monitor_enabled: Arc::new(AtomicBool::new(false)),
            monitor_point: Arc::new(AtomicU32::new(0)),
            thread_handle: std::sync::Mutex::new(None),
            bgm_thread_handle: std::sync::Mutex::new(None),
            model_states: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            model_switch_sender,
            model_switch_receiver,
            resource_dir: std::sync::Mutex::new(None),
            app_handle: std::sync::Mutex::new(app_handle),
            lifecycle_lock: std::sync::Mutex::new(()),
        }
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &Arc<RwLock<DenoiseConfig>> { &self.config }
    #[allow(dead_code)]
    pub fn eq_config(&self) -> &Arc<RwLock<EqConfig>> { &self.eq_config }
    #[allow(dead_code)]
    pub fn eq_config_dirty(&self) -> &Arc<AtomicBool> { &self.eq_config_dirty }
    pub fn stats(&self) -> &Arc<RwLock<AudioStats>> { &self.stats }
    #[allow(dead_code)]
    pub fn bgm_running(&self) -> &Arc<AtomicBool> { &self.bgm_running }
    #[allow(dead_code)]
    pub fn bgm_gain(&self) -> &Arc<AtomicU32> { &self.bgm_gain }
    #[allow(dead_code)]
    pub fn bgm_skip_rate(&self) -> &Arc<AtomicU32> { &self.bgm_skip_rate }
    #[allow(dead_code)]
    pub fn bgm_config(&self) -> &Arc<RwLock<BgmConfig>> { &self.bgm_config }
    #[allow(dead_code)]
    pub fn bgm_sender(&self) -> &Sender<Vec<i16>> { &self.bgm_sender }
    #[allow(dead_code)]
    pub fn bgm_was_active(&self) -> &Arc<AtomicBool> { &self.bgm_was_active }
    #[allow(dead_code)]
    pub fn explode_enabled(&self) -> &Arc<AtomicBool> { &self.explode_enabled }
    #[allow(dead_code)]
    pub fn explode_state(&self) -> &Arc<ExplodeState> { &self.explode_state }
    #[allow(dead_code)]
    pub fn monitor_enabled(&self) -> &Arc<AtomicBool> { &self.monitor_enabled }
    #[allow(dead_code)]
    pub fn monitor_point(&self) -> &Arc<AtomicU32> { &self.monitor_point }
    #[allow(dead_code)]
    #[allow(dead_code)]
    pub fn running(&self) -> &Arc<AtomicBool> { &self.running }
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool { self.running.load(Ordering::Acquire) }

    pub fn start(
        &self,
        input_device_name: Option<String>,
        output_device_name: Option<String>,
        model_name: Option<String>,
        resource_dir: Option<std::path::PathBuf>,
    ) -> Result<(), String> {
        let _lock = self.lifecycle_lock.lock().unwrap_or_else(|e| e.into_inner());
        if self.running.load(Ordering::Acquire) {
            return Err("Engine is already running".to_string());
        }
        self.stop_inner();

        // 保存 resource_dir 供 switch_model 使用
        *self.resource_dir.lock().unwrap() = resource_dir.clone();

        let running = self.running.clone();
        let config = self.config.clone();
        let eq_config = self.eq_config.clone();
        let eq_config_dirty = self.eq_config_dirty.clone();
        let stats = self.stats.clone();
        let bgm_running = self.bgm_running.clone();
        let bgm_gain = self.bgm_gain.clone();
        let bgm_skip_rate = self.bgm_skip_rate.clone();
        let bgm_receiver = self.bgm_receiver.clone();
        let bgm_was_active = self.bgm_was_active.clone();
        let explode_enabled = self.explode_enabled.clone();
        let explode_state = self.explode_state.clone();
        let monitor_enabled = self.monitor_enabled.clone();
        let monitor_point = self.monitor_point.clone();
        let saved_model_states = self.model_states.clone();
        let model_switch_receiver = self.model_switch_receiver.clone();
        let app_handle = self.app_handle.lock().unwrap().clone();

        running.store(true, Ordering::Release);

        let handle = std::thread::Builder::new()
            .name("audio-main".into())
            .spawn(move || {
                if let Err(e) = audio_loop(
                    running, config, eq_config, eq_config_dirty, stats,
                    bgm_running, bgm_gain, bgm_skip_rate, bgm_receiver, bgm_was_active,
                    explode_enabled, explode_state, monitor_enabled, monitor_point,
                    input_device_name, output_device_name, model_name, resource_dir,
                    saved_model_states, model_switch_receiver, app_handle,
                ) {
                    log::error!("Audio loop error: {}", e);
                }
            })
            .map_err(|e| format!("Failed to spawn audio thread: {}", e))?;

        *self.thread_handle.lock().unwrap() = Some(handle);
        Ok(())
    }

    pub fn stop(&self) {
        let _lock = self.lifecycle_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.stop_inner();
    }

    fn stop_inner(&self) {
        self.running.store(false, Ordering::Release);
        if let Some(handle) = self.thread_handle.lock().unwrap().take() {
            let _ = handle.join();
        }
        // 必须设 bgm_thread_running（BGM 线程读的 flag），不是 bgm_running
        self.bgm_thread_running.lock().store(false, Ordering::Release);
        self.bgm_was_active.store(false, Ordering::Release);
        if let Some(handle) = self.bgm_thread_handle.lock().unwrap().take() {
            let _ = handle.join();
        }
        self.bgm_running.store(false, Ordering::Release);
        flush_debug_log();
    }

    pub fn update_config(&self, new_config: DenoiseConfig) { *self.config.write() = new_config; }
    pub fn get_config(&self) -> DenoiseConfig { self.config.read().clone() }
    pub fn update_eq_config(&self, new_config: EqConfig) {
        *self.eq_config.write() = new_config;
        self.eq_config_dirty.store(true, Ordering::Release);
    }
    pub fn get_eq_config(&self) -> EqConfig { self.eq_config.read().clone() }

    /// 热切换降噪模型（不重启引擎）
    /// 在后台线程创建新模型，创建完成后发送到 DSP 线程替换
    pub fn switch_model(&self, model_name: String) -> Result<(), String> {
        if !self.running.load(Ordering::Acquire) {
            return Err("Engine is not running".to_string());
        }

        let sender = self.model_switch_sender.clone();
        let resource_dir = self.resource_dir.lock().unwrap().clone();

        // 后台线程创建模型，避免阻塞 DSP 线程
        std::thread::Builder::new()
            .name("model-loader".into())
            .spawn(move || {
                let new_model = denoise::create_model(&model_name, resource_dir.as_deref());
                // 非阻塞发送，channel 满说明已有切换请求，丢弃本次
                let _ = sender.try_send((model_name, new_model));
            })
            .map_err(|e| format!("Failed to spawn model loader: {}", e))?;

        Ok(())
    }

    pub fn start_bgm(&self, pid: u32) -> Result<(), String> {
        self.stop_bgm();
        let running = Arc::new(AtomicBool::new(true));
        *self.bgm_thread_running.lock() = running.clone();
        let sender = self.bgm_sender.clone();
        let skip_rate = self.bgm_skip_rate.clone();
        let bgm_running = self.bgm_running.clone();
        let handle = std::thread::Builder::new()
            .name(format!("bgm-{}", pid))
            .spawn(move || {
                bgm_running.store(true, Ordering::Release);
                let _ = crate::bgm::bgm_process_loop(running, sender, pid, skip_rate);
                bgm_running.store(false, Ordering::Release);
            })
            .map_err(|e| format!("Failed to spawn BGM thread: {}", e))?;
        *self.bgm_thread_handle.lock().unwrap() = Some(handle);
        self.bgm_was_active.store(true, Ordering::Release);
        Ok(())
    }

    pub fn stop_bgm(&self) {
        // 通知 BGM 线程退出（bgm_thread_running 是线程读的 flag）
        self.bgm_thread_running.lock().store(false, Ordering::Release);
        if let Some(handle) = self.bgm_thread_handle.lock().unwrap().take() {
            let _ = handle.join();
        }
        // 线程退出后 bgm_running 自动变为 false（线程末尾设置）
        self.bgm_running.store(false, Ordering::Release);
    }

    pub fn cancel_bgm_auto_restart(&self) { self.bgm_was_active.store(false, Ordering::Release); }

    pub fn set_explode_mode(&self, enabled: bool) {
        self.explode_enabled.store(enabled, Ordering::Release);
        self.explode_state.enabled.store(enabled, Ordering::Release);
    }

    pub fn set_explode_intensity(&self, intensity: u32) {
        self.explode_state.intensity.store(intensity, Ordering::Release);
    }

    pub fn set_explode_effect(&self, effect: crate::explode::ExplodeEffect) {
        self.explode_state.effect_type.store(effect as u32, Ordering::Release);
    }

    pub fn update_bgm_config(&self, bgm_gain: f32) {
        if bgm_gain.is_finite() {
            self.bgm_gain.store(bgm_gain.to_bits(), Ordering::Release);
        }
    }

    pub fn set_monitor_enabled(&self, enabled: bool) { self.monitor_enabled.store(enabled, Ordering::Release); }
    pub fn set_monitor_point(&self, point: MonitorPoint) { self.monitor_point.store(point, Ordering::Release); }

    #[allow(dead_code)]
    pub fn save_model_state(&self, name: &str, state: Vec<u8>) {
        if let Ok(mut states) = self.model_states.lock() { states.insert(name.to_string(), state); }
    }
    #[allow(dead_code)]
    pub fn get_saved_model_states(&self) -> std::collections::HashMap<String, Vec<u8>> {
        self.model_states.lock().unwrap().clone()
    }

    pub fn list_audio_processes(&self) -> Result<Vec<(String, String, u32)>, String> {
        crate::bgm::list_audio_processes()
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().unwrap() = Some(handle);
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) { self.stop(); }
}

/// 三线程音频循环入口
///
/// 架构：capture_thread → ring A → dsp_thread → ring B → render_thread
#[allow(clippy::too_many_arguments)]
fn audio_loop(
    running: Arc<AtomicBool>,
    config: Arc<RwLock<DenoiseConfig>>,
    eq_config: Arc<RwLock<EqConfig>>,
    eq_config_dirty: Arc<AtomicBool>,
    stats: Arc<RwLock<AudioStats>>,
    bgm_running: Arc<AtomicBool>,
    bgm_gain: Arc<AtomicU32>,
    _bgm_skip_rate: Arc<AtomicU32>,
    bgm_receiver: Receiver<Vec<i16>>,
    _bgm_was_active: Arc<AtomicBool>,
    explode_enabled: Arc<AtomicBool>,
    explode_state: Arc<ExplodeState>,
    monitor_enabled: Arc<AtomicBool>,
    monitor_point: Arc<AtomicU32>,
    input_device_name: Option<String>,
    output_device_name: Option<String>,
    model_name: Option<String>,
    resource_dir: Option<std::path::PathBuf>,
    saved_model_states: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    model_switch_receiver: Receiver<(String, Box<dyn denoise::DenoiseModel>)>,
    app_handle: Option<AppHandle>,
) -> Result<(), String> {
    debug_log("audio_loop: starting (three-thread architecture)");

    let input_id = input_device_name.unwrap_or_default();
    let output_id = output_device_name.unwrap_or_default();

    // ── 在主线程预加载模型（参考 noisegate-ref）──
    let mut denoise = denoise::create_model(model_name.as_deref().unwrap_or("RNNoise"), resource_dir.as_deref());
    let current_model_name = denoise.name().to_string();
    if let Ok(states_lock) = saved_model_states.lock() {
        if let Some(state) = states_lock.get(&current_model_name) {
            log::info!("Restoring model '{}' state ({} bytes)", current_model_name, state.len());
            denoise.load_state(state);
        }
    }
    log::info!("Using denoise model: {}", current_model_name);

    // ── 创建 ring buffers ──
    let (prod_a, cons_a) = HeapRb::<Frame>::new(RING_FRAMES).split();
    let (prod_b, cons_b) = HeapRb::<Frame>::new(RING_FRAMES).split();

    // ── 启动顺序：DSP → capture → render ──

    // ── 1. 启动 DSP 线程（模型已预加载）──
    let running_dsp = running.clone();
    let config_dsp = config.clone();
    let eq_config_dsp = eq_config.clone();
    let eq_config_dirty_dsp = eq_config_dirty.clone();
    let stats_dsp = stats.clone();
    let bgm_running_dsp = bgm_running.clone();
    let bgm_gain_dsp = bgm_gain.clone();
    let bgm_receiver_dsp = bgm_receiver;
    let explode_enabled_dsp = explode_enabled.clone();
    let explode_state_dsp = explode_state.clone();
    let monitor_enabled_dsp = monitor_enabled.clone();
    let monitor_point_dsp = monitor_point.clone();
    let saved_dsp = saved_model_states.clone();
    let model_switch_receiver_dsp = model_switch_receiver;
    let app_dsp = app_handle.clone();
    let model_name_dsp = current_model_name.clone();

    let dsp_handle = std::thread::Builder::new()
        .name("audio-dsp".into())
        .spawn(move || {
            #[cfg(windows)]
            let _mmcss = crate::mmcss::ProAudio::set_for_current_thread();
            dsp_thread(
                running_dsp, cons_a, prod_b,
                config_dsp, eq_config_dsp, eq_config_dirty_dsp, stats_dsp,
                bgm_running_dsp, bgm_gain_dsp, bgm_receiver_dsp,
                explode_enabled_dsp, explode_state_dsp,
                monitor_enabled_dsp, monitor_point_dsp,
                denoise, &model_name_dsp,
                &saved_dsp, model_switch_receiver_dsp, &app_dsp,
            );
        })
        .map_err(|e| format!("Failed to spawn DSP thread: {}", e))?;

    // ── 2. 启动 capture 线程 ──
    struct CaptureSink {
        prod: ringbuf::CachingProd<Arc<HeapRb<Frame>>>,
    }
    impl wasapi_capture::FrameSink for CaptureSink {
        fn on_frame(&mut self, frame: &Frame) {
            if self.prod.try_push(*frame).is_err() {
                debug_log_dev("capture: ring A full, dropping frame");
            }
        }
    }
    let capture = wasapi_capture::WasapiCapture::start(
        &input_id, Box::new(CaptureSink { prod: prod_a }),
    ).map_err(|e| format!("Failed to start capture: {}", e))?;

    // ── 3. 启动 render 线程 ──
    struct RenderSource {
        cons: ringbuf::CachingCons<Arc<HeapRb<Frame>>>,
    }
    impl wasapi_render::FrameSource for RenderSource {
        fn next_frame(&mut self) -> Option<Frame> { self.cons.try_pop() }
    }
    let render = wasapi_render::WasapiRender::start(
        &output_id, Box::new(RenderSource { cons: cons_b }),
    ).map_err(|e| format!("Failed to start render: {}", e))?;

    // ── 主线程等待退出 ──
    let _ = dsp_handle.join();

    // drop capture 和 render（触发各自的线程退出）
    drop(capture);
    drop(render);

    // 保存模型状态（在 drop capture 之后，确保模型已用完）
    flush_debug_log();
    Ok(())
}

/// DSP 线程：从 ring A 拉取 f32 mono 帧 → 处理 → 推入 ring B
fn dsp_thread(
    running: Arc<AtomicBool>,
    mut cons_a: ringbuf::CachingCons<Arc<HeapRb<Frame>>>,
    mut prod_b: ringbuf::CachingProd<Arc<HeapRb<Frame>>>,
    config: Arc<RwLock<DenoiseConfig>>,
    eq_config: Arc<RwLock<EqConfig>>,
    eq_config_dirty: Arc<AtomicBool>,
    stats: Arc<RwLock<AudioStats>>,
    bgm_running: Arc<AtomicBool>,
    bgm_gain: Arc<AtomicU32>,
    bgm_receiver: Receiver<Vec<i16>>,
    explode_enabled: Arc<AtomicBool>,
    explode_state: Arc<ExplodeState>,
    monitor_enabled: Arc<AtomicBool>,
    monitor_point: Arc<AtomicU32>,
    mut denoise: Box<dyn denoise::DenoiseModel>,
    current_model_name: &str,
    saved_model_states: &std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    model_switch_receiver: Receiver<(String, Box<dyn denoise::DenoiseModel>)>,
    _app_handle: &Option<AppHandle>,
) {
    debug_log_dev("dsp_thread: started");

    let mut current_model_name = current_model_name.to_string();
    debug_log(&format!("DSP thread: model={}", current_model_name));

    // EQ 初始化
    let mut eq_processor = crate::eq::EqProcessor::new(48000);
    let mut last_eq_config: Option<EqConfig> = None;
    let current_eq_config = eq_config.read().clone();
    eq_config_dirty.store(false, Ordering::Release);
    if current_eq_config.enabled {
        eq_processor.apply_config(&current_eq_config);
        last_eq_config = Some(current_eq_config.clone());
    }

    // 监听设备：延迟初始化
    let mut monitor = crate::audio_init::MonitorState {
        client: None, render: None, event: None,
        sample_rate: 0, buffer: Vec::new(),
        current_device_id: String::new(), was_streaming: false,
    };
    let mut mon_was_enabled = false;
    let mut resample_buf: Vec<f32> = vec![0.0; FRAME_SIZE * 2];

    // 爆炸模式音频状态
    let mut explode_audio = ExplodeAudioState::new();

    let mut last_strength: f32 = -1.0;
    let mut frame_count: u64 = 0;
    let mut frames_dropped: u64 = 0;
    let mut bgm_buf: Vec<f32> = Vec::new();
    let mut bgm_read_pos: usize = 0;
    let mut hp_x_prev: f32 = 0.0;
    let mut hp_y_prev: f32 = 0.0;
    let mut stats_start = std::time::Instant::now();
    let mut gate_open: f32 = 1.0; // 噪声门状态：0.0=关闭，1.0=打开

    // 打印初始配置
    let init_config = config.read().clone();
    debug_log(&format!("DSP_INIT: model={} enabled={} strength={} suppress={} mic_gain={} eq={} explode={} bgm={}",
        current_model_name, init_config.enabled, init_config.strength, init_config.suppress_level, init_config.mic_gain,
        eq_config.read().enabled, explode_enabled.load(Ordering::Relaxed), bgm_running.load(Ordering::Relaxed)));

    while running.load(Ordering::Acquire) {
        // 检查模型热切换请求（非阻塞，模型已在后台线程创建好）
        if let Ok((new_model_name, mut new_model)) = model_switch_receiver.try_recv() {
            debug_log(&format!("MODEL_SWITCH: {} -> {}", current_model_name, new_model_name));
            // 保存旧模型状态
            if let Some(state) = denoise.save_state() {
                if let Ok(mut states) = saved_model_states.lock() {
                    states.insert(current_model_name.clone(), state);
                }
            }
            // 尝试加载旧模型状态（仅同类型模型兼容）
            if let Ok(states) = saved_model_states.lock() {
                if let Some(state) = states.get(&new_model_name) {
                    new_model.load_state(state);
                    debug_log(&format!("MODEL_SWITCH: restored state for {}", new_model_name));
                }
            }
            // 应用当前 strength 设置
            let current_config = config.read().clone();
            new_model.update_strength(current_config.strength);
            denoise = new_model;
            current_model_name = new_model_name.clone();
            debug_log(&format!("MODEL_SWITCH: now using {}", current_model_name));
        }

        // 从 ring A 拉取帧
        let mut frame = match cons_a.try_pop() {
            Some(f) => f,
            None => {
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue;
            }
        };

        frame_count += 1;
        let current_config = config.read().clone();

        // DeepFilterNet strength 更新
        if (current_config.strength - last_strength).abs() > f32::EPSILON {
            denoise.update_strength(current_config.strength);
            last_strength = current_config.strength;
        }

        // EQ 配置更新
        if eq_config_dirty.load(Ordering::Acquire) {
            let new_eq = eq_config.read().clone();
            eq_processor.apply_config(&new_eq);
            last_eq_config = Some(new_eq);
            eq_config_dirty.store(false, Ordering::Release);
        }

        // 保存原始输入用于频谱/电平统计（normalized f32 [-1.0, 1.0]）
        let input_frame = frame;

        // ── 监听点 1：原始输入 ──
        let mon_enabled = monitor_enabled.load(Ordering::Acquire);
        let mon_point = monitor_point.load(Ordering::Relaxed);
        monitor_write!(monitor, 1, mon_point, mon_enabled, &input_frame, &mut resample_buf);

        // ── 1. 降噪 ──
        if current_config.enabled {
            denoise.process_frame(&mut frame, &input_frame);

            // NaN 检查
            for sample in frame.iter_mut() {
                if !sample.is_finite() { *sample = 0.0; }
            }

            // 每 500 帧打印一次完整状态（仅开发环境）
            if frame_count % 500 == 1 {
                let in_peak = input_frame.iter().fold(0.0f32, |a, &x| a.max(x.abs()));
                let out_peak = frame.iter().fold(0.0f32, |a, &x| a.max(x.abs()));
                debug_log_dev(&format!(
                    "STAGE[model={} strength={} suppress={}]: in={:.6} out={:.6}",
                    current_model_name, current_config.strength, current_config.suppress_level,
                    in_peak, out_peak
                ));
            }

            // Strength mixing（normalized 范围）
            if current_config.strength < 1.0 && !denoise.has_internal_strength_control() {
                for (denoised, &original) in frame.iter_mut().zip(input_frame.iter()) {
                    *denoised = original * (1.0 - current_config.strength) + *denoised * current_config.strength;
                }
            }

            // 噪声门（仅 RNNoise，DeepFilterNet3 不需要）
            if !denoise.has_internal_strength_control() && current_config.suppress_level > 0.01 {
                let threshold = 0.001 + current_config.suppress_level * 0.009;
                let frame_rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
                let target = if frame_rms > threshold { 1.0 } else { 0.0 };
                let speed = if target > gate_open { 0.1 } else { 0.02 };
                gate_open = gate_open + (target - gate_open) * speed;
                if gate_open < 0.99 {
                    for sample in frame.iter_mut() {
                        *sample *= gate_open;
                    }
                }
            }

            // Denormal flush
            for sample in frame.iter_mut() {
                if !sample.is_finite() || sample.abs() < 1e-10 { *sample = 0.0; }
            }
        }

        // ── 监听点 2：降噪后 ──
        monitor_write!(monitor, 2, mon_point, mon_enabled, &frame, &mut resample_buf);

        // ── 2. 增益 ──
        for sample in frame.iter_mut() { *sample *= current_config.mic_gain; }

        // ── 监听点 3：增益后 ──
        monitor_write!(monitor, 3, mon_point, mon_enabled, &frame, &mut resample_buf);

        // ── 3. EQ（带噪声门：信号极小时跳过 EQ，防止 biquad 放大噪声底）──
        if last_eq_config.as_ref().map_or(false, |c| c.enabled) {
            let eq_input_peak = frame.iter().fold(0.0f32, |a, &x| a.max(x.abs()));
            if eq_input_peak > 0.001 {
                eq_processor.process_frame(&mut frame);
                for sample in frame.iter_mut() {
                    if !sample.is_finite() || sample.abs() < 1e-10 { *sample = 0.0; }
                }
            } else {
                // 信号极小时重置 EQ 状态，防止累积
                eq_processor.reset_all();
            }
        }

        // ── 监听点 4：EQ 后 ──
        monitor_write!(monitor, 4, mon_point, mon_enabled, &frame, &mut resample_buf);

        // ── 4. BGM 混音 ──
        if bgm_running.load(Ordering::Acquire) {
            while let Ok(data) = bgm_receiver.try_recv() {
                for &sample in &data {
                    bgm_buf.push(sample as f32 / 32768.0);
                }
            }
            // 溢出保护：已消费部分超过一半时 compact
            if bgm_read_pos > bgm_buf.len() / 2 && bgm_read_pos > 0 {
                bgm_buf.drain(..bgm_read_pos);
                bgm_read_pos = 0;
            }
            // 硬上限：堆积过多时丢弃最旧数据
            let max_bgm = FRAME_SIZE * 20;
            if bgm_buf.len() > max_bgm {
                let excess = bgm_buf.len() - max_bgm;
                bgm_buf.drain(..excess);
                bgm_read_pos = bgm_read_pos.saturating_sub(excess);
            }
            let bgm_gain_linear = f32::from_bits(bgm_gain.load(Ordering::Relaxed)).max(0.0).min(2.0);
            if !bgm_buf.is_empty() {
                let needed = FRAME_SIZE * 2;
                if bgm_read_pos + needed <= bgm_buf.len() {
                    for i in 0..FRAME_SIZE {
                        let l = bgm_buf[bgm_read_pos + i * 2];
                        let r = bgm_buf[bgm_read_pos + i * 2 + 1];
                        let mono = (l + r) * 0.5;
                        frame[i] = frame[i] + mono * bgm_gain_linear;
                    }
                    bgm_read_pos += needed;
                }
                if bgm_read_pos >= bgm_buf.len() {
                    bgm_buf.clear();
                    bgm_read_pos = 0;
                }
            }
        }

        // ── 5. 爆炸模式（模块内部用 i16 范围常量，需转换）──
        if explode_enabled.load(Ordering::Acquire) {
            let mut scaled = [0.0f32; FRAME_SIZE];
            for (i, &s) in frame.iter().enumerate() { scaled[i] = s * 32767.0; }
            let mut output = [0.0f32; FRAME_SIZE];
            crate::explode::process_explode_into(&scaled, &mut output, &explode_state, &mut explode_audio);
            for (i, &s) in output.iter().enumerate() { frame[i] = s / 32767.0; }
        }

        // ── 6. Soft limiter（normalized f32 范围，炸麦模式下跳过压缩）──
        if !explode_enabled.load(Ordering::Acquire) {
            for sample in frame.iter_mut() {
                if sample.is_finite() {
                    let abs = sample.abs();
                    if abs > 0.73 { *sample = sample.signum() * (0.73 + (abs - 0.73) * 0.1).min(0.92); }
                    if !sample.is_finite() { *sample = 0.0; }
                } else { *sample = 0.0; }
            }
        } else {
            // 炸麦模式：只做 NaN 检查，不做压缩
            for sample in frame.iter_mut() {
                if !sample.is_finite() { *sample = 0.0; }
            }
        }

        // ── 6.5 高通滤波器（80Hz，切除次声波）──
        {
            const HP_ALPHA: f32 = 0.98953;
            let mut x_prev = hp_x_prev;
            let mut y_prev = hp_y_prev;
            for sample in frame.iter_mut() {
                let x = *sample;
                let y = HP_ALPHA * (y_prev + x - x_prev);
                *sample = y;
                x_prev = x;
                y_prev = y;
            }
            if !x_prev.is_finite() || x_prev.abs() < 1e-10 { x_prev = 0.0; }
            if !y_prev.is_finite() || y_prev.abs() < 1e-10 { y_prev = 0.0; }
            hp_x_prev = x_prev;
            hp_y_prev = y_prev;
        }

        // ── 监听点 5：最终输出（limiter 后）──
        monitor_write!(monitor, 5, mon_point, mon_enabled, &frame, &mut resample_buf);

        // 监听流启停控制
        if mon_enabled {
            // 延迟初始化
            if !mon_was_enabled {
                monitor = init_monitor(&String::new(), 48000, FRAME_SIZE);
                mon_was_enabled = true;
                debug_log_dev("Monitor: initialized");
            }
            if !monitor.was_streaming && mon_point > 0 {
                if let Some(ref mut m_client) = monitor.client {
                    match m_client.start_stream() {
                        Ok(()) => {
                            monitor.was_streaming = true;
                            debug_log_dev(&format!("Monitor: started streaming at point {}", mon_point));
                        }
                        Err(e) => {
                            debug_log(&format!("Monitor: start_stream FAILED: {:?}", e));
                        }
                    }
                }
            }
        } else {
            if monitor.was_streaming {
                if let Some(ref mut m_client) = monitor.client {
                    let _ = m_client.stop_stream();
                    debug_log_dev("Monitor: stopped streaming");
                }
                monitor.was_streaming = false;
            }
            mon_was_enabled = false;
        }

        // ── 8. 统计更新（每 5 帧） ──
        if frame_count % 5 == 0 {
            // 帧数据是 normalized f32 [-1.0, 1.0]，参考值 1.0
            let input_rms = calculate_rms(&input_frame);
            let output_rms = calculate_rms(&frame);
            let input_level_db = if input_rms > 1e-10 {
                20.0 * input_rms.log10()
            } else {
                -100.0
            };
            let output_level_db = if output_rms > 1e-10 {
                20.0 * output_rms.log10()
            } else {
                -100.0
            };

            // compute_spectrum_into 期望 i16 范围，缩放后计算
            let mut scaled_input = [0.0f32; FRAME_SIZE];
            for (i, &s) in input_frame.iter().enumerate() {
                scaled_input[i] = s * 32767.0;
            }
            let mut spectrum_buf = [0.0f32; 32];
            compute_spectrum_into(&scaled_input, &mut spectrum_buf);

            let mut s = stats.write();
            s.input_level = input_level_db;
            s.output_level = output_level_db;
            s.noise_reduction_db = input_level_db - output_level_db;
            s.latency_ms = 10.0 + 10.0; // capture buffer + render buffer ≈ 20ms
            s.frames_processed = frame_count;
            s.frames_dropped = frames_dropped;
            s.spectrum.copy_from_slice(&spectrum_buf);
        }

        // ── 9. 推入 ring B ──
        if prod_b.try_push(frame).is_err() {
            // 前 100 帧是启动预热期（render 初始化等），不计入丢帧统计
            if frame_count > 100 {
                frames_dropped += 1;
            }
        }

        // ── 10. 健康统计 ──
        if stats_start.elapsed().as_secs() >= 5 {
            let drop_rate = frames_dropped as f64 / frame_count as f64 * 100.0;
            debug_log(&format!("HEALTH: frame={} dropped={} ({:.2}%)", frame_count, frames_dropped, drop_rate));
            stats_start = std::time::Instant::now();
            flush_debug_log();
        }
    }

    // 保存模型状态
    if let Some(s) = denoise.save_state() {
        if let Ok(mut states_lock) = saved_model_states.lock() {
            states_lock.insert(current_model_name, s);
        }
    }
    debug_log(&format!("dsp_thread: exiting (processed {} frames, dropped {})", frame_count, frames_dropped));
    flush_debug_log();
}
