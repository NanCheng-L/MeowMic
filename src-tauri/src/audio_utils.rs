#![allow(dead_code)]
use wasapi::*;

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
    monitor_buffer: &mut [u8],
    _monitor_sample_rate: u32,
    _resample_buf: &mut [f32],
) {
    let render = match render_opt {
        Some(r) => r,
        None => return,
    };

    // 等待设备事件（非阻塞，2ms 超时）
    if let Some(evt) = event_opt {
        let _ = evt.wait_for_event(2);
    }

    // 设备原生格式是 f32 stereo，直接写入（和 render 一样）
    let frames = samples.len();
    let stereo_f32_bytes = frames * 2 * 4; // stereo f32
    if stereo_f32_bytes > monitor_buffer.len() {
        return;
    }

    // mono f32 → stereo f32
    for i in 0..frames {
        let val = samples[i];
        let bytes = val.to_le_bytes();
        let pos = i * 8; // 2 channels * 4 bytes
        if pos + 8 <= monitor_buffer.len() {
            // left channel
            monitor_buffer[pos] = bytes[0];
            monitor_buffer[pos + 1] = bytes[1];
            monitor_buffer[pos + 2] = bytes[2];
            monitor_buffer[pos + 3] = bytes[3];
            // right channel
            monitor_buffer[pos + 4] = bytes[0];
            monitor_buffer[pos + 5] = bytes[1];
            monitor_buffer[pos + 6] = bytes[2];
            monitor_buffer[pos + 7] = bytes[3];
        }
    }
    if let Err(e) = render.write_to_device(frames, &monitor_buffer[..stereo_f32_bytes], None) {
        crate::debug::debug_log(&format!("Monitor write FAILED: frames={} err={:?}", frames, e));
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

