/// 音频处理模块
///
/// 包含降噪、增益、EQ、爆炸模式、BGM 混音、Soft limiter 等处理函数。

use crate::audio_engine::DenoiseConfig;
use crate::audio_utils::{calculate_rms, compute_spectrum, resample_in_place, write_to_monitor};
use crate::debug::debug_log;
use crate::denoise::{self, FRAME_SIZE};
use crate::eq::{EqConfig, EqProcessor};
use crate::explode::{ExplodeState, ExplodeAudioState, process_explode_into};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use wasapi::SampleType;

/// 帧处理状态（可变）
pub struct FrameState {
    pub denoise: Box<dyn denoise::DenoiseModel>,
    pub eq_processor: EqProcessor,
    pub last_eq_config: Option<EqConfig>,
    pub consecutive_zero_output: u32,
    pub bgm_buf: Vec<i16>,
    pub output_buffer: Vec<u8>,
    pub frame_count: u64,
    pub monitor_client: Option<wasapi::AudioClient>,
    pub monitor_render: Option<wasapi::AudioRenderClient>,
    pub monitor_event: Option<wasapi::Handle>,
    pub monitor_buffer: Vec<u8>,
    pub monitor_was_streaming: bool,
    pub monitor_sample_rate: u32,
    // 预分配的工作 buffer，避免每帧堆分配
    pub work_buf_a: Vec<f32>,
    pub work_buf_b: Vec<f32>,
    pub resample_buf: Vec<f32>,
    pub monitor_resample_buf: Vec<f32>,
    /// 爆炸模式音频线程独占状态（无锁）
    pub explode_audio: ExplodeAudioState,
    /// input_acc 内部读位置（避免 drain 的 O(n) memmove）
    pub input_read_pos: usize,
    /// 模型重建时置 true，audio_loop 清空 input_acc
    pub clear_input_acc: bool,
    /// process_input 预分配工作 buffer
    pub input_work_a: Vec<f32>,
    pub input_work_b: Vec<f32>,
}

/// 帧处理共享依赖（只读引用）
pub struct FrameDeps<'a> {
    pub eq_config: &'a Arc<parking_lot::RwLock<EqConfig>>,
    pub eq_config_dirty: &'a Arc<AtomicBool>,
    pub bgm_running: &'a Arc<AtomicBool>,
    pub bgm_receiver: &'a Option<crossbeam_channel::Receiver<Vec<i16>>>,
    pub bgm_gain: &'a Arc<AtomicU32>,
    pub monitor_enabled: &'a Arc<AtomicBool>,
    pub monitor_point: &'a Arc<AtomicU32>,
    pub explode_enabled: &'a Arc<AtomicBool>,
    pub explode_state: &'a Arc<ExplodeState>,
    pub stats: &'a Arc<parking_lot::RwLock<crate::AudioStats>>,
    pub saved_model_states: &'a Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    pub model_name: Option<&'a str>,
    pub resource_dir: Option<&'a std::path::Path>,
    pub current_model_name: &'a str,
    pub frame_size: usize,
    pub output_sample_rate: u32,
    pub output_channels: usize,
    pub output_bytes_per_frame: usize,
    pub output_bits: u16,
    pub output_sample_type: &'a SampleType,
    pub input_client: &'a wasapi::AudioClient,
    pub input_sample_rate: u32,
}

/// 音频帧诊断数据
pub struct PeakDiagnostics {
    pub pre_gain: f32,
    pub post_gain: f32,
    pub post_eq: f32,
    pub post_bgm: f32,
    pub output: f32,
}

