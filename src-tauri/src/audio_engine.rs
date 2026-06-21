use crossbeam_channel::{bounded, Receiver, Sender};
use crate::denoise::{self, FRAME_SIZE};
use crate::eq::{EqConfig, EqProcessor};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use wasapi::*;
pub use crate::debug::debug_log;
use crate::device::find_device;
pub use crate::audio_utils::{calculate_rms, compute_spectrum, write_to_monitor, resample_linear, bytes_to_f32_samples, downmix_to_mono, upmix_to_stereo};
pub use crate::bgm::list_audio_processes;
pub use crate::bgm::bgm_process_loop;

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
}

impl Default for BgmConfig {
    fn default() -> Self {
        Self {
            process_names: Vec::new(),
            process_pids: Vec::new(),
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

/// 监听点：在音频链路的哪个阶段输出到监听设备
/// 0=关闭, 1=原始输入, 2=降噪后, 3=增益后, 4=EQ后, 5=最终输出
pub type MonitorPoint = u32;

pub struct AudioEngine {
    running: Arc<AtomicBool>,
    config: Arc<RwLock<DenoiseConfig>>,
    eq_config: Arc<RwLock<EqConfig>>,
    /// EQ 配置变更标记：update_eq_config 时置 true，audio_loop 读取后清 false
    eq_config_dirty: Arc<AtomicBool>,
    stats: Arc<RwLock<AudioStats>>,
    /// audio_loop 读取：BGM 是否激活（控制混音开关）
    bgm_running: Arc<AtomicBool>,
    /// BGM 增益（无锁原子读，避免 RwLock 竞争）
    bgm_gain: Arc<AtomicU32>,
    /// BGM 漂移补偿：输出 pending 水位过高时，通知捕获线程跳过部分采样（f32 bit-cast，0.0~0.05）
    bgm_skip_rate: Arc<AtomicU32>,
    bgm_config: Arc<RwLock<BgmConfig>>,
    bgm_sender: Sender<Vec<i16>>,
    bgm_receiver: Receiver<Vec<i16>>,
    /// 每批 BGM 线程的独立停止标志，start_bgm 时替换（新旧线程用不同 Arc，互不干扰）
    bgm_thread_running: parking_lot::Mutex<Arc<AtomicBool>>,
    /// 引擎重启前 BGM 是否激活，用于自动恢复
    bgm_was_active: Arc<AtomicBool>,
    explode_enabled: Arc<AtomicBool>,
    explode_intensity: Arc<AtomicU32>,
    monitor_enabled: Arc<AtomicBool>,
    monitor_point: Arc<AtomicU32>,
    thread_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    bgm_thread_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    /// 模型内部状态（归一化统计等），按模型名分别保存
    model_states: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    /// AppHandle 用于 emit 事件通知前端
    app_handle: std::sync::Mutex<Option<AppHandle>>,
    /// start/stop 生命周期互斥锁，防止并发调用导致双音频线程
    lifecycle_lock: std::sync::Mutex<()>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (bgm_tx, bgm_rx) = bounded::<Vec<i16>>(32);
        Self {
            running: Arc::new(AtomicBool::new(false)),
            config: Arc::new(RwLock::new(DenoiseConfig::default())),
            eq_config: Arc::new(RwLock::new(EqConfig::default())),
            eq_config_dirty: Arc::new(AtomicBool::new(true)),
            stats: Arc::new(RwLock::new(AudioStats::default())),
            bgm_running: Arc::new(AtomicBool::new(false)),
            bgm_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            bgm_skip_rate: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            bgm_config: Arc::new(RwLock::new(BgmConfig::default())),
            bgm_sender: bgm_tx,
            bgm_receiver: bgm_rx,
            bgm_thread_running: parking_lot::Mutex::new(Arc::new(AtomicBool::new(false))),
            bgm_was_active: Arc::new(AtomicBool::new(false)),
            explode_enabled: Arc::new(AtomicBool::new(false)),
            explode_intensity: Arc::new(AtomicU32::new(50)),
            monitor_enabled: Arc::new(AtomicBool::new(false)),
            monitor_point: Arc::new(AtomicU32::new(0)),
            thread_handle: std::sync::Mutex::new(None),
            bgm_thread_handle: std::sync::Mutex::new(None),
            model_states: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            app_handle: std::sync::Mutex::new(None),
            lifecycle_lock: std::sync::Mutex::new(()),
        }
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
    }

    pub fn start(
        &self,
        input_device_name: Option<String>,
        output_device_name: Option<String>,
        model_name: Option<String>,
        resource_dir: Option<std::path::PathBuf>,
        monitor_enabled_init: bool,
    ) -> Result<(), String> {
        // 生命周期互斥锁：防止 start/stop 并发调用导致双音频线程
        let _guard = self.lifecycle_lock.lock().unwrap_or_else(|e| e.into_inner());

        // 如果引擎正在运行，先停止并等待清理完成
        if self.running.load(Ordering::Relaxed) {
            debug_log("Engine restart: stopping current engine first");
            self.stop_inner();
        }

        // 模型在音频线程内加载（避免阻塞主线程）
        let mn = model_name.clone();
        let rd = resource_dir.clone();

        let running = self.running.clone();
        let config = self.config.clone();
        let eq_config = self.eq_config.clone();
        let eq_config_dirty = self.eq_config_dirty.clone();
        let stats = self.stats.clone();
        let bgm_running = self.bgm_running.clone();
        let bgm_gain = self.bgm_gain.clone();
        let bgm_receiver = self.bgm_receiver.clone();
        let explode_enabled = self.explode_enabled.clone();
        let explode_intensity = self.explode_intensity.clone();
        let monitor_enabled = self.monitor_enabled.clone();
        let monitor_point = self.monitor_point.clone();
        let model_states = self.model_states.clone();
        let bgm_skip_rate = self.bgm_skip_rate.clone();

        // 同步初始监听状态
        self.monitor_enabled.store(monitor_enabled_init, Ordering::Relaxed);

        running.store(true, Ordering::Release);

        // 获取 AppHandle 用于 emit 事件
        let app_handle = self.app_handle.lock().unwrap_or_else(|e| e.into_inner()).clone();

        let handle = std::thread::spawn(move || {
            if let Err(e) = audio_loop(
                running,
                config,
                eq_config,
                eq_config_dirty,
                stats,
                bgm_running,
                bgm_gain,
                Some(bgm_receiver),
                input_device_name,
                output_device_name,
                mn,
                rd,
                explode_enabled,
                explode_intensity,
                monitor_enabled,
                monitor_point,
                model_states,
                app_handle,
                bgm_skip_rate,
            ) {
                log::error!("Audio engine error: {}", e);
            }
        });

        // 存储线程句柄，stop() 时等待退出
        *self.thread_handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);

        // 引擎重启后自动恢复 BGM（如果之前是激活的）
        if self.bgm_was_active.swap(false, Ordering::Acquire) {
            let pids = self.bgm_config.read().process_pids.clone();
            if !pids.is_empty() {
                log::info!("auto-restarting BGM after engine restart, pids={:?}", pids);
                debug_log(&format!("start: auto-restarting BGM, pids={:?}", pids));
                if let Err(e) = self.start_bgm(pids) {
                    log::error!("auto-restart BGM failed: {}", e);
                }
            }
        }

        Ok(())
    }

    pub fn stop(&self) {
        let _guard = self.lifecycle_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.stop_inner();
    }

    /// 内部停止，不加锁（供 start() 内部调用，避免死锁）
    /// 停止 BGM 线程但不设置 bgm_was_active（内部重启不应污染恢复标记）
    fn stop_inner(&self) {
        self.running.store(false, Ordering::Release);
        // 停止 BGM 线程（不设置 bgm_was_active）
        {
            let stop = self.bgm_thread_running.lock().clone();
            stop.store(false, Ordering::Release);
        }
        self.bgm_running.store(false, Ordering::Release);
        let old_handle = self.bgm_thread_handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = old_handle {
            std::thread::Builder::new()
                .name("bgm-cleanup".into())
                .spawn(move || { let _ = h.join(); })
                .ok();
        }
        // 等待音频线程退出，避免旧流残留导致回音
        // audio_loop 内部会先 join 输入/输出线程，再退出
        if let Some(handle) = self.thread_handle.lock().unwrap_or_else(|e| e.into_inner()).take() {
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
    ///
    /// 设计：每批线程持有独立的 `AtomicBool` 停止标志。`start_bgm` 不 join 旧线程
    /// （避免阻塞 engine lock），只标记旧 flag 为 false 让其自行退出（~100ms 内
    /// `wait_for_event` 超时后退出）。新线程使用新 flag，互不干扰。
    pub fn start_bgm(&self, pids: Vec<u32>) -> Result<(), String> {
        debug_log("start_bgm: begin");
        // WASAPI 共享模式下多个 loopback 客户端会互相干扰，强制单进程
        let pids = if pids.len() > 1 {
            log::warn!("start_bgm: multiple PIDs passed ({}), truncating to first", pids.len());
            vec![pids[0]]
        } else {
            pids
        };
        // 1. 标记旧线程停止（不 join，让其自行退出，避免阻塞 engine lock）
        {
            let old_stop = self.bgm_thread_running.lock().clone();
            old_stop.store(false, Ordering::Release);
            debug_log("start_bgm: old threads marked stop");
        }
        self.bgm_running.store(false, Ordering::Release);
        // 不在这里 drain channel——audio_loop 的 Receiver clone 是竞争消费者，
        // 两边同时 try_recv 会互相抢消息。旧数据在 bgm_running=false 期间不会被混入输出，
        // 下次 bgm_running=true 时 audio_loop 会自然消费掉（bgm_buf 有上限保护）。

        log::info!("start_bgm: pids={:?}", pids);
        let sender = self.bgm_sender.clone();
        let skip_rate = self.bgm_skip_rate.clone();

        // 更新配置
        {
            let mut cfg = self.bgm_config.write();
            cfg.process_pids = pids.clone();
        }

        // 2. 创建新的独立停止标志给这批线程（旧线程持有旧 Arc，互不干扰）
        let new_stop = Arc::new(AtomicBool::new(true));
        *self.bgm_thread_running.lock() = new_stop.clone();
        self.bgm_running.store(true, Ordering::Release);
        debug_log(&format!("start_bgm: spawning {} threads", pids.len()));

        // 为每个 PID 启动一个独立的 loopback 线程
        let mut handles = Vec::new();
        for pid in pids {
            let sender_clone = sender.clone();
            let running_clone = new_stop.clone();
            let skip_rate_clone = skip_rate.clone();
            let handle = std::thread::Builder::new()
                .name(format!("bgm-loopback-{}", pid))
                .spawn(move || {
                    debug_log(&format!("bgm-loopback-{}: thread started", pid));
                    if let Err(e) = bgm_process_loop(running_clone, sender_clone, pid, skip_rate_clone) {
                        log::error!("BGM process loopback error for pid {}: {}", pid, e);
                        debug_log(&format!("bgm-loopback-{}: ERROR {}", pid, e));
                    }
                    debug_log(&format!("bgm-loopback-{}: thread exiting", pid));
                })
                .map_err(|e| format!("Failed to spawn BGM thread: {}", e))?;
            handles.push(handle);
            debug_log(&format!("start_bgm: spawned thread for pid {}", pid));
        }

        // manager 线程负责 join 所有子线程（不阻塞主流程，只用于清理泄漏的线程）
        let manager_handle = std::thread::Builder::new()
            .name("bgm-manager".into())
            .spawn(move || {
                for handle in handles {
                    let _ = handle.join();
                }
            })
            .map_err(|e| format!("Failed to spawn BGM manager: {}", e))?;

        // 旧句柄交给后台线程 join
        let old_handle = self.bgm_thread_handle.lock().unwrap_or_else(|e| e.into_inner()).replace(manager_handle);
        if let Some(h) = old_handle {
            std::thread::Builder::new()
                .name("bgm-cleanup".into())
                .spawn(move || { let _ = h.join(); })
                .ok();
        }
        debug_log("start_bgm: done");
        // 线程全部启动成功后才标记，失败时不会残留 true
        self.bgm_was_active.store(true, Ordering::Release);

        Ok(())
    }

    /// 停止 BGM 捕获（只标记停止，不 join，避免阻塞 engine lock）
    pub fn stop_bgm(&self) {
        debug_log("stop_bgm: begin");
        // 记录 BGM 是否活跃，用于引擎重启后自动恢复
        if self.bgm_running.load(Ordering::Acquire) {
            self.bgm_was_active.store(true, Ordering::Release);
        }
        {
            let stop = self.bgm_thread_running.lock().clone();
            stop.store(false, Ordering::Release);
            debug_log("stop_bgm: threads marked stop");
        }
        self.bgm_running.store(false, Ordering::Release);
        // 将旧句柄交给后台线程 join（避免 detach 导致 WASAPI 资源泄漏）
        let old_handle = self.bgm_thread_handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = old_handle {
            std::thread::Builder::new()
                .name("bgm-cleanup".into())
                .spawn(move || { let _ = h.join(); })
                .ok();
        }
        debug_log("stop_bgm: done");
    }

    /// 用户手动停止 BGM 时调用，取消引擎重启后的自动恢复
    pub fn cancel_bgm_auto_restart(&self) {
        self.bgm_was_active.store(false, Ordering::Release);
    }

    /// 更新 BGM 配置（增益等）
    pub fn update_bgm_config(&self, bgm_gain: f32) {
        if bgm_gain.is_finite() {
            self.bgm_gain.store(bgm_gain.max(0.0).min(10.0).to_bits(), Ordering::Release);
        }
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

    /// 设置监听点（0=关闭, 1=原始输入, 2=降噪后, 3=增益后, 4=EQ后, 5=最终输出）
    pub fn set_monitor_point(&self, point: MonitorPoint) {
        self.monitor_point.store(point, Ordering::Relaxed);
    }

    /// 更新 EQ 配置
    pub fn update_eq_config(&self, eq_config: EqConfig) {
        *self.eq_config.write() = eq_config;
        self.eq_config_dirty.store(true, Ordering::Release);
    }

    /// 获取 EQ 配置
    pub fn get_eq_config(&self) -> EqConfig {
        self.eq_config.read().clone()
    }
}

// BGM 相关代码（list_audio_processes, bgm_process_loop, APP_NAME_MAP 等）已移至 bgm.rs

// ====== 音频处理链：将原始字节转为 f32 mono 48kHz ======
fn process_input(
    raw_bytes: &[u8],
    bits: u16,
    sample_type: &SampleType,
    channels: usize,
    sample_rate: u32,
) -> Vec<f32> {
    let raw_samples = bytes_to_f32_samples(raw_bytes, bits, sample_type, channels);
    let mono = downmix_to_mono(&raw_samples, channels);
    if sample_rate != 48000 {
        resample_linear(&mono, sample_rate, 48000)
    } else {
        mono
    }
}

// ====== 将 f32 mono 样本格式化为输出设备字节 ======
fn format_output_bytes(
    samples: &[f32],
    buffer: &mut [u8],
    channels: usize,
    bytes_per_frame: usize,
    bits: u16,
    sample_type: &SampleType,
) -> usize {
    let stereo = upmix_to_stereo(samples, channels);
    let out_frames = stereo.len() / channels;
    let out_bytes = out_frames * bytes_per_frame;
    buffer[..out_bytes].fill(0);
    for (i, &sample) in stereo.iter().enumerate() {
        let byte_pos = i * (bytes_per_frame / channels);
        match (bits, sample_type) {
            (8, SampleType::Int) => {
                let val = ((sample / 128.0) + 128.0).clamp(0.0, 255.0) as u8;
                if byte_pos < buffer.len() { buffer[byte_pos] = val; }
            }
            (16, SampleType::Int) => {
                let val = sample.clamp(-32768.0, 32767.0) as i16;
                let bytes = val.to_le_bytes();
                if byte_pos + 1 < buffer.len() {
                    buffer[byte_pos] = bytes[0];
                    buffer[byte_pos + 1] = bytes[1];
                }
            }
            (24, SampleType::Int) => {
                let val = (sample * 256.0).clamp(-8388608.0, 8388607.0) as i32;
                if byte_pos + 2 < buffer.len() {
                    buffer[byte_pos] = (val & 0xFF) as u8;
                    buffer[byte_pos + 1] = ((val >> 8) & 0xFF) as u8;
                    buffer[byte_pos + 2] = ((val >> 16) & 0xFF) as u8;
                }
            }
            (32, SampleType::Int) => {
                let val = (sample * 65536.0).clamp(-2147483648.0, 2147483647.0) as i32;
                let bytes = val.to_le_bytes();
                if byte_pos + 3 < buffer.len() {
                    buffer[byte_pos..byte_pos + 4].copy_from_slice(&bytes);
                }
            }
            (32, SampleType::Float) => {
                let val = (sample / 32767.0).clamp(-1.0, 1.0);
                let bytes = val.to_le_bytes();
                if byte_pos + 3 < buffer.len() {
                    buffer[byte_pos..byte_pos + 4].copy_from_slice(&bytes);
                }
            }
            (64, SampleType::Float) => {
                let val = (sample as f64 / 32767.0).clamp(-1.0, 1.0);
                let bytes = val.to_le_bytes();
                if byte_pos + 7 < buffer.len() {
                    buffer[byte_pos..byte_pos + 8].copy_from_slice(&bytes);
                }
            }
            _ => {
                let val = sample.clamp(-32768.0, 32767.0) as i16;
                let bytes = val.to_le_bytes();
                if byte_pos + 1 < buffer.len() {
                    buffer[byte_pos] = bytes[0];
                    buffer[byte_pos + 1] = bytes[1];
                }
            }
        }
    }
    out_bytes
}

// ====== 处理链各阶段峰值 ======
struct PeakDiagnostics {
    pre_gain: f32,
    post_gain: f32,
    post_eq: f32,
    post_bgm: f32,
    output: f32,
}

// ====== 诊断日志：每秒摘要 + 异常告警 ======
fn log_diagnostics(
    frame_count: u64,
    input_frame: &[f32; 480],
    peaks: &PeakDiagnostics,
    mic_gain: f32,
    eq_enabled: bool,
    bgm_active: bool,
) {
    // 每秒完整信号链摘要
    if frame_count % 480 == 0 {
        debug_log(&format!(
            "DIAG frame={} | in={:.0} preG={:.0} postG={:.0} postEQ={:.0} postBGM={:.0} out={:.0} | gain={:.1} eq={} bgm={}",
            frame_count,
            input_frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max),
            peaks.pre_gain, peaks.post_gain, peaks.post_eq, peaks.post_bgm, peaks.output,
            mic_gain, eq_enabled, bgm_active
        ));
    }

    // 异常告警
    let alarm = peaks.post_gain > 24000.0 || peaks.post_eq > 24000.0
        || peaks.post_bgm > 24000.0 || peaks.output > 24000.0;
    if alarm {
        debug_log(&format!(
            "ALARM frame={} | in={:.0} preG={:.0} postG={:.0} postEQ={:.0} postBGM={:.0} out={:.0} | gain={:.1} eq={} bgm={}",
            frame_count,
            input_frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max),
            peaks.pre_gain, peaks.post_gain, peaks.post_eq, peaks.post_bgm, peaks.output,
            mic_gain, eq_enabled, bgm_active
        ));
    }
}

/// WASAPI 输出资源（跨线程传递用）
/// wasapi 的 COM 对象和 Handle 不实现 Send，但 COM MTA 模式下跨线程安全
struct OutputResources {
    client: AudioClient,
    render: AudioRenderClient,
    event: wasapi::Handle,
}
unsafe impl Send for OutputResources {}

/// 输出线程：独立驱动 WASAPI 输出设备时钟，从 channel 接收处理后的音频数据
/// 与处理线程解耦，确保输出写入不受降噪/EQ/BGM 处理延迟影响
fn output_thread(
    running: Arc<AtomicBool>,
    receiver: Receiver<Vec<u8>>,
    res: OutputResources,
    output_bytes_per_frame: usize,
    frame_size: usize,
    bgm_skip_rate: Arc<AtomicU32>,
) {
    let _ = initialize_mta().ok();

    // 提升输出线程到实时优先级
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(hThread: isize, nPriority: i32) -> i32;
        }
        SetThreadPriority(GetCurrentThread(), 15); // THREAD_PRIORITY_TIME_CRITICAL
    }

