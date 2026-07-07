#![allow(dead_code)]
use wasapi::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// 监听写入失败计数器（用于限制日志频率）
static MONITOR_FAIL_COUNT: AtomicU64 = AtomicU64::new(0);
static MONITOR_FAIL_FIRST_TIME: AtomicU64 = AtomicU64::new(0);
static MONITOR_FAIL_LAST_LOG: AtomicU64 = AtomicU64::new(0);

pub fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// 计算频谱能量分布（写入预分配 buffer，避免堆分配）
pub fn compute_spectrum_into(samples: &[f32], output: &mut [f32]) {
    let n = samples.len();
    let bands = output.len();
    if n == 0 {
        output.fill(0.0);
        return;
    }

    let band_size = n / bands;
    for i in 0..bands {
        let start = i * band_size;
        let end = (start + band_size).min(n);
        if start >= n {
            output[i] = 0.0;
            continue;
        }
        let rms = calculate_rms(&samples[start..end]);
        let mut val = rms / 32768.0;
        if val > 0.001 {
            val = (val.log10() + 3.0) / 3.0;
        }
        // 用 max/min 替代 clamp（clamp 遇到 NaN 会 panic）
        output[i] = val.max(0.0).min(1.0);
    }
}

/// 将 f32 样本写入监听设备（非阻塞，使用预分配的 monitor_buffer）
pub fn write_to_monitor(
    samples: &[f32],
    render_opt: &Option<AudioRenderClient>,
    event_opt: &Option<wasapi::Handle>,
    client_opt: &Option<AudioClient>,
    monitor_buffer: &mut [u8],
    _monitor_sample_rate: u32,
    _resample_buf: &mut [f32],
    channels: u16,
    bits_per_sample: u16,
) {
    let render = match render_opt {
        Some(r) => r,
        None => return,
    };

    // 等待设备事件（非阻塞，2ms 超时）
    if let Some(evt) = event_opt {
        let _ = evt.wait_for_event(2);
    }

    // 检查可用空间，避免缓冲区溢出
    let frames = samples.len();
    let writable = if let Some(client) = client_opt {
        match client.get_available_space_in_frames() {
            Ok(n) => n as usize,
            Err(_) => return,
        }
    } else {
        frames
    };
    if writable == 0 {
        return;
    }
    let frames_to_write = frames.min(writable);

    // 计算每帧字节数
    let bytes_per_sample = bits_per_sample as usize / 8;
    let bytes_per_frame = channels as usize * bytes_per_sample;
    let total_bytes = frames_to_write * bytes_per_frame;
    if total_bytes > monitor_buffer.len() {
        return;
    }

    // mono f32 → 设备原生格式（多声道复制）
    match bits_per_sample {
        32 => {
            // f32 格式
            for i in 0..frames_to_write {
                let val = samples[i];
                let bytes = val.to_le_bytes();
                for ch in 0..channels as usize {
                    let pos = i * bytes_per_frame + ch * bytes_per_sample;
                    if pos + 4 <= monitor_buffer.len() {
                        monitor_buffer[pos] = bytes[0];
                        monitor_buffer[pos + 1] = bytes[1];
                        monitor_buffer[pos + 2] = bytes[2];
                        monitor_buffer[pos + 3] = bytes[3];
                    }
                }
            }
        }
        16 => {
            // i16 格式
            for i in 0..frames_to_write {
                let val = (samples[i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
                let bytes = val.to_le_bytes();
                for ch in 0..channels as usize {
                    let pos = i * bytes_per_frame + ch * bytes_per_sample;
                    if pos + 2 <= monitor_buffer.len() {
                        monitor_buffer[pos] = bytes[0];
                        monitor_buffer[pos + 1] = bytes[1];
                    }
                }
            }
        }
        _ => {
            // 其他格式 fallback 到 f32
            crate::debug::debug_log(&format!("Monitor: unsupported {}bit format, fallback to f32", bits_per_sample));
            for i in 0..frames_to_write {
                let val = samples[i];
                let bytes = val.to_le_bytes();
                for ch in 0..channels as usize {
                    let pos = i * bytes_per_frame + ch * 4;
                    if pos + 4 <= monitor_buffer.len() {
                        monitor_buffer[pos] = bytes[0];
                        monitor_buffer[pos + 1] = bytes[1];
                        monitor_buffer[pos + 2] = bytes[2];
                        monitor_buffer[pos + 3] = bytes[3];
                    }
                }
            }
        }
    }
    if let Err(e) = render.write_to_device(frames_to_write, &monitor_buffer[..total_bytes], None) {
        let count = MONITOR_FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // 第一次：记录起始时间，打印完整错误
        if count == 0 {
            MONITOR_FAIL_FIRST_TIME.store(now_sec, Ordering::Relaxed);
            MONITOR_FAIL_LAST_LOG.store(now_sec, Ordering::Relaxed);
            crate::debug::debug_log(&format!(
                "Monitor write FAILED: frames={} channels={} bits={} err={:?}",
                frames_to_write, channels, bits_per_sample, e
            ));
        } else {
            let last_log = MONITOR_FAIL_LAST_LOG.load(Ordering::Relaxed);
            // 每 10 秒打印一次汇总
            if now_sec.saturating_sub(last_log) >= 10 {
                let start = MONITOR_FAIL_FIRST_TIME.load(Ordering::Relaxed);
                let elapsed = now_sec.saturating_sub(start);
                crate::debug::debug_log(&format!(
                    "Monitor write FAILED: {} times in {}s (last err={:?})",
                    count + 1, elapsed, e
                ));
                MONITOR_FAIL_LAST_LOG.store(now_sec, Ordering::Relaxed);
            }
        }
    }
}

/// 线性插值重采样（写入预分配的 output buffer，返回实际写入的样本数）
pub fn resample_in_place(input: &[f32], from_rate: u32, to_rate: u32, output: &mut [f32]) -> usize {
    if from_rate == to_rate || input.is_empty() {
        let len = input.len().min(output.len());
        output[..len].copy_from_slice(&input[..len]);
        return len;
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (input.len() as f64 / ratio) as usize;
    let output_len = output_len.min(output.len());
    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        let frac = src_pos - src_idx as f64;
        output[i] = if src_idx + 1 < input.len() {
            input[src_idx] * (1.0 - frac as f32) + input[src_idx + 1] * frac as f32
        } else {
            input[src_idx]
        };
    }
    output_len
}

/// 将原始字节样本（i16 或 f32）转换为 f32 归一化值（写入预分配 buffer，返回样本数）
pub fn bytes_to_f32_samples_into(buf: &[u8], bits: u16, sample_type: &SampleType, _channels: usize, output: &mut [f32]) -> usize {
    match (bits, sample_type) {
        (8, SampleType::Int) => {
            let len = buf.len().min(output.len());
            for (i, &b) in buf.iter().take(len).enumerate() {
                output[i] = (b as i16 - 128) as f32 * 128.0;
            }
            len
        }
        (16, SampleType::Int) => {
            let len = (buf.len() / 2).min(output.len());
            for (i, c) in buf.chunks_exact(2).take(len).enumerate() {
                output[i] = i16::from_le_bytes([c[0], c[1]]) as f32;
            }
            len
        }
        (24, SampleType::Int) => {
            let len = (buf.len() / 3).min(output.len());
            for (i, c) in buf.chunks_exact(3).take(len).enumerate() {
                let val = (c[0] as i32) | ((c[1] as i32) << 8) | ((c[2] as i32) << 16);
                let val = if val & 0x800000 != 0 { val | 0xFF000000u32 as i32 } else { val };
                output[i] = (val >> 8) as f32;
            }
            len
        }
        (32, SampleType::Int) => {
            let len = (buf.len() / 4).min(output.len());
            for (i, c) in buf.chunks_exact(4).take(len).enumerate() {
                let val = i32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                output[i] = (val >> 16) as f32;
            }
            len
        }
        (32, SampleType::Float) => {
            let len = (buf.len() / 4).min(output.len());
            for (i, c) in buf.chunks_exact(4).take(len).enumerate() {
                output[i] = f32::from_le_bytes([c[0], c[1], c[2], c[3]]) * 32767.0;
            }
            len
        }
        (64, SampleType::Float) => {
            let len = (buf.len() / 8).min(output.len());
            for (i, c) in buf.chunks_exact(8).take(len).enumerate() {
                output[i] = f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]) as f32 * 32767.0;
            }
            len
        }
        _ => {
            log::warn!("Unsupported audio format: {}bit {:?}, falling back to 16bit", bits, sample_type);
            let len = (buf.len() / 2).min(output.len());
            for (i, c) in buf.chunks_exact(2).take(len).enumerate() {
                output[i] = i16::from_le_bytes([c[0], c[1]]) as f32;
            }
            len
        }
    }
}

/// 将原始字节样本（i16 或 f32）转换为 f32 归一化值（写入预分配 buffer，返回样本数）
pub fn downmix_to_mono_into(samples: &[f32], channels: usize, output: &mut [f32]) -> usize {
    if channels <= 1 {
        let len = samples.len().min(output.len());
        output[..len].copy_from_slice(&samples[..len]);
        return len;
    }
    let mut count = 0;
    for frame in samples.chunks(channels) {
        if count >= output.len() { break; }
        output[count] = frame.iter().sum::<f32>() / channels as f32;
        count += 1;
    }
    count
}

