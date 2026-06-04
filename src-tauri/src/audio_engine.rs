use crossbeam_channel::{bounded, Receiver, Sender};
use nnnoiseless::DenoiseState;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use wasapi::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenoiseConfig {
    pub enabled: bool,
    pub strength: f32,
    pub suppress_level: f32,
}

impl Default for DenoiseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: 0.5,
            suppress_level: 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BgmConfig {
    pub process_name: Option<String>,
    pub process_pid: Option<u32>,
    pub volume: f32,
    pub enabled: bool,
}

impl Default for BgmConfig {
    fn default() -> Self {
        Self {
            process_name: None,
            process_pid: None,
            volume: 0.3,
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
    stats: Arc<RwLock<AudioStats>>,
    bgm_running: Arc<AtomicBool>,
    bgm_config: Arc<RwLock<BgmConfig>>,
    bgm_sender: Sender<Vec<i16>>,
    bgm_receiver: Receiver<Vec<i16>>,
    explode_enabled: Arc<AtomicBool>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (bgm_tx, bgm_rx) = bounded::<Vec<i16>>(2);
        Self {
            running: Arc::new(AtomicBool::new(false)),
            config: Arc::new(RwLock::new(DenoiseConfig::default())),
            stats: Arc::new(RwLock::new(AudioStats::default())),
            bgm_running: Arc::new(AtomicBool::new(false)),
            bgm_config: Arc::new(RwLock::new(BgmConfig::default())),
            bgm_sender: bgm_tx,
            bgm_receiver: bgm_rx,
            explode_enabled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(
        &self,
        input_device_name: Option<String>,
        output_device_name: Option<String>,
    ) -> Result<(), String> {
        if self.running.load(Ordering::Relaxed) {
            return Err("Engine is already running".to_string());
        }

        let running = self.running.clone();
        let config = self.config.clone();
        let stats = self.stats.clone();
        let bgm_config = self.bgm_config.clone();
        let bgm_running = self.bgm_running.clone();
        let bgm_receiver = self.bgm_receiver.clone();
        let explode_enabled = self.explode_enabled.clone();

        running.store(true, Ordering::Relaxed);

        std::thread::spawn(move || {
            if let Err(e) = audio_loop(
                running,
                config,
                stats,
                bgm_running,
                bgm_config,
                Some(bgm_receiver),
                input_device_name,
                output_device_name,
                explode_enabled,
            ) {
                log::error!("Audio engine error: {}", e);
            }
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        self.stop_bgm();
    }

    pub fn update_config(&self, config: DenoiseConfig) {
        *self.config.write() = config;
    }

    pub fn get_stats(&self) -> AudioStats {
        self.stats.read().clone()
    }

    /// 列出正在播放音频的进程（用于 BGM Loopback）
    pub fn list_audio_processes(&self) -> Result<Vec<(String, String, u32)>, String> {
        list_audio_processes()
    }

    /// 启动 BGM 捕获线程（按进程 PID）
    pub fn start_bgm(&self, process_name: String, pid: u32) -> Result<(), String> {
        // 如果旧线程还在运行，先停止并等待退出
        if self.bgm_running.load(Ordering::Relaxed) {
            self.bgm_running.store(false, Ordering::Relaxed);
            // 给旧线程时间退出 WASAPI（最多 300ms）
            std::thread::sleep(Duration::from_millis(300));
            // 清空 channel 中残留数据
            while self.bgm_receiver.try_recv().is_ok() {}
        }

        let sender = self.bgm_sender.clone();

        // 更新配置
        {
            let mut cfg = self.bgm_config.write();
            cfg.process_name = Some(process_name);
            cfg.process_pid = Some(pid);
            cfg.enabled = true;
        }

        let bgm_running = self.bgm_running.clone();
        bgm_running.store(true, Ordering::Relaxed);
        let bgm_config = self.bgm_config.clone();

        std::thread::Builder::new()
            .name("bgm-loopback".into())
            .spawn(move || {
                if let Err(e) = bgm_process_loop(bgm_running, bgm_config, sender, pid) {
                    log::error!("BGM process loopback error: {}", e);
                }
            })
            .map_err(|e| format!("Failed to spawn BGM thread: {}", e))?;

        Ok(())
    }

    /// 停止 BGM 捕获
    pub fn stop_bgm(&self) {
        self.bgm_running.store(false, Ordering::Relaxed);
        let mut cfg = self.bgm_config.write();
        cfg.enabled = false;
    }

    /// 更新 BGM 配置（音量等）
    pub fn update_bgm_config(&self, volume: f32) {
        let mut cfg = self.bgm_config.write();
        cfg.volume = volume.clamp(0.0, 1.0);
    }

    /// 设置爆炸模式
    pub fn set_explode_mode(&self, enabled: bool) {
        self.explode_enabled.store(enabled, Ordering::Relaxed);
    }
}

/// 列出正在播放音频的进程
#[cfg(windows)]
fn list_audio_processes() -> Result<Vec<(String, String, u32)>, String> {
    use std::collections::HashSet;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|e| format!("Failed to create snapshot: {}", e))?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..std::mem::zeroed()
        };

        let mut processes = Vec::new();
        let mut seen_apps: HashSet<String> = HashSet::new();

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0)],
                );
                let pid = entry.th32ProcessID;