    debug_log("output_thread: started");
    let mut output_buf: Vec<u8> = Vec::new();
    let mut frame_count: u64 = 0;

    while running.load(Ordering::Acquire) {
        // 非阻塞接收新数据（避免阻塞导致写入延迟）
        while let Ok(data) = receiver.try_recv() {
            output_buf.extend_from_slice(&data);
        }

        // 等待 WASAPI 输出设备就绪
        if res.event.wait_for_event(100).is_err() {
            continue;
        }

        // 获取可用空间，写入数据
        match res.client.get_available_space_in_frames() {
            Ok(available) if available > 0 => {
                let available_bytes = available as usize * output_bytes_per_frame;
                let write_bytes = output_buf.len().min(available_bytes);
                if write_bytes > 0 {
                    // 按帧对齐
                    let write_bytes = (write_bytes / output_bytes_per_frame) * output_bytes_per_frame;
                    if write_bytes > 0 {
                        let frames = write_bytes / output_bytes_per_frame;
                        if let Err(e) = res.render.write_to_device(frames, &output_buf[..write_bytes], None) {
                            log::warn!("output_thread: write error: {:?}", e);
                        }
                        output_buf.drain(..write_bytes);
                    }
                }
                // channel 中剩余数据过多时丢弃（处理速度跟不上输出）
                let max_buf = frame_size * output_bytes_per_frame * 10; // ~100ms
                if output_buf.len() > max_buf {
                    let drain = output_buf.len() - max_buf;
                    output_buf.drain(..drain);
                    if frame_count % 480 == 0 {
                        debug_log(&format!("output_thread: dropped {} bytes (channel overflow)", drain));
                    }
                }
            }
            Ok(_) => {
                // 设备缓冲区满，数据保留在 output_buf 下次再写
            }
            Err(e) => {
                log::warn!("output_thread: get_available_space error: {:?}", e);
            }
        }

        frame_count += 1;

        // BGM 漂移补偿：根据 output_buf 水位通知 BGM 捕获线程调整采样率
        let pending_frames = output_buf.len() / output_bytes_per_frame;
        let skip = if pending_frames > frame_size * 2 {
            0.05
        } else if pending_frames > frame_size {
            ((pending_frames - frame_size) as f32 / frame_size as f32 * 0.05).max(0.001)
        } else {
            0.0
        };
        bgm_skip_rate.store(skip.to_bits(), Ordering::Relaxed);
    }

