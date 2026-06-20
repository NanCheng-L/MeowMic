use wasapi::*;

pub fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

pub fn compute_spectrum(samples: &[f32], bands: usize) -> Vec<f32> {
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

/// 将 f32 样本写入监听设备
pub fn write_to_monitor(
    samples: &[f32],
    render_opt: &Option<AudioRenderClient>,
    event_opt: &Option<wasapi::Handle>,
    buffer: &mut [u8],
    monitor_sample_rate: u32,
) {
    let render = match render_opt {
        Some(r) => r,
        None => return,
    };
    let ready = match event_opt {
        Some(evt) => evt.wait_for_event(10).is_ok(),
        None => true,
    };
    if !ready {
        return;
    }
    // 重采样到监听设备采样率
    let resampled = if monitor_sample_rate != 48000 {
        resample_linear(samples, 48000, monitor_sample_rate)
    } else {
        samples.to_vec()
    };
    let stereo = upmix_to_stereo(&resampled, 2);
    for (i, &sample) in stereo.iter().enumerate() {
        let val = (sample.clamp(-32768.0, 32767.0)) as i16;
        let bytes = val.to_le_bytes();
        if i * 2 + 1 < buffer.len() {
            buffer[i * 2] = bytes[0];
            buffer[i * 2 + 1] = bytes[1];
        }
    }
    let frames = resampled.len();
    let _ = render.write_to_device(frames, &buffer[..frames * 4], None);
}

/// 线性插值重采样：将音频从 from_rate 重采样到 to_rate
pub fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (input.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        let frac = src_pos - src_idx as f64;
        let sample = if src_idx + 1 < input.len() {
            input[src_idx] * (1.0 - frac as f32) + input[src_idx + 1] * frac as f32
        } else {
            input[src_idx]
        };
        output.push(sample);
    }
    output
}

/// 将原始字节样本（i16 或 f32）转换为 f32 归一化值
pub fn bytes_to_f32_samples(buf: &[u8], bits: u16, sample_type: &SampleType, _channels: usize) -> Vec<f32> {
    match (bits, sample_type) {
        (8, SampleType::Int) => buf
            .iter()
            .map(|&b| (b as i16 - 128) as f32 * 128.0) // unsigned 8-bit: 0-255 → -128~127, 然后放大到 i16 范围
            .collect(),
        (16, SampleType::Int) => buf
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32)
            .collect(),
        (24, SampleType::Int) => buf
            .chunks_exact(3)
            .map(|c| {
                // 24-bit little-endian: [low, mid, high]，符号扩展到 i32
                let val = (c[0] as i32) | ((c[1] as i32) << 8) | ((c[2] as i32) << 16);
                // 符号扩展：如果 bit 23 为 1，高 8 位填 1
                let val = if val & 0x800000 != 0 { val | 0xFF000000u32 as i32 } else { val };
                (val >> 8) as f32 // 右移 8 位，相当于除以 256，映射到 i16 范围
            })
            .collect(),
        (32, SampleType::Int) => buf
            .chunks_exact(4)
            .map(|c| {
                let val = i32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                (val >> 16) as f32
            })
            .collect(),
        (32, SampleType::Float) => buf
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) * 32767.0)
            .collect(),
        (64, SampleType::Float) => buf
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]) as f32 * 32767.0)
            .collect(),
        _ => {
            log::warn!("Unsupported audio format: {}bit {:?}, falling back to 16bit", bits, sample_type);
            buf.chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32)
                .collect()
        }
    }
}

/// 多声道转单声道（取平均）
pub fn downmix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// 单声道转多声道（复制到每个声道）
pub fn upmix_to_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .iter()
        .flat_map(|&s| vec![s; channels])
        .collect()
}