/// 格式化输出字节
pub fn format_output_bytes(
    samples: &[f32],
    output_buffer: &mut [u8],
    output_channels: usize,
    output_bytes_per_frame: usize,
    output_bits: u16,
    output_sample_type: &SampleType,
) -> usize {
    let out_frames = samples.len() / output_channels;
    let out_bytes = out_frames * output_bytes_per_frame;

    match output_bits {
        16 => match output_sample_type {
            SampleType::Int => {
                for (i, frame) in samples.chunks(output_channels).enumerate() {
                    let offset = i * output_bytes_per_frame;
                    if offset + output_bytes_per_frame <= output_buffer.len() {
                        for (ch, &s) in frame.iter().enumerate() {
                            let clamped = s.max(-32768.0).min(32767.0) as i16;
                            let bytes = clamped.to_le_bytes();
                            let pos = offset + ch * 2;
                            if pos + 2 <= output_buffer.len() {
                                output_buffer[pos] = bytes[0];
                                output_buffer[pos + 1] = bytes[1];
                            }
                        }
                    }
                }
            }
            _ => {
                log::warn!("Unsupported 16-bit output format: {:?}", output_sample_type);
            }
        },
        32 => match output_sample_type {
            SampleType::Float => {
                for (i, frame) in samples.chunks(output_channels).enumerate() {
                    let offset = i * output_bytes_per_frame;
                    if offset + output_bytes_per_frame <= output_buffer.len() {
                        for (ch, &s) in frame.iter().enumerate() {
                            let bytes = s.to_le_bytes();
                            let pos = offset + ch * 4;
                            if pos + 4 <= output_buffer.len() {
                                output_buffer[pos] = bytes[0];
                                output_buffer[pos + 1] = bytes[1];
                                output_buffer[pos + 2] = bytes[2];
                                output_buffer[pos + 3] = bytes[3];
                            }
                        }
                    }
                }
            }
            SampleType::Int => {
                for (i, frame) in samples.chunks(output_channels).enumerate() {
                    let offset = i * output_bytes_per_frame;
                    if offset + output_bytes_per_frame <= output_buffer.len() {
                        for (ch, &s) in frame.iter().enumerate() {
                            let clamped = s.max(-2147483648.0).min(2147483647.0) as i32;
                            let bytes = clamped.to_le_bytes();
                            let pos = offset + ch * 4;
                            if pos + 4 <= output_buffer.len() {
                                output_buffer[pos] = bytes[0];
                                output_buffer[pos + 1] = bytes[1];
                                output_buffer[pos + 2] = bytes[2];
                                output_buffer[pos + 3] = bytes[3];
                            }
                        }
                    }
                }
            }
        },
        24 => match output_sample_type {
            SampleType::Int => {
                for (i, frame) in samples.chunks(output_channels).enumerate() {
                    let offset = i * output_bytes_per_frame;
                    if offset + output_bytes_per_frame <= output_buffer.len() {
                        for (ch, &s) in frame.iter().enumerate() {
                            let clamped = s.max(-8388608.0).min(8388607.0) as i32;
                            let bytes = clamped.to_le_bytes();
                            let pos = offset + ch * 3;
                            if pos + 3 <= output_buffer.len() {
                                output_buffer[pos] = bytes[0];
                                output_buffer[pos + 1] = bytes[1];
                                output_buffer[pos + 2] = bytes[2];
                            }
                        }
                    }
                }
            }
            _ => {
                log::warn!("Unsupported 24-bit output format: {:?}", output_sample_type);
            }
        },
        _ => {
            log::warn!("Unsupported output bit depth: {}", output_bits);
        }
    }

    out_bytes
}

/// 诊断日志
pub fn log_diagnostics(
    frame_count: u64,
    input_frame: &[f32; FRAME_SIZE],
    peaks: &PeakDiagnostics,
    mic_gain: f32,
    eq_enabled: bool,
    bgm_running: bool,
) {
    if frame_count % 480 == 0 {
        let input_rms = calculate_rms(input_frame);
        let input_level_db = if input_rms > 0.0 {
            20.0 * (input_rms / 32768.0).log10()
        } else {
            -100.0
        };

        debug_log(&format!(
            "frame={} in_rms={:.1}dB gain={:.1} eq={} bgm={} | peaks: preG={:.0} postG={:.0} postEQ={:.0} postBGM={:.0} out={:.0}",
            frame_count, input_level_db, mic_gain, eq_enabled, bgm_running,
            peaks.pre_gain, peaks.post_gain, peaks.post_eq, peaks.post_bgm, peaks.output
        ));
    }
}