    // 清理：停止 WASAPI 流
    let _ = res.client.stop_stream();
    debug_log("output_thread: exiting");
}

/// 主音频循环：麦克风采集 → 降噪 → EQ → 混音（可选 BGM）→ 输出
fn audio_loop(
    running: Arc<AtomicBool>,
    config: Arc<RwLock<DenoiseConfig>>,
    eq_config: Arc<RwLock<EqConfig>>,
    eq_config_dirty: Arc<AtomicBool>,
    stats: Arc<RwLock<AudioStats>>,
    bgm_running: Arc<AtomicBool>,
    bgm_gain: Arc<AtomicU32>,
    bgm_receiver: Option<Receiver<Vec<i16>>>,
    input_device_name: Option<String>,
    output_device_name: Option<String>,
    model_name: Option<String>,
    resource_dir: Option<std::path::PathBuf>,
    explode_enabled: Arc<AtomicBool>,
    explode_intensity: Arc<AtomicU32>,
    monitor_enabled: Arc<AtomicBool>,
    monitor_point: Arc<AtomicU32>,
    saved_model_states: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    _app_handle: Option<AppHandle>,
    bgm_skip_rate: Arc<AtomicU32>,
) -> Result<(), String> {
    let _ = initialize_mta().ok();

    // 提升音频线程到实时优先级，防止游戏抢占 CPU 导致颤音
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(hThread: isize, nPriority: i32) -> i32;
        }
        // THREAD_PRIORITY_TIME_CRITICAL = 15
        SetThreadPriority(GetCurrentThread(), 15);
    }

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
    let output_bits = output_format.get_bitspersample();
    let output_sample_type = output_format.get_subformat().unwrap_or(SampleType::Int);
    log::info!("Output device format: {}Hz, {}ch, {}bit, {:?}",
        output_sample_rate, output_channels, output_bits, output_sample_type
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
    let mut monitor_sample_rate: u32 = output_sample_rate; // 默认用输出设备采样率，初始化时会更新
    // 监听缓冲区：考虑重采样后可能变大（最高 192kHz → 4x）
    let monitor_max_frames = frame_size * (output_sample_rate as usize / 48000 + 1);
    let mut monitor_buffer = vec![0u8; monitor_max_frames * 2 * 2]; // stereo i16
    let mut current_monitor_device_id = String::new();

    // 辅助闭包：初始化监听客户端到指定设备
    let input_device_id_for_monitor = input_device.get_id().unwrap_or_default();
    let init_monitor_client = |device: &Device, client_opt: &mut Option<AudioClient>, render_opt: &mut Option<AudioRenderClient>, event_opt: &mut Option<wasapi::Handle>, device_id_out: &mut String, monitor_sr: &mut u32| -> bool {
        let device_id = device.get_id().unwrap_or_default();
        let device_name = device.get_friendlyname().unwrap_or_default();

        // 检查同 USB 设备冲突
        let extract_usb_id = |id: &str| -> String {
            if let Some(start) = id.find("vid_") {
                let rest = &id[start..];
                if let Some(end) = rest.find('&') {
                    if let Some(pid_start) = rest.find("pid_") {
                        let pid_rest = &rest[pid_start..];
                        if let Some(pid_end) = pid_rest.find(|c: char| c == '&' || c == '#') {
                            return rest[..end + pid_end].to_string();
                        }
                    }
                }
            }
            String::new()
        };
        let monitor_usb = extract_usb_id(&device_id);
        let input_usb = extract_usb_id(&input_device_id_for_monitor);
        if !monitor_usb.is_empty() && monitor_usb == input_usb {
            log::warn!("Monitor skipped: device '{}' shares USB ID with input", device_name);
            debug_log(&format!("Monitor: SKIPPED - '{}' same USB device as input", device_name));
            return false;
        }

        if let Ok(mut m_client) = device.get_iaudioclient() {
            // 获取监听设备的采样率，但固定使用 16bit int 格式（写入代码固定按 i16 处理）
            let device_sample_rate = m_client.get_mixformat()
                .map(|f| f.get_samplespersec())
                .unwrap_or(48000);
            let monitor_format = WaveFormat::new(16, 16, &SampleType::Int, device_sample_rate as usize, 2, None);
            let (def_time, _) = m_client.get_periods().unwrap_or((0, 0));
            if m_client.initialize_client(
                &monitor_format,
                def_time,
                &Direction::Render,
                &ShareMode::Shared,
                true,
            ).is_ok() {
                if let Ok(render) = m_client.get_audiorenderclient() {
                    let evt = m_client.set_get_eventhandle();
                    log::info!("Monitor client ready on '{}' (format: {}bit, {}Hz)", device_name, monitor_format.get_bitspersample(), device_sample_rate);
                    debug_log(&format!("Monitor: READY on '{}' ({}Hz)", device_name, device_sample_rate));
                    *client_opt = Some(m_client);
                    *render_opt = Some(render);
                    *event_opt = evt.ok();
                    *device_id_out = device_id;
                    *monitor_sr = device_sample_rate;
                    return true;
                }
            }
        }
        false
    };

    // 初始化监听：系统默认输出设备
    debug_log("Monitor: looking for default output device...");
    match find_device(None, false) {
        Ok(monitor_output) => {
            let monitor_output_name = monitor_output.get_friendlyname().unwrap_or_default();
            debug_log(&format!("Monitor: default output = '{}'", monitor_output_name));
            init_monitor_client(
                &monitor_output, &mut monitor_client_opt, &mut monitor_render_opt, &mut monitor_event_opt, &mut current_monitor_device_id, &mut monitor_sample_rate
            );
        }
        Err(e) => {
            debug_log(&format!("Monitor: failed to find default output device: {}", e));
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
    let mut last_eq_config: Option<EqConfig> = None;
    let mut last_strength = -1.0f32; // 强制首次同步
    let current_eq_config = eq_config.read().clone();
    eq_config_dirty.store(false, Ordering::Release); // 初始读取后清除标记
    if current_eq_config.enabled {
        eq_processor.apply_config(&current_eq_config);
        last_eq_config = Some(current_eq_config.clone());
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

    // ====== 三线程架构：输出线程独立驱动 WASAPI 时钟 ======
    // 创建输出 channel（处理线程 → 输出线程）
    let (output_tx, output_rx) = bounded::<Vec<u8>>(10);

    // 取出 output 相关句柄，打包为 OutputResources 移交给输出线程
    let output_res = OutputResources {
        client: output_client,
        render: output_render,
        event: _output_handle,
    };

    // spawn 输出线程
    let bgm_skip_rate_out = bgm_skip_rate.clone();
    let running_out = running.clone();
    let output_handle = std::thread::Builder::new()
        .name("audio-output".into())
        .spawn(move || {
            output_thread(
                running_out,
                output_rx,
                output_res,
                output_bytes_per_frame,
                frame_size,
                bgm_skip_rate_out,
            );
        })
        .map_err(|e| format!("Failed to spawn output thread: {}", e))?;

    let mut input_buffer = vec![0u8; frame_size * input_bytes_per_frame];
    // 输出缓冲区：按最大可能的重采样膨胀分配（如 96kHz 输出时翻倍）
    let max_output_ratio = (output_sample_rate as f64 / 48000.0).ceil() as usize;
    let max_output_frames = frame_size * max_output_ratio;
    let mut output_buffer = vec![0u8; max_output_frames * output_bytes_per_frame];
    let mut frame_count: u64 = 0;

    // 输入累积缓冲：非 48kHz 设备需要重采样，这里累积 mono f32 样本
    let mut input_acc: Vec<f32> = Vec::new();

    // BGM 缓冲：立体声 i16 样本队列
    let mut bgm_buf: Vec<i16> = Vec::new();
    let mut monitor_was_streaming = false; // 流未在初始化时启动，由主循环控制
    let mut consecutive_zero_output: u32 = 0; // 模型损坏检测计数器

    let mut loop_iteration: u64 = 0;
    let mut consecutive_zero_reads: u32 = 0;
    let mut consecutive_read_errors: u32 = 0;
    let mut last_monitor_check_iteration: u64 = 0;
    const MONITOR_CHECK_INTERVAL: u64 = 100; // 每 100 次循环检测一次（约 1 秒）
    while running.load(Ordering::Acquire) {
        if input_handle.wait_for_event(100).is_err() {
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        }

        loop_iteration += 1;
        let current_config = config.read().clone();

        // ====== 监听设备自动跟随：检测系统默认输出设备是否变化 ======
        if loop_iteration - last_monitor_check_iteration >= MONITOR_CHECK_INTERVAL {
            last_monitor_check_iteration = loop_iteration;
            // 只有初始化时成功设置过监听设备才检测，避免初始化失败时反复触发重启
            if !current_monitor_device_id.is_empty() {
                if let Ok(new_default_output) = find_device(None, false) {
                    let new_id = new_default_output.get_id().unwrap_or_default();
                    if new_id != current_monitor_device_id && !new_id.is_empty() {
                        log::info!("Monitor device changed: '{}' -> '{}'",
                            current_monitor_device_id, new_id);
                        debug_log(&format!("Monitor: device changed from '{}' to '{}', requesting restart",
                            current_monitor_device_id, new_id));
                        // 通知前端重启引擎
                        if let Some(ref app) = _app_handle {
                            let _ = app.emit("restart-needed", ());
                        }
                    }
                }
            }
        }

        // DeepFilterNet: strength 变化时更新内部降噪强度
        if (current_config.strength - last_strength).abs() > f32::EPSILON {
            denoise.update_strength(current_config.strength);
            last_strength = current_config.strength;
        }

        match input_capture.read_from_device(&mut input_buffer) {
            Ok((frames_read, _flags)) => {
                consecutive_read_errors = 0; // 重置错误计数
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

                let bytes_read = frames_read as usize * input_bytes_per_frame;
                let resampled = process_input(
                    &input_buffer[..bytes_read],
                    input_bits, &input_sample_type,
                    input_channels, input_sample_rate,
                );
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
                        let denoise_start = std::time::Instant::now();
                        denoise.process_frame(&mut output_frame, &input_frame);
                        let _denoise_ms = denoise_start.elapsed().as_micros() as f32 / 1000.0;

                        // 防止 denoise 模型输出 NaN/Inf 穿透链路
                        for s in output_frame.iter_mut() {
                            if !s.is_finite() {
                                *s = 0.0;
                            }
                        }

                        // 检测模型是否被回声打废：输入能量异常高（回声反馈）但降噪输出全零
                        // 阈值 1000：回声反馈时输入能量飙升到数千，正常键盘鼠标只有几百
                        let input_peak = input_frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                        let output_peak = output_frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                        if input_peak > 1000.0 && output_peak < 1.0 {
                            consecutive_zero_output += 1;
                            if consecutive_zero_output >= 10 {
                                log::warn!("Denoise model corrupted (in={:.0}, out={:.0}), rebuilding", input_peak, output_peak);
                                denoise = denoise::create_model(
                                    model_name.as_deref().unwrap_or("RNNoise"),
                                    resource_dir.as_deref()
                                );
                                eq_processor = EqProcessor::new(48000);
                                last_eq_config = None; // 强制重新应用配置
                                eq_config_dirty.store(false, Ordering::Release);
                                let eq_cfg = eq_config.read().clone();
                                if eq_cfg.enabled {
                                    eq_processor.apply_config(&eq_cfg);
                                    last_eq_config = Some(eq_cfg);
                                }
                                input_acc.clear();
                                // 清除当前模型的保存状态，防止损坏状态被重新加载（不清其他模型）
                                if let Ok(mut states) = saved_model_states.lock() {
                                    states.remove(&current_model_name);
                                }
                                consecutive_zero_output = 0;
                            }
                        } else {
                            consecutive_zero_output = 0;
                        }
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

                    // Apply strength mixing (仅对没有内部强度控制的模型生效)
                    // DeepFilterNet 通过 atten_lim_db 内部控制强度，不需要外部混音
                    let mixed_samples: Vec<f32> =
                        if current_config.enabled && current_config.strength < 1.0 && !denoise.has_internal_strength_control() {
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

                    // ====== 诊断：增益前峰值 ======
                    let pre_gain_peak = mixed_samples.iter()
                        .map(|s| s.abs()).fold(0.0f32, f32::max);

                    // ============ 监听流启停控制（必须在写入之前）============
                    let current_monitor_point = monitor_point.load(Ordering::Relaxed);
                    let monitor_wants = monitor_enabled.load(Ordering::Relaxed) && current_monitor_point > 0;
                    if monitor_wants {
                        if !monitor_was_streaming {
                            if let Some(ref mut m_client) = monitor_client_opt {
                                let _ = m_client.start_stream();
                            }
                            monitor_was_streaming = true;
                        }
                    } else if monitor_was_streaming {
                        if let Some(ref mut m_client) = monitor_client_opt {
                            let _ = m_client.stop_stream();
                        }
                        monitor_was_streaming = false;
                    }

                    // 监听点 1=原始输入（降噪前的 chunk）
                    if monitor_wants && current_monitor_point == 1 {
                        write_to_monitor(&chunk, &monitor_render_opt, &monitor_event_opt, &mut monitor_buffer, monitor_sample_rate);
                    }

                    // 监听点 2=降噪后（strength mixing 后，增益前）
                    if monitor_wants && current_monitor_point == 2 {
                        write_to_monitor(&mixed_samples, &monitor_render_opt, &monitor_event_opt, &mut monitor_buffer, monitor_sample_rate);
                    }

                    // 应用麦克风增益（在降噪后，避免放大噪音）
                    let mic_gain = current_config.mic_gain;
                    let mixed_samples: Vec<f32> = mixed_samples.iter().map(|s| s * mic_gain).collect();

                    // ====== 诊断：增益后峰值 ======
                    let post_gain_peak = mixed_samples.iter()
                        .map(|s| s.abs()).fold(0.0f32, f32::max);

                    // 监听点 3=增益后（EQ 前）
                    if monitor_wants && current_monitor_point == 3 {
                        write_to_monitor(&mixed_samples, &monitor_render_opt, &monitor_event_opt, &mut monitor_buffer, monitor_sample_rate);
                    }

                    // ============ EQ 均衡器 ============
                    // 仅在配置变化时读取 RwLock + 重算系数（避免每帧 clone + lock 开销）
                    if eq_config_dirty.swap(false, Ordering::Acquire) {
                        last_eq_config = Some(eq_config.read().clone());
                        if let Some(ref eq) = last_eq_config {
                            if eq.enabled {
                                eq_processor.apply_config(eq);
                            }
                        }
                    }
                    let current_eq = last_eq_config.clone().unwrap_or_default();
                    let mixed_samples = if current_eq.enabled {
                        let mut frame = mixed_samples;
                        eq_processor.process_frame(&mut frame);
                        // EQ biquad 滤波器在极端参数下可能输出 NaN/Inf
                        for s in frame.iter_mut() {
                            if !s.is_finite() {
                                *s = 0.0;
                            }
                        }
                        frame
                    } else {
                        last_eq_config = None; // EQ 关闭时清除跟踪，重新开启时重算系数
                        mixed_samples
                    };

                    // ====== 诊断：EQ 后峰值 ======
                    let post_eq_peak = mixed_samples.iter()
                        .map(|s| s.abs()).fold(0.0f32, f32::max);

                    // 监听点 4=EQ后
                    if monitor_wants && current_monitor_point == 4 {
                        write_to_monitor(&mixed_samples, &monitor_render_opt, &monitor_event_opt, &mut monitor_buffer, monitor_sample_rate);
                    }

                    // ============ 💥 爆炸模式：方波失真 ============
                    let mut mixed_samples = if explode_enabled.load(Ordering::Relaxed) {
                        let intensity = explode_intensity.load(Ordering::Relaxed) as f32;
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

                    // 爆炸模式输出 NaN/Inf 检查（防止污染后续 BGM 混音）
                    for sample in mixed_samples.iter_mut() {
                        if !sample.is_finite() {
                            *sample = 0.0;
                        }
                    }

                    // ============ BGM 混音 ============
                    let final_samples = if bgm_running.load(Ordering::Acquire) {
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

                        // 诊断：BGM buffer 峰值
                        if frame_count % 480 == 0 {
                            let bgm_peak = bgm_buf.iter().map(|s| s.abs()).fold(0i16, i16::max);
                            let gain = f32::from_bits(bgm_gain.load(Ordering::Acquire));
                            debug_log(&format!("BGM MIX: buf_len={} peak={} gain={:.2}", bgm_buf.len(), bgm_peak, gain));
                        }

                        let bgm_gain_val = f32::from_bits(bgm_gain.load(Ordering::Acquire));

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
                                mic_sample + bgm_mono * bgm_gain_val
                            })
                            .collect()
                    } else {
                        mixed_samples
                    };

                    // 消费已使用的 BGM 样本（仅在 BGM 运行时 drain，否则保留数据）
                    if bgm_running.load(Ordering::Acquire) {
                        let used = final_samples.len() * 2;
                        if used <= bgm_buf.len() {
                            bgm_buf.drain(..used);
                        } else {
                            bgm_buf.clear();
                        }
                    }

                    // 每秒记录 BGM 缓冲区状态，诊断漂移
                    if frame_count % 480 == 0 && bgm_running.load(Ordering::Acquire) {
                        debug_log(&format!("BGM buf level: {} samples ({:.1}ms)", bgm_buf.len(), bgm_buf.len() as f64 / 48000.0 * 1000.0));
                    }

                    // ====== 诊断：BGM 混音后峰值 ======
                    let post_bgm_peak = final_samples.iter()
                        .map(|s| s.abs()).fold(0.0f32, f32::max);

                    // ============ Soft limiter ============
                    let mut limited_samples = final_samples;
                    if !explode_enabled.load(Ordering::Relaxed) {
                        for sample in limited_samples.iter_mut() {
                            // 先处理 NaN/Inf
                            if !sample.is_finite() {
                                *sample = 0.0;
                                continue;
                            }
                            let level = sample.abs();
                            if level > 24000.0 {
                                let excess = level - 24000.0;
                                let compressed = 24000.0 + excess * 0.1;
                                let sign = if *sample > 0.0 { 1.0 } else { -1.0 };
                                *sample = sign * compressed.min(30000.0);
                            }
                        }
                    } else {
                        // 爆炸模式也要防 NaN/Inf
                        for sample in limited_samples.iter_mut() {
                            if !sample.is_finite() {
                                *sample = 0.0;
                            }
                        }
                    }

                    // ====== 诊断：limiter 后峰值 ======
                    let output_peak = limited_samples.iter()
                        .map(|s| s.abs()).fold(0.0f32, f32::max);

                    // 监听点 5=最终输出（limiter 后）
                    if monitor_wants && current_monitor_point == 5 {
                        write_to_monitor(&limited_samples, &monitor_render_opt, &monitor_event_opt, &mut monitor_buffer, monitor_sample_rate);
                    }

                    // 6. 重采样回设备采样率 + 7. 单声道→多声道 + 8. f32→字节
                    let resampled_out = if output_sample_rate != 48000 {
                        resample_linear(&limited_samples, 48000, output_sample_rate)
                    } else {
                        limited_samples.clone()
                    };
                    let out_bytes = format_output_bytes(
                        &resampled_out, &mut output_buffer,
                        output_channels, output_bytes_per_frame,
                        output_bits, &output_sample_type,
                    );

                    // ====== 输出：发送到输出线程 ======
                    if output_tx.try_send(output_buffer[..out_bytes].to_vec()).is_err() {
                        // channel 满或已关闭，丢弃当前帧
                        if frame_count % 2400 == 0 {
                            debug_log("output channel full or closed, dropped frame");
                        }
                    }

                    frame_count += 1;

                    // ====== 诊断日志 ======
                    log_diagnostics(
                        frame_count, &input_frame,
                        &PeakDiagnostics { pre_gain: pre_gain_peak, post_gain: post_gain_peak, post_eq: post_eq_peak, post_bgm: post_bgm_peak, output: output_peak },
                        mic_gain, current_eq.enabled, bgm_running.load(Ordering::Acquire),
                    );

                    // 监听设备变更检测已移至输出线程
                    // 异常告警：任一阶段峰值超过安全阈值，立即记录
                    let alarm = post_gain_peak > 24000.0 || post_eq_peak > 24000.0
                        || post_bgm_peak > 24000.0 || output_peak > 24000.0;
                    if alarm {
                        debug_log(&format!(
                            "ALARM frame={} | in={:.0} preG={:.0} postG={:.0} postEQ={:.0} postBGM={:.0} out={:.0} | gain={:.1} eq={} bgm={}",
                            frame_count, input_frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max),
                            pre_gain_peak, post_gain_peak, post_eq_peak, post_bgm_peak, output_peak,
                            mic_gain, current_eq.enabled, bgm_running.load(Ordering::Acquire)
                        ));
                    }

                    // 延迟测量（每帧更新，供前端显示）
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

                        let input_padding = input_client.get_current_padding().unwrap_or(0);
                        let input_latency = input_padding as f32 / input_sample_rate as f32 * 1000.0;
                        // 输出延迟估算（output_client 在输出线程中，无法直接查询）
                        let output_latency = frame_size as f32 / output_sample_rate as f32 * 1000.0;

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
                consecutive_read_errors += 1;
                log::warn!("Failed to read input: {} (consecutive: {})", e, consecutive_read_errors);
                // 连续读取失败超过 10 次，退出循环（cleanup 会关闭 output channel 停止输出线程）
                if consecutive_read_errors >= 10 {
                    debug_log(&format!("Input device disconnected ({} errors), stopping", consecutive_read_errors));
                    if let Some(ref m_client) = monitor_client_opt {
                        let _ = m_client.stop_stream();
                    }
                    break;
                }
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
    if let Some(m_client) = monitor_client_opt {
        m_client.stop_stream().ok();
    }

    // 关闭输出 channel，通知输出线程退出
    drop(output_tx);
    // 等待输出线程退出
    let _ = output_handle.join();

    Ok(())
}
