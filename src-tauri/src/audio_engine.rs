use crossbeam_channel::{bounded, Receiver, Sender};
use crate::denoise::{self, FRAME_SIZE};
use crate::eq::{EqConfig, EqProcessor};
use crate::explode::{ExplodeEffect, ExplodeState};
use crate::audio_init::{init_audio_devices, init_monitor, warmup_streams};
use crate::audio_process::{FrameState, FrameDeps, process_frame};
use crate::explode::ExplodeAudioState;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use wasapi::*;
pub use crate::debug::{debug_log, flush_debug_log};
use crate::device::find_device;
pub use crate::audio_utils::{bytes_to_f32_samples_into, downmix_to_mono_into, resample_in_place};
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
    pub spectrum: [f32; 32],
    pub frames_dropped: u64,
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
    explode_state: Arc<ExplodeState>,
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
            explode_state: Arc::new(ExplodeState::new()),
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
        let explode_state = self.explode_state.clone();
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
                explode_state,
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
        self.explode_enabled.store(enabled, Ordering::Release);
        self.explode_state.enabled.store(enabled, Ordering::Release);
    }

    /// 设置爆炸强度 (1-100)，所有效果共用此滑块
    pub fn set_explode_intensity(&self, intensity: u32) {
        let val = intensity.clamp(1, 100);
        self.explode_state.intensity.store(val, Ordering::Relaxed);
    }

    /// 设置爆炸效果类型
    pub fn set_explode_effect(&self, effect: ExplodeEffect) {
        self.explode_state.effect_type.store(effect as u32, Ordering::Relaxed);
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

// ====== 音频处理链：将原始字节转为 f32 mono 48kHz（零分配，写入预分配 buffer）======
fn process_input(
    raw_bytes: &[u8],
    bits: u16,
    sample_type: &SampleType,
    channels: usize,
    sample_rate: u32,
    work_a: &mut [f32],
    work_b: &mut [f32],
    output: &mut [f32],
) -> usize {
    let n = bytes_to_f32_samples_into(raw_bytes, bits, sample_type, channels, work_a);
    let m = downmix_to_mono_into(&work_a[..n], channels, work_b);
    if sample_rate != 48000 {
        resample_in_place(&work_b[..m], sample_rate, 48000, output)
    } else {
        let len = m.min(output.len());
        output[..len].copy_from_slice(&work_b[..len]);
        len
    }
}

// ====== 音频处理辅助结构体 ======

/// WASAPI 输出资源（跨线程传递用）
/// wasapi 的 COM 对象和 Handle 不实现 Send，但 COM MTA 模式下跨线程安全
struct OutputResources {
    client: AudioClient,
    render: AudioRenderClient,
    event: wasapi::Handle,
}
unsafe impl Send for OutputResources {}

/// 输出线程：按 wasapi-rs 官方 playsine 示例模式
/// start_stream → 循环(get_available → write → wait_event)
fn output_thread(
    running: Arc<AtomicBool>,
    receiver: Receiver<Vec<u8>>,
    return_tx: Sender<Vec<u8>>,
    res: OutputResources,
    output_bytes_per_frame: usize,
    frame_size: usize,
    bgm_skip_rate: Arc<AtomicU32>,
) {
    let _ = initialize_mta().ok();

    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(hThread: isize, nPriority: i32) -> i32;
        }
        SetThreadPriority(GetCurrentThread(), 15);
    }

    // 按官方示例：先 start_stream，循环内先写再等
    if let Err(e) = res.client.start_stream() {
        debug_log(&format!("output_thread: start_stream failed: {:?}", e));
        return;
    }
    debug_log("output_thread: started");

    let mut output_buf: Vec<u8> = Vec::with_capacity(frame_size * output_bytes_per_frame * 4);
    let mut silence_buf: Vec<u8> = vec![0u8; frame_size * output_bytes_per_frame * 4];
    let mut read_pos: usize = 0;
    let mut write_ok_count: u32 = 0;
    let mut write_err_count: u32 = 0;
    let mut stats_start = std::time::Instant::now();
    let frame_bytes = frame_size * output_bytes_per_frame;

    while running.load(Ordering::Acquire) {
        // 1. 收集 channel 中所有可用数据，归还 Vec
        while let Ok(data) = receiver.try_recv() {
            output_buf.extend_from_slice(&data);
            let _ = return_tx.try_send(data);
        }

        // 2. 获取可用空间（首次调用返回整个缓冲区，自然填满）
        let available = match res.client.get_available_space_in_frames() {
            Ok(n) if n > 0 => n as usize,
            _ => {
                // 无可用空间，等一下再试
                if res.event.wait_for_event(10).is_err() {
                    continue;
                }
                continue;
            }
        };

        let valid_len = output_buf.len() - read_pos;
        let valid_frames = valid_len / output_bytes_per_frame;

        // 3. 写满可用空间（有数据写数据，没数据写静音）
        if valid_frames > 0 {
            let frames_to_write = valid_frames.min(available);
            let write_bytes = frames_to_write * output_bytes_per_frame;
            if write_bytes > 0 {
                match res.render.write_to_device(frames_to_write, &output_buf[read_pos..read_pos + write_bytes], None) {
                    Ok(()) => {
                        write_ok_count += 1;
                        read_pos += write_bytes;
                        if read_pos == output_buf.len() {
                            output_buf.clear();
                            read_pos = 0;
                        } else if read_pos > output_buf.len() / 2 {
                            output_buf.drain(..read_pos);
                            read_pos = 0;
                        }
                    }
                    Err(e) => {
                        write_err_count += 1;
                        if write_err_count <= 3 {
                            log::warn!("output_thread: write error: {:?}", e);
                        }
                    }
                }
            }
        } else {
            // 无数据：写满静音，维持 WASAPI 时钟
            let silence_bytes = available * output_bytes_per_frame;
            if silence_buf.len() < silence_bytes {
                silence_buf.resize(silence_bytes, 0);
            }
            let _ = res.render.write_to_device(available, &silence_buf[..silence_bytes], None);
        }

        // 4. 溢出保护
        let max_buf = frame_bytes * 20;
        let pending = output_buf.len().saturating_sub(read_pos);
        if pending > max_buf {
            let excess = pending - max_buf;
            read_pos += excess;
            debug_log(&format!("output_thread: skipped {} bytes (overflow)", excess));
        }

        // 5. 统计
        if stats_start.elapsed().as_secs() >= 5 {
            let pending = (output_buf.len() - read_pos) / output_bytes_per_frame;
            debug_log(&format!(
                "output_thread stats: wrote={} err={} pending_frames={}",
                write_ok_count, write_err_count, pending,
            ));
            write_ok_count = 0;
            write_err_count = 0;
            stats_start = std::time::Instant::now();
        }

        // 6. BGM 漂移补偿
        let pending_frames = (output_buf.len() - read_pos) / output_bytes_per_frame;
        let skip = if pending_frames > frame_size * 2 {
            0.05
        } else if pending_frames > frame_size {
            ((pending_frames - frame_size) as f32 / frame_size as f32 * 0.05).max(0.001)
        } else {
            0.0
        };
        bgm_skip_rate.store(skip.to_bits(), Ordering::Relaxed);

        // 7. 等待设备消费（官方模式：写完再等）
        let _ = res.event.wait_for_event(100);
    }

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
    explode_state: Arc<ExplodeState>,
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

    // 1. 初始化音频设备
    let devices = init_audio_devices(input_device_name.as_deref(), output_device_name.as_deref())?;

    let frame_size = FRAME_SIZE;
    let input_bytes_per_frame = devices.input_format.get_blockalign() as usize;
    let output_bytes_per_frame = devices.output_format.get_blockalign() as usize;
    let input_bits = devices.input_format.get_bitspersample();
    let input_sample_type = devices.input_format.get_subformat().unwrap_or(SampleType::Int);
    let input_channels = devices.input_format.get_nchannels() as usize;
    let input_sample_rate = devices.input_format.get_samplespersec();
    let output_sample_rate = devices.output_format.get_samplespersec();
    let output_channels = devices.output_format.get_nchannels() as usize;
    let output_bits = devices.output_format.get_bitspersample();
    let output_sample_type = devices.output_format.get_subformat().unwrap_or(SampleType::Int);

    // 提取引用供主循环使用
    let input_handle = &devices.input_handle;
    let input_capture = &devices.input_capture;

    // 2. 初始化监听
    let mut monitor = init_monitor(&devices.input_device_id, output_sample_rate, frame_size);

    // 3. 模型加载（在音频流启动之前，避免阻塞 WASAPI 导致炸麦）
    let load_start = std::time::Instant::now();
    let mut denoise = denoise::create_model(model_name.as_deref().unwrap_or("RNNoise"), resource_dir.as_deref());
    let load_elapsed = load_start.elapsed().as_millis();
    log::info!("Model '{}' loaded in {}ms", denoise.name(), load_elapsed);

    let current_model_name = denoise.name().to_string();
    if let Ok(states_lock) = saved_model_states.lock() {
        if let Some(state) = states_lock.get(&current_model_name) {
            log::info!("Restoring model '{}' state ({} bytes)", current_model_name, state.len());
            denoise.load_state(state);
        }
    }
    log::info!("Using denoise model: {}", current_model_name);

    // 4. EQ 处理器初始化
    let mut eq_processor = EqProcessor::new(48000);
    let mut last_eq_config: Option<EqConfig> = None;
    let current_eq_config = eq_config.read().clone();
    eq_config_dirty.store(false, Ordering::Release);
    if current_eq_config.enabled {
        eq_processor.apply_config(&current_eq_config);
        last_eq_config = Some(current_eq_config.clone());
        log::info!("EQ enabled with preset bands");
    }

    // 5. 启动输入流（先不启动输出流，避免 warmup 期间输出缓冲区欠载）
    devices.input_client
        .start_stream()
        .map_err(|e| format!("Failed to start input: {}", e))?;

    // 6. 预热（只预热输入流）
    warmup_streams(&devices.input_client, &devices.input_handle, &devices.input_capture, input_bytes_per_frame, frame_size);

    // 7. 不在这里启动输出流，由输出线程预填充后再启动

    // 8. 启动输出线程
    let (output_tx, output_rx) = bounded::<Vec<u8>>(10);
    let (return_tx, return_rx) = bounded::<Vec<u8>>(4);
    let output_res = OutputResources {
        client: devices.output_client,
        render: devices.output_render,
        event: devices.output_handle,
    };
    let bgm_skip_rate_out = bgm_skip_rate.clone();
    let running_out = running.clone();
    let output_handle = std::thread::Builder::new()
        .name("audio-output".into())
        .spawn(move || {
            output_thread(running_out, output_rx, return_tx, output_res, output_bytes_per_frame, frame_size, bgm_skip_rate_out);
        })
        .map_err(|e| format!("Failed to spawn output thread: {}", e))?;

    // 9. 初始化帧处理状态
    let max_output_ratio = (output_sample_rate as f64 / 48000.0).ceil() as usize;
    let max_output_frames = frame_size * max_output_ratio;
    let mut frame_state = FrameState {
        denoise,
        eq_processor,
        last_eq_config,
        consecutive_zero_output: 0,
        bgm_buf: Vec::new(),
        output_buffer: vec![0u8; max_output_frames * output_bytes_per_frame],
        frame_count: 0,
        frames_dropped: 0,
        monitor_client: monitor.client.take(),
        monitor_render: monitor.render.take(),
        monitor_event: monitor.event.take(),
        monitor_buffer: monitor.buffer,
        monitor_was_streaming: monitor.was_streaming,
        monitor_sample_rate: monitor.sample_rate,
        work_buf_a: vec![0.0f32; frame_size],
        work_buf_b: vec![0.0f32; frame_size],
        resample_buf: vec![0.0f32; max_output_frames],
        monitor_resample_buf: vec![0.0f32; max_output_frames],
        explode_audio: ExplodeAudioState::new(),
        input_read_pos: 0,
        clear_input_acc: false,
        input_work_a: vec![0.0f32; 1920], // max input per read
        input_work_b: vec![0.0f32; 1920],
        bgm_read_pos: 0,
        spectrum_buf: [0.0f32; 32],
        hp_x_prev: 0.0,
        hp_y_prev: 0.0,
    };

    let mut input_buffer = vec![0u8; frame_size * input_bytes_per_frame];
    let mut input_acc: Vec<f32> = Vec::with_capacity(frame_size * 4);
    // 输出 buffer 池：轮换使用，避免每帧 .to_vec() 堆分配
    let mut output_buf_pool: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; max_output_frames * output_bytes_per_frame]).collect();
    let mut output_buf_idx: usize = 0;
    let mut last_strength = -1.0f32;
    let mut loop_iteration: u64 = 0;
    let mut consecutive_zero_reads: u32 = 0;
    let mut consecutive_read_errors: u32 = 0;

    // 9. 监听设备自动跟随：后台线程检测
    let monitor_device_id_for_thread = monitor.current_device_id.clone();
    let app_handle_for_monitor = _app_handle.clone();
    let running_for_monitor = running.clone();
    let _monitor_watcher = if !monitor_device_id_for_thread.is_empty() {
        let current_id = std::sync::Arc::new(std::sync::Mutex::new(monitor_device_id_for_thread.clone()));
        std::thread::spawn(move || {
            while running_for_monitor.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_secs(2));
                if !running_for_monitor.load(Ordering::Acquire) {
                    break;
                }
                if let Ok(new_default_output) = find_device(None, false) {
                    if let Ok(new_id) = new_default_output.get_id() {
                        let id_str = new_id.to_string();
                        let mut last_id = current_id.lock().unwrap_or_else(|e| e.into_inner());
                        if id_str != *last_id && !id_str.is_empty() {
                            log::info!("Monitor device changed: '{}' -> '{}'", *last_id, id_str);
                            debug_log(&format!("Monitor: device changed from '{}' to '{}', requesting restart", *last_id, id_str));
                            *last_id = id_str;
                            drop(last_id);
                            if let Some(ref app) = app_handle_for_monitor {
                                let _ = app.emit("restart-needed", ());
                            }
                        }
                    }
                }
            }
            debug_log("Monitor watcher thread exited");
        });
        Some(())
    } else {
        None
    };

    // 10. 构建共享依赖
    let deps = FrameDeps {
        eq_config: &eq_config,
        eq_config_dirty: &eq_config_dirty,
        bgm_running: &bgm_running,
        bgm_receiver: &bgm_receiver,
        bgm_gain: &bgm_gain,
        monitor_enabled: &monitor_enabled,
        monitor_point: &monitor_point,
        explode_enabled: &explode_enabled,
        explode_state: &explode_state,
        stats: &stats,
        saved_model_states: &saved_model_states,
        model_name: model_name.as_deref(),
        resource_dir: resource_dir.as_deref(),
        current_model_name: &current_model_name,
        frame_size,
        output_sample_rate,
        output_channels,
        output_bytes_per_frame,
        output_bits,
        output_sample_type: &output_sample_type,
        input_client: &devices.input_client,
        input_sample_rate,
    };

    let mut slow_frame_logged = false;

    while running.load(Ordering::Acquire) {
        let iter_start = std::time::Instant::now();

        if input_handle.wait_for_event(100).is_err() {
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        }

        loop_iteration += 1;
        let t0 = iter_start.elapsed().as_micros();
        let current_config = config.read().clone();

        // DeepFilterNet: strength 变化时更新内部降噪强度
        if (current_config.strength - last_strength).abs() > f32::EPSILON {
            frame_state.denoise.update_strength(current_config.strength);
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

                let bytes_read = frames_read as usize * input_bytes_per_frame;

                // 检测输入数据异常：全零帧或连续相同值（USB 传输错误）
                let buf_slice = &input_buffer[..bytes_read];
                let all_zero = buf_slice.iter().all(|&b| b == 0);
                let all_same = buf_slice.len() > 4 && buf_slice.windows(2).all(|w| w[0] == w[1]);
                if all_zero || all_same {
                    debug_log(&format!("INPUT ABNORMAL: frames={} all_zero={} all_same={}, first_bytes={:?}",
                        frames_read, all_zero, all_same, &buf_slice[..buf_slice.len().min(16)]));
                }

                let resampled_len = process_input(
                    &input_buffer[..bytes_read],
                    input_bits, &input_sample_type,
                    input_channels, input_sample_rate,
                    &mut frame_state.input_work_a,
                    &mut frame_state.input_work_b,
                    &mut frame_state.resample_buf,
                );
                input_acc.extend_from_slice(&frame_state.resample_buf[..resampled_len]);

                // 每攒够 frame_size (480) 个样本就处理一帧
                let mut frames_processed_this_iter = 0u32;
                while input_acc.len() - frame_state.input_read_pos >= frame_size {
                    let start = frame_state.input_read_pos;
                    let mut chunk = [0.0f32; 480];
                    chunk.copy_from_slice(&input_acc[start..start + frame_size]);
                    frame_state.input_read_pos += frame_size;
                    let t_frame_start = std::time::Instant::now();
                    let out_bytes = process_frame(&chunk, &current_config, &mut frame_state, &deps);
                    let frame_us = t_frame_start.elapsed().as_micros();
                    frames_processed_this_iter += 1;
                    if frame_us > 12000 {
                        frame_state.frames_dropped += 1;
                        debug_log(&format!(
                            "SLOW FRAME: {}us frame={} dropped={} acc_len={} iter={}",
                            frame_us, frame_state.frame_count, frame_state.frames_dropped,
                            input_acc.len(), loop_iteration
                        ));
                    }
                    if out_bytes > 0 {
                        let mut buf = return_rx.try_recv().unwrap_or_else(|_| std::mem::take(&mut output_buf_pool[output_buf_idx]));
                        buf.clear();
                        buf.extend_from_slice(&frame_state.output_buffer[..out_bytes]);
                        if output_tx.try_send(buf).is_err() {
                            frame_state.frames_dropped += 1;
                            debug_log(&format!(
                                "OUTPUT DROP: channel full frame={} dropped={}",
                                frame_state.frame_count, frame_state.frames_dropped
                            ));
                        }
                        output_buf_idx = (output_buf_idx + 1) % output_buf_pool.len();
                    }
                }
                let iter_us = iter_start.elapsed().as_micros();
                if iter_us > 15000 && !slow_frame_logged {
                    debug_log(&format!("SLOW ITER: total={}us wait={}us frames={} iter={}",
                        iter_us, t0, frames_processed_this_iter, loop_iteration));
                    slow_frame_logged = true;
                } else if iter_us <= 15000 {
                    slow_frame_logged = false;
                }
                // 消费完毕，清空 acc（保留未处理的尾部）
                let t_drain_start = std::time::Instant::now();
                if frame_state.clear_input_acc {
                    input_acc.clear();
                    frame_state.input_read_pos = 0;
                    frame_state.clear_input_acc = false;
                } else if frame_state.input_read_pos > 0 {
                    input_acc.drain(..frame_state.input_read_pos);
                    frame_state.input_read_pos = 0;
                }
                let drain_us = t_drain_start.elapsed().as_micros();
                if drain_us > 5000 {
                    debug_log(&format!("SLOW DRAIN: {}us acc_len={} iter={}", drain_us, input_acc.len(), loop_iteration));
                }
                // 每 500 帧（约 5 秒）输出一次丢帧汇总
                if frame_state.frame_count % 500 == 0 && frame_state.frame_count > 0 {
                    let drop_rate = frame_state.frames_dropped as f64 / frame_state.frame_count as f64 * 100.0;
                    debug_log(&format!(
                        "HEALTH: frame={} dropped={} ({:.2}%) acc_len={} iter={}",
                        frame_state.frame_count, frame_state.frames_dropped,
                        drop_rate, input_acc.len(), loop_iteration
                    ));
                }
            }
            Err(e) => {
                consecutive_read_errors += 1;
                log::warn!("Failed to read input: {} (consecutive: {})", e, consecutive_read_errors);
                // 连续读取失败超过 10 次，退出循环（cleanup 会关闭 output channel 停止输出线程）
                if consecutive_read_errors >= 10 {
                    debug_log(&format!("Input device disconnected ({} errors), stopping", consecutive_read_errors));
                    if let Some(ref m_client) = frame_state.monitor_client {
                        let _ = m_client.stop_stream();
                    }
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    // 保存模型状态（归一化统计），供下次启动时恢复
    let model_save_name = frame_state.denoise.name().to_string();
    if let Some(s) = frame_state.denoise.save_state() {
        log::info!("Saving model '{}' state ({} bytes)", model_save_name, s.len());
        if let Ok(mut states_lock) = saved_model_states.lock() {
            states_lock.insert(model_save_name, s);
        }
    }

    // 清理 WASAPI 流
    devices.input_client.stop_stream().ok();
    if let Some(m_client) = frame_state.monitor_client {
        m_client.stop_stream().ok();
    }

    // 关闭输出 channel，通知输出线程退出
    drop(output_tx);
    // 等待输出线程退出
    let _ = output_handle.join();

    // Flush 调试日志 + 轮转（不在热路径做，避免磁盘 I/O 尖刺）
    flush_debug_log();

    Ok(())
}