                // 只显示常见音乐/视频播放器进程
                let lower = name.to_lowercase();
                let is_media = lower.contains("cloudmusic")      // 网易云
                    || lower.contains("qqmusic")       // QQ 音乐
                    || lower.contains("kugou")         // 酷狗
                    || lower.contains("kuwo")          // 酷我
                    || lower.contains("kwmusic")       // 酷我
                    || lower.contains("spotify")       // Spotify
                    || lower.contains("foobar")        // foobar2000
                    || lower.contains("aimp")          // AIMP
                    || lower.contains("musicbee")      // MusicBee
                    || lower.contains("winamp")        // Winamp
                    || lower.contains("potplayer")     // PotPlayer
                    || lower.contains("vlc")           // VLC
                    || lower.contains("mpv");          // mpv

                // 过滤掉 reporter/crash/helper 等辅助进程
                let is_helper = lower.contains("reporter")
                    || lower.contains("crash")
                    || lower.contains("helper")
                    || lower.contains("update")
                    || lower.contains("service")
                    || lower.contains("agent");

                // 按应用名去重，同一应用只显示一个（include_tree 会捕获所有子进程）
                if is_media && !is_helper {
                    let app_key = if lower.contains("cloudmusic") { "cloudmusic" }
                        else if lower.contains("qqmusic") { "qqmusic" }
                        else if lower.contains("kugou") { "kugou" }
                        else if lower.contains("kuwo") || lower.contains("kwmusic") { "kuwo" }
                        else if lower.contains("spotify") { "spotify" }
                        else if lower.contains("foobar") { "foobar" }
                        else if lower.contains("aimp") { "aimp" }
                        else if lower.contains("musicbee") { "musicbee" }
                        else if lower.contains("winamp") { "winamp" }
                        else if lower.contains("potplayer") { "potplayer" }
                        else if lower.contains("vlc") { "vlc" }
                        else if lower.contains("mpv") { "mpv" }
                        else { &lower };

                    if !seen_apps.contains(app_key) {
                        seen_apps.insert(app_key.to_string());
                        let friendly = match app_key {
                            "cloudmusic" => "网易云音乐",
                            "qqmusic" => "QQ 音乐",
                            "kugou" => "酷狗音乐",
                            "kuwo" => "酷我音乐",
                            "spotify" => "Spotify",
                            "foobar" => "foobar2000",
                            "aimp" => "AIMP",
                            "musicbee" => "MusicBee",
                            "winamp" => "Winamp",
                            "potplayer" => "PotPlayer",
                            "vlc" => "VLC",
                            "mpv" => "mpv",
                            _ => &name,
                        };
                        processes.push((friendly.to_string(), name, pid));
                    }
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = windows::Win32::Foundation::CloseHandle(snapshot);
        Ok(processes)
    }
}