/// 处理单帧音频：降噪 → strength mixing → 增益 → EQ → 爆炸 → BGM → limiter → 输出
/// 返回输出字节数（供 audio_loop 发送到输出线程）
pub fn process_frame(
    chunk: &[f32],
    current_config: &DenoiseConfig,
    state: &mut FrameState,
    deps: &FrameDeps,
) -> usize {
    let frame_start = std::time::Instant::now();

    let mut input_frame = [0.0f32; 480];
    let len = chunk.len().min(deps.frame_size);
    input_frame[..len].copy_from_slice(&chunk[..len]);

    let mut output_frame = input_frame;
    if current_config.enabled {
        let denoise_start = std::time::Instant::now();
        state.denoise.process_frame(&mut output_frame, &input_frame);
        let denoise_ms = denoise_start.elapsed().as_micros() as f32 / 1000.0;
        if denoise_ms > 8.0 {
            debug_log(&format!(
                "SLOW denoise: {:.1}ms frame={}",
                denoise_ms, state.frame_count
            ));
        }

        // 防止 denoise 模型输出 NaN/Inf 穿透链路
        for s in output_frame.iter_mut() {
            if !s.is_finite() {
                *s = 0.0;
            }
        }

        // 检测模型是否被回声打废：输入能量异常高（回声反馈）但降噪输出全零
        let input_peak = input_frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let output_peak = output_frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        if input_peak > 1000.0 && output_peak < 1.0 {
            state.consecutive_zero_output += 1;
            if state.consecutive_zero_output >= 10 {
                log::warn!(
                    "Denoise model corrupted (in={:.0}, out={:.0}), rebuilding",
                    input_peak,
                    output_peak
                );
                state.denoise = denoise::create_model(
                    deps.model_name.unwrap_or("RNNoise"),
                    deps.resource_dir,
                );
                state.eq_processor = EqProcessor::new(48000);
                state.last_eq_config = None;
                deps.eq_config_dirty.store(false, Ordering::Release);
                let eq_cfg = deps.eq_config.read().clone();
                if eq_cfg.enabled {
                    state.eq_processor.apply_config(&eq_cfg);
                    state.last_eq_config = Some(eq_cfg);
                }
                state.clear_input_acc = true;
                if let Ok(mut states) = deps.saved_model_states.lock() {
                    states.remove(deps.current_model_name);
                }
                state.consecutive_zero_output = 0;
            }
        } else {
            state.consecutive_zero_output = 0;
        }

        // Strength mixing: 混合原始信号和降噪信号
        if current_config.strength < 1.0
            && !state.denoise.has_internal_strength_control()
        {
            output_frame
                .iter_mut()
                .zip(input_frame.iter())
                .for_each(|(denoised, &original)| {
                    *denoised =
                        original * (1.0 - current_config.strength) + *denoised * current_config.strength;
                });
        }
    }

    // ============ 监听流启停控制（必须在写入之前）============
    let monitor_enabled = deps.monitor_enabled.load(Ordering::Relaxed);
    let monitor_point = deps.monitor_point.load(Ordering::Relaxed);
    let monitor_wants = monitor_enabled && monitor_point > 0;
    let current_monitor_point = monitor_point;

    // 每 1000 帧记录一次监听状态
    if state.frame_count % 1000 == 0 {
        debug_log(&format!(
            "Monitor state: enabled={} point={} wants={} was_streaming={} has_client={}",
            monitor_enabled, current_monitor_point, monitor_wants,
            state.monitor_was_streaming, state.monitor_client.is_some()
        ));
    }

    if monitor_wants {
        if !state.monitor_was_streaming {
            if let Some(ref mut m_client) = state.monitor_client {
                let _ = m_client.start_stream();
                debug_log(&format!("Monitor: started streaming at point {}", current_monitor_point));
            }
            state.monitor_was_streaming = true;
        }
    } else if state.monitor_was_streaming {
        if let Some(ref mut m_client) = state.monitor_client {
            let _ = m_client.stop_stream();
            debug_log("Monitor: stopped streaming");
        }
        state.monitor_was_streaming = false;
    }

    // 监听点 1=原始输入
    if monitor_wants && current_monitor_point == 1 {
        write_to_monitor(
            &input_frame,
            &state.monitor_render,
            &state.monitor_event,
            &mut state.monitor_buffer,
            state.monitor_sample_rate,
            &mut state.monitor_resample_buf,
        );
    }

    // 监听点 2=降噪后
    if monitor_wants && current_monitor_point == 2 {
        write_to_monitor(
            &output_frame,
            &state.monitor_render,
            &state.monitor_event,
            &mut state.monitor_buffer,
            state.monitor_sample_rate,
            &mut state.monitor_resample_buf,
        );
    }

    // ====== 增益（原地写入 work_buf_a）======
    let mic_gain = current_config.mic_gain;
    let pre_gain_peak = output_frame
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, f32::max);
    for (i, &s) in output_frame.iter().enumerate() {
        state.work_buf_a[i] = s * mic_gain;
    }
    let post_gain_peak = state.work_buf_a[..deps.frame_size]
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, f32::max);

    // 监听点 3=增益后
    if monitor_wants && current_monitor_point == 3 {
        write_to_monitor(
            &state.work_buf_a[..deps.frame_size],
            &state.monitor_render,
            &state.monitor_event,
            &mut state.monitor_buffer,
            state.monitor_sample_rate,
            &mut state.monitor_resample_buf,
        );
    }

    // ====== EQ 均衡器 ======
    let mut eq_config_changed = false;
    {
        let new_eq_config = deps.eq_config.read().clone();
        if let Some(ref last) = state.last_eq_config {
            if new_eq_config != *last {
                eq_config_changed = true;
            }
        } else if new_eq_config.enabled {
            eq_config_changed = true;
        }
        if eq_config_changed {
            deps.eq_config_dirty.store(false, Ordering::Release);
            state.eq_processor.apply_config(&new_eq_config);
            state.last_eq_config = Some(new_eq_config);
        }
    }
    let current_eq = state.last_eq_config.clone().unwrap_or_default();
    // EQ 处理：work_buf_a → work_buf_b（如果开启），否则直接引用 work_buf_a
    if current_eq.enabled {
        state.work_buf_b[..deps.frame_size].copy_from_slice(&state.work_buf_a[..deps.frame_size]);
        state.eq_processor.process_frame(&mut state.work_buf_b[..deps.frame_size]);
        // NaN/Inf 检查
        for s in state.work_buf_b[..deps.frame_size].iter_mut() {
            if !s.is_finite() {
                *s = 0.0;
            }
        }
    } else {
        state.work_buf_b[..deps.frame_size].copy_from_slice(&state.work_buf_a[..deps.frame_size]);
    }
    let post_eq_peak = state.work_buf_b[..deps.frame_size]
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, f32::max);

    // 监听点 4=EQ 后
    if monitor_wants && current_monitor_point == 4 {
        write_to_monitor(
            &state.work_buf_b[..deps.frame_size],
            &state.monitor_render,
            &state.monitor_event,
            &mut state.monitor_buffer,
            state.monitor_sample_rate,
            &mut state.monitor_resample_buf,
        );
    }

    // ====== 爆炸模式：work_buf_b → work_buf_a ======
    if deps.explode_enabled.load(Ordering::Relaxed) {
        process_explode_into(
            &state.work_buf_b[..deps.frame_size],
            &mut state.work_buf_a[..deps.frame_size],
            &deps.explode_state,
            &mut state.explode_audio,
        );
    } else {
        state.work_buf_a[..deps.frame_size]
            .copy_from_slice(&state.work_buf_b[..deps.frame_size]);
    }

    // ====== BGM 混音：work_buf_a → work_buf_b ======
    if deps.bgm_running.load(Ordering::Acquire) {
        if let Some(ref receiver) = *deps.bgm_receiver {
            while let Ok(bgm_samples) = receiver.try_recv() {
                state.bgm_buf.extend_from_slice(&bgm_samples);
            }
        }

        let max_bgm = deps.frame_size * 10;
        if state.bgm_buf.len() > max_bgm {
            state.bgm_buf.drain(..state.bgm_buf.len() - max_bgm);
        }

        let bgm_gain_bits = deps.bgm_gain.load(Ordering::Relaxed);
        let bgm_gain_val = f32::from_bits(bgm_gain_bits).max(0.0).min(2.0);

        for i in 0..deps.frame_size {
            let bgm_idx = i * 2;
            let bgm_l = if bgm_idx < state.bgm_buf.len() {
                state.bgm_buf[bgm_idx] as f32
            } else {
                0.0
            };
            let bgm_r = if bgm_idx + 1 < state.bgm_buf.len() {
                state.bgm_buf[bgm_idx + 1] as f32
            } else {
                0.0
            };
            let bgm_mono = (bgm_l + bgm_r) / 2.0;
            state.work_buf_b[i] = state.work_buf_a[i] + bgm_mono * bgm_gain_val;
        }

        // 消费已使用的 BGM 样本
        let used = deps.frame_size * 2;
        if used <= state.bgm_buf.len() {
            state.bgm_buf.drain(..used);
        } else {
            state.bgm_buf.clear();
        }
    } else {
        // 无 BGM：work_buf_a → work_buf_b
        state.work_buf_b[..deps.frame_size]
            .copy_from_slice(&state.work_buf_a[..deps.frame_size]);
    }

    // 每秒记录 BGM 缓冲区状态，诊断漂移
    if state.frame_count % 480 == 0 && deps.bgm_running.load(Ordering::Acquire) {
        debug_log(&format!(
            "BGM buf level: {} samples ({:.1}ms)",
            state.bgm_buf.len(),
            state.bgm_buf.len() as f64 / 48000.0 * 1000.0
        ));
    }

    // ====== 诊断：BGM 混音后峰值 ======
    let post_bgm_peak = state.work_buf_b[..deps.frame_size]
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, f32::max);

    // ============ Soft limiter（work_buf_b 原地处理）============
    if !deps.explode_state.enabled.load(Ordering::Relaxed) {
        for sample in state.work_buf_b[..deps.frame_size].iter_mut() {
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
        for sample in state.work_buf_b[..deps.frame_size].iter_mut() {
            if !sample.is_finite() {
                *sample = 0.0;
            }
        }
    }

    // ====== 诊断：limiter 后峰值 ======
    let output_peak = state.work_buf_b[..deps.frame_size]
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, f32::max);

    // 监听点 5=最终输出（limiter 后）
    if monitor_wants && current_monitor_point == 5 {
        write_to_monitor(
            &state.work_buf_b[..deps.frame_size],
            &state.monitor_render,
            &state.monitor_event,
            &mut state.monitor_buffer,
            state.monitor_sample_rate,
            &mut state.monitor_resample_buf,
        );
    }

    // 重采样回设备采样率 + 单声道→多声道 + f32→字节
    let resample_len = if deps.output_sample_rate != 48000 {
        resample_in_place(
            &state.work_buf_b[..deps.frame_size],
            48000,
            deps.output_sample_rate,
            &mut state.resample_buf,
        )
    } else {
        state.resample_buf[..deps.frame_size]
            .copy_from_slice(&state.work_buf_b[..deps.frame_size]);
        deps.frame_size
    };
    let out_bytes = format_output_bytes(
        &state.resample_buf[..resample_len],
        &mut state.output_buffer,
        deps.output_channels,
        deps.output_bytes_per_frame,
        deps.output_bits,
        deps.output_sample_type,
    );

    state.frame_count += 1;

    // 帧处理总耗时
    let frame_ms = frame_start.elapsed().as_micros() as f32 / 1000.0;
    if frame_ms > 10.0 {
        debug_log(&format!(
            "SLOW frame: {:.1}ms frame={}",
            frame_ms, state.frame_count
        ));
    }

    // ====== 诊断日志 ======
    log_diagnostics(
        state.frame_count,
        &input_frame,
        &PeakDiagnostics {
            pre_gain: pre_gain_peak,
            post_gain: post_gain_peak,
            post_eq: post_eq_peak,
            post_bgm: post_bgm_peak,
            output: output_peak,
        },
        mic_gain,
        current_eq.enabled,
        deps.bgm_running.load(Ordering::Acquire),
    );

    // 异常告警
    let alarm = post_gain_peak > 24000.0
        || post_eq_peak > 24000.0
        || post_bgm_peak > 24000.0
        || output_peak > 24000.0;
    if alarm {
        debug_log(&format!(
            "ALARM frame={} | in={:.0} preG={:.0} postG={:.0} postEQ={:.0} postBGM={:.0} out={:.0} | gain={:.1} eq={} bgm={}",
            state.frame_count,
            input_frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max),
            pre_gain_peak,
            post_gain_peak,
            post_eq_peak,
            post_bgm_peak,
            output_peak,
            mic_gain,
            current_eq.enabled,
            deps.bgm_running.load(Ordering::Acquire)
        ));
    }

    // 延迟测量（每帧更新，供前端显示）
    if state.frame_count % 5 == 0 {
        let input_rms = calculate_rms(&input_frame);
        let output_rms = calculate_rms(&state.work_buf_b[..deps.frame_size]);
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

        let input_padding = deps.input_client.get_current_padding().unwrap_or(0);
        let input_latency = input_padding as f32 / deps.input_sample_rate as f32 * 1000.0;
        let output_latency = deps.frame_size as f32 / deps.output_sample_rate as f32 * 1000.0;

        let mut current_stats = deps.stats.write();
        current_stats.input_level = input_level_db;
        current_stats.output_level = output_level_db;
        current_stats.noise_reduction_db = input_level_db - output_level_db;
        current_stats.latency_ms = input_latency + output_latency;
        current_stats.frames_processed = state.frame_count;
        current_stats.spectrum = compute_spectrum(&input_frame, 32);
    }

    out_bytes
}
