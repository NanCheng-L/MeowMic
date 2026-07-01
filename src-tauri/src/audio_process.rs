#![allow(dead_code)]

/// 音频处理模块（旧单线程架构代码，已被 dsp_thread 替代，保留供参考）

use crate::audio_engine::DenoiseConfig;
use crate::audio_utils::{calculate_rms, compute_spectrum_into, resample_in_place, write_to_monitor};
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
    pub frames_dropped: u64,
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
    /// BGM 缓冲区读位置（避免 drain 的 O(n) memmove）
    pub bgm_read_pos: usize,
    /// 频谱计算预分配 buffer（避免每 5 帧堆分配）
    pub spectrum_buf: [f32; 32],
    /// 高通滤波器状态：切除 80Hz 以下次声波
    pub hp_x_prev: f32,
    pub hp_y_prev: f32,
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

/// 将 mono f32 样本（i16 范围）转换为多声道输出字节
/// 对齐 v0.2.8 行为：mono→多声道扩展 + 各格式正确缩放
pub fn format_output_bytes(
    samples: &[f32],        // mono 样本，值域 [-32768, 32767]
    output_buffer: &mut [u8],
    output_channels: usize,
    output_bytes_per_frame: usize,
    output_bits: u16,
    output_sample_type: &SampleType,
) -> usize {
    let mono_frames = samples.len();
    let out_bytes = mono_frames * output_bytes_per_frame;
    let bytes_per_sample = output_bytes_per_frame / output_channels;

    match output_bits {
        8 => match output_sample_type {
            SampleType::Int => {
                for (fi, &s) in samples.iter().enumerate() {
                    let val = ((s / 128.0) + 128.0).max(0.0).min(255.0) as u8;
                    for ch in 0..output_channels {
                        let pos = fi * output_bytes_per_frame + ch * bytes_per_sample;
                        if pos < output_buffer.len() {
                            output_buffer[pos] = val;
                        }
                    }
                }
            }
            _ => log::warn!("Unsupported 8-bit output format: {:?}", output_sample_type),
        },
        16 => match output_sample_type {
            SampleType::Int => {
                for (fi, &s) in samples.iter().enumerate() {
                    let val = s.max(-32768.0).min(32767.0) as i16;
                    let bytes = val.to_le_bytes();
                    for ch in 0..output_channels {
                        let pos = fi * output_bytes_per_frame + ch * bytes_per_sample;
                        if pos + 2 <= output_buffer.len() {
                            output_buffer[pos] = bytes[0];
                            output_buffer[pos + 1] = bytes[1];
                        }
                    }
                }
            }
            _ => log::warn!("Unsupported 16-bit output format: {:?}", output_sample_type),
        },
        24 => match output_sample_type {
            SampleType::Int => {
                for (fi, &s) in samples.iter().enumerate() {
                    let val = (s * 256.0).max(-8388608.0).min(8388607.0) as i32;
                    for ch in 0..output_channels {
                        let pos = fi * output_bytes_per_frame + ch * bytes_per_sample;
                        if pos + 3 <= output_buffer.len() {
                            output_buffer[pos] = (val & 0xFF) as u8;
                            output_buffer[pos + 1] = ((val >> 8) & 0xFF) as u8;
                            output_buffer[pos + 2] = ((val >> 16) & 0xFF) as u8;
                        }
                    }
                }
            }
            _ => log::warn!("Unsupported 24-bit output format: {:?}", output_sample_type),
        },
        32 => match output_sample_type {
            SampleType::Int => {
                for (fi, &s) in samples.iter().enumerate() {
                    let val = (s * 65536.0).max(-2147483648.0).min(2147483647.0) as i32;
                    let bytes = val.to_le_bytes();
                    for ch in 0..output_channels {
                        let pos = fi * output_bytes_per_frame + ch * bytes_per_sample;
                        if pos + 4 <= output_buffer.len() {
                            output_buffer[pos..pos + 4].copy_from_slice(&bytes);
                        }
                    }
                }
            }
            SampleType::Float => {
                for (fi, &s) in samples.iter().enumerate() {
                    let val = (s / 32767.0).max(-1.0).min(1.0);
                    let bytes = val.to_le_bytes();
                    for ch in 0..output_channels {
                        let pos = fi * output_bytes_per_frame + ch * bytes_per_sample;
                        if pos + 4 <= output_buffer.len() {
                            output_buffer[pos..pos + 4].copy_from_slice(&bytes);
                        }
                    }
                }
            }
        },
        64 => match output_sample_type {
            SampleType::Float => {
                for (fi, &s) in samples.iter().enumerate() {
                    let val = (s as f64 / 32767.0).max(-1.0).min(1.0);
                    let bytes = val.to_le_bytes();
                    for ch in 0..output_channels {
                        let pos = fi * output_bytes_per_frame + ch * bytes_per_sample;
                        if pos + 8 <= output_buffer.len() {
                            output_buffer[pos..pos + 8].copy_from_slice(&bytes);
                        }
                    }
                }
            }
            _ => log::warn!("Unsupported 64-bit output format: {:?}", output_sample_type),
        },
        _ => {
            // fallback to 16-bit
            log::warn!("Unsupported output bit depth: {}, falling back to 16-bit", output_bits);
            for (fi, &s) in samples.iter().enumerate() {
                let val = s.max(-32768.0).min(32767.0) as i16;
                let bytes = val.to_le_bytes();
                for ch in 0..output_channels {
                    let pos = fi * output_bytes_per_frame + ch * bytes_per_sample;
                    if pos + 2 <= output_buffer.len() {
                        output_buffer[pos] = bytes[0];
                        output_buffer[pos + 1] = bytes[1];
                    }
                }
            }
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
    let mut input_frame = [0.0f32; 480];
    let len = chunk.len().min(deps.frame_size);
    input_frame[..len].copy_from_slice(&chunk[..len]);

    let mut output_frame = input_frame;
    if current_config.enabled {
        state.denoise.process_frame(&mut output_frame, &input_frame);

        // NaN 检查 + input/output peak 检测（融合为一遍遍历）
        let mut input_peak = 0.0f32;
        let mut output_peak = 0.0f32;
        for (out, &inp) in output_frame.iter_mut().zip(input_frame.iter()) {
            if !out.is_finite() {
                *out = 0.0;
            }
            input_peak = input_peak.max(inp.abs());
            output_peak = output_peak.max(out.abs());
        }

        // 检测模型是否被回声打废：输入能量异常高（回声反馈）但降噪输出全零
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

        // Denormal flush: RNNoise 在静音帧可能输出极小的非规格化浮点数，
        // 经过 gain/EQ 放大后产生刺啦声。强制 flush to zero。
        for sample in output_frame.iter_mut() {
            if !sample.is_finite() || sample.abs() < 1e-10 {
                *sample = 0.0;
            }
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

    // ====== 增益（原地写入 work_buf_a，pre/post peak 融合为一遍遍历）======
    let mic_gain = current_config.mic_gain;
    let mut pre_gain_peak = 0.0f32;
    let mut post_gain_peak = 0.0f32;
    for (i, &s) in output_frame.iter().enumerate() {
        pre_gain_peak = pre_gain_peak.max(s.abs());
        let val = s * mic_gain;
        state.work_buf_a[i] = val;
        post_gain_peak = post_gain_peak.max(val.abs());
    }

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
    let post_eq_peak = if current_eq.enabled {
        state.work_buf_b[..deps.frame_size].copy_from_slice(&state.work_buf_a[..deps.frame_size]);
        state.eq_processor.process_frame(&mut state.work_buf_b[..deps.frame_size]);
        // NaN/Inf 检查 + post_eq_peak（融合为一遍）
        let mut post_eq_local = 0.0f32;
        for s in state.work_buf_b[..deps.frame_size].iter_mut() {
            if !s.is_finite() {
                *s = 0.0;
            }
            post_eq_local = post_eq_local.max(s.abs());
        }
        post_eq_local
    } else {
        state.work_buf_b[..deps.frame_size].copy_from_slice(&state.work_buf_a[..deps.frame_size]);
        state.work_buf_b[..deps.frame_size]
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max)
    };

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

        // 溢出保护：已消费部分超过 buffer 一半时 compact（低频 O(n)）
        let consumed = state.bgm_read_pos;
        if consumed > state.bgm_buf.len() / 2 && consumed > 0 {
            state.bgm_buf.drain(..consumed);
            state.bgm_read_pos = 0;
        }
        // 硬上限：BGM 数据堆积过多时丢弃最旧的数据
        let max_bgm = deps.frame_size * 20;
        if state.bgm_buf.len() > max_bgm {
            let excess = state.bgm_buf.len() - max_bgm;
            state.bgm_buf.drain(..excess);
            state.bgm_read_pos = state.bgm_read_pos.saturating_sub(excess);
        }

        let bgm_gain_bits = deps.bgm_gain.load(Ordering::Relaxed);
        let bgm_gain_val = f32::from_bits(bgm_gain_bits).max(0.0).min(2.0);

        let needed = deps.frame_size * 2;

        // 数据不足时只消费可用部分，避免 bgm_read_pos 越界
        let available_from_pos = state.bgm_buf.len().saturating_sub(state.bgm_read_pos);
        let actual_needed = needed.min(available_from_pos);
        // 实际读取位置按 i16 样本数计算（不是字节数）
        let actual_samples = actual_needed / 2;

        for i in 0..actual_samples {
            let bgm_idx = state.bgm_read_pos + i * 2;
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
        // 不足的部分保持 work_buf_a 的值（无 BGM 混入）
        for i in actual_samples..deps.frame_size {
            state.work_buf_b[i] = state.work_buf_a[i];
        }

        // 推进读位置
        state.bgm_read_pos += actual_needed;
        // 全部消费完时 reset
        if state.bgm_read_pos >= state.bgm_buf.len() {
            state.bgm_buf.clear();
            state.bgm_read_pos = 0;
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

    // ============ Soft limiter + output_peak（融合为一遍）============
    let output_peak;
    if !deps.explode_state.enabled.load(Ordering::Relaxed) {
        let mut peak = 0.0f32;
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
            peak = peak.max(sample.abs());
        }
        output_peak = peak;
    } else {
        let mut peak = 0.0f32;
        for sample in state.work_buf_b[..deps.frame_size].iter_mut() {
            if !sample.is_finite() {
                *sample = 0.0;
            }
            peak = peak.max(sample.abs());
        }
        output_peak = peak;
    }

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

    // 高通滤波器：切除 80Hz 以下次声波能量
    const HP_ALPHA: f32 = 0.98953;
    {
        let buf = &mut state.work_buf_b[..deps.frame_size];
        let mut x_prev = state.hp_x_prev;
        let mut y_prev = state.hp_y_prev;
        for sample in buf.iter_mut() {
            let x = *sample;
            let y = HP_ALPHA * (y_prev + x - x_prev);
            *sample = y;
            x_prev = x;
            y_prev = y;
        }
        // Denormal flush：静音时状态累积极小值，denormal 运算比正常慢 10-100 倍
        if !x_prev.is_finite() || x_prev.abs() < 1e-10 { x_prev = 0.0; }
        if !y_prev.is_finite() || y_prev.abs() < 1e-10 { y_prev = 0.0; }
        state.hp_x_prev = x_prev;
        state.hp_y_prev = y_prev;
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
        current_stats.frames_dropped = state.frames_dropped;
        compute_spectrum_into(&input_frame, &mut state.spectrum_buf);
        current_stats.spectrum.copy_from_slice(&state.spectrum_buf);
    }

    out_bytes
}