#[cfg(not(windows))]
fn list_audio_processes() -> Result<Vec<(String, String, u32)>, String> {
    Ok(vec![])
}

/// BGM 按进程捕获线程：使用 WASAPI Process Loopback API
fn bgm_process_loop(
    running: Arc<AtomicBool>,
    _config: Arc<RwLock<BgmConfig>>,
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

/// 主音频循环：麦克风采集 → 降噪 → 混音（可选 BGM）→ 输出
fn audio_loop(
    running: Arc<AtomicBool>,
    config: Arc<RwLock<DenoiseConfig>>,
    stats: Arc<RwLock<AudioStats>>,
    bgm_running: Arc<AtomicBool>,
    bgm_config: Arc<RwLock<BgmConfig>>,
    bgm_receiver: Option<Receiver<Vec<i16>>>,
    input_device_name: Option<String>,
    output_device_name: Option<String>,
    explode_enabled: Arc<AtomicBool>,
) -> Result<(), String> {
    let _ = initialize_mta().ok();

    let input_device = find_device(input_device_name.as_deref(), true)
        .map_err(|e| format!("Failed to find input device: {}", e))?;
    let output_device = find_device(output_device_name.as_deref(), false)
        .map_err(|e| format!("Failed to find output device: {}", e))?;

    let input_format = WaveFormat::new(16, 16, &SampleType::Int, 48000, 1, None);
    let output_format = WaveFormat::new(16, 16, &SampleType::Int, 48000, 2, None);

    let mut input_client = input_device
        .get_iaudioclient()
        .map_err(|e| format!("Failed to get input client: {}", e))?;

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

    let _output_handle = output_client
        .set_get_eventhandle()
        .map_err(|e| format!("Failed to set output event handle: {}", e))?;

    let input_capture = input_client
        .get_audiocaptureclient()
        .map_err(|e| format!("Failed to get input capture client: {}", e))?;

    let output_render = output_client
        .get_audiorenderclient()
        .map_err(|e| format!("Failed to get output render client: {}", e))?;

    input_client
        .start_stream()
        .map_err(|e| format!("Failed to start input: {}", e))?;
    output_client
        .start_stream()
        .map_err(|e| format!("Failed to start output: {}", e))?;

    let frame_size = DenoiseState::FRAME_SIZE;
    let mut denoise = DenoiseState::new();
    let mut first_frame = true;
    let mut input_buffer = vec![0u8; frame_size * 2];
    let mut output_buffer = vec![0u8; frame_size * 4];
    let mut frame_count: u64 = 0;

    // BGM 缓冲：立体声 i16 样本队列
    let mut bgm_buf: Vec<i16> = Vec::new();

    while running.load(Ordering::Relaxed) {
        if input_handle.wait_for_event(100).is_err() {
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        }

        let current_config = config.read().clone();

        match input_capture.read_from_device(&mut input_buffer) {
            Ok((frames_read, _flags)) => {
                if frames_read == 0 {
                    continue;
                }

                let bytes_read = frames_read as usize * 2;
                let samples_i16: Vec<i16> = input_buffer[..bytes_read]
                    .chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                // nnnoiseless expects [-32768, 32767] range
                let mut input_frame = [0.0f32; 480];
                for (i, &s) in samples_i16.iter().enumerate().take(frame_size) {
                    input_frame[i] = s as f32;
                }

                let mut output_frame = [0.0f32; 480];
                denoise.process_frame(&mut output_frame, &input_frame);

                // Skip first frame (fade-in artifact)
                let output_samples = if first_frame {
                    first_frame = false;
                    samples_i16.iter().map(|&s| s as f32).collect()
                } else if current_config.enabled {
                    output_frame.iter().copied().collect()
                } else {
                    samples_i16.iter().map(|&s| s as f32).collect()
                };

                // Apply strength mixing
                let mixed_samples: Vec<f32> =
                    if current_config.enabled && current_config.strength < 1.0 {
                        output_frame
                            .iter()
                            .zip(samples_i16.iter())
                            .map(|(&denoised, &original)| {
                                let orig = original as f32;
                                orig * (1.0 - current_config.strength)
                                    + denoised * current_config.strength
                            })
                            .collect()
                    } else {
                        output_samples
                    };

                // ============ 💥 爆炸模式：方波失真 ============
                let mixed_samples = if explode_enabled.load(Ordering::Relaxed) {
                    mixed_samples
                        .iter()
                        .map(|&s| {
                            // 超高增益，正常说话直接顶满
                            let boosted = s * 50.0;
                            // 方波削波：±500 硬截断，几乎把所有波形变成方波
                            let clipped = boosted.clamp(-500.0, 500.0);
                            // 再放大回满幅，保持极响
                            clipped * 65.0
                        })
                        .collect()
                } else {
                    mixed_samples
                };

                // ============ BGM 混音 ============
                let final_samples = if bgm_running.load(Ordering::Relaxed) {
                    // 收取所有可用的 BGM 样本
                    if let Some(ref rx) = bgm_receiver {
                        while let Ok(bgm_samples) = rx.try_recv() {
                            bgm_buf.extend_from_slice(&bgm_samples);
                        }
                    }
                    // 限制 BGM 缓冲不超过 5 帧（~50ms），防止延迟累积
                    let max_buf = frame_size * 2 * 5;
                    if bgm_buf.len() > max_buf {
                        let drain = bgm_buf.len() - max_buf;
                        bgm_buf.drain(..drain);
                    }

                    let bgm_vol_raw = bgm_config.read().volume;
                    // 平方根曲线：低音量更精细，高音量压缩，防止突然爆音
                    let bgm_vol = bgm_vol_raw.sqrt() * 0.4; // 最大增益 40%

                    mixed_samples
                        .iter()
                        .enumerate()
                        .map(|(i, &mic_sample)| {
                            if bgm_buf.is_empty() {
                                return mic_sample;
                            }
                            // BGM 是立体声，每个样本对应左右声道
                            // 麦克风是单声道，取左右平均
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

                            mic_sample + bgm_mono * bgm_vol
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

                // ============ Soft limiter（爆炸模式下跳过，让 hard clip 产生失真）============
                let mut limited_samples = final_samples;
                if !explode_enabled.load(Ordering::Relaxed) {
                    for sample in limited_samples.iter_mut() {
                        let level = sample.abs();
                        if level > 28000.0 {
                            // Soft knee: 超过 28000 后渐进压缩
                            let excess = level - 28000.0;
                            let compressed = 28000.0 + excess * 0.2;
                            let sign = if *sample > 0.0 { 1.0 } else { -1.0 };
                            *sample = sign * compressed.min(32767.0);
                        }
                    }
                }

                // Convert to i16 for output
                let output_i16: Vec<i16> = limited_samples
                    .iter()
                    .map(|&s| s.clamp(-32768.0, 32767.0) as i16)
                    .collect();

                // Mono to stereo
                for (i, &sample) in output_i16.iter().enumerate() {
                    let bytes = sample.to_le_bytes();
                    output_buffer[i * 4] = bytes[0];
                    output_buffer[i * 4 + 1] = bytes[1];
                    output_buffer[i * 4 + 2] = bytes[0];
                    output_buffer[i * 4 + 3] = bytes[1];
                }

                let _ = output_render.write_to_device(
                    frames_read as usize,
                    &output_buffer[..frames_read as usize * 4],
                    None,
                );

                frame_count += 1;

                if frame_count % 5 == 0 {
                    let input_f32: Vec<f32> = samples_i16.iter().map(|&s| s as f32).collect();
                    let input_rms = calculate_rms(&input_f32);
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

                    let mut current_stats = stats.write();
                    current_stats.input_level = input_level_db;
                    current_stats.output_level = output_level_db;
                    current_stats.noise_reduction_db = input_level_db - output_level_db;
                    current_stats.frames_processed = frame_count;
                    current_stats.spectrum = compute_spectrum(&input_f32, 32);
                }
            }
            Err(e) => {
                log::warn!("Failed to read input: {}", e);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    input_client.stop_stream().ok();
    output_client.stop_stream().ok();

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
