#![allow(dead_code)]
use wasapi::*;
use std::sync::atomic::{AtomicU64, Ordering};
use rustfft::{FftPlanner, num_complex::Complex};

/// 鐩戝惉鍐欏叆澶辫触璁℃暟鍣紙鐢ㄤ簬闄愬埗鏃ュ織棰戠巼锛?
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

/// 璁＄畻棰戣氨鑳介噺鍒嗗竷锛團FT 棰戝煙鍒嗘瀽锛屽啓鍏ラ鍒嗛厤 buffer锛?
/// 杈撳叆锛氬綊涓€鍖?f32 鏍锋湰 [-1.0, 1.0]
/// 杈撳嚭锛?-1 鑼冨洿
/// 鍙傝€?spectrum-analyzer crate 鐨勬爣鍑嗗仛娉曪細magnitude/N 鈫?20*log10 鈫?鏄犲皠
pub fn compute_spectrum_into(samples: &[f32], output: &mut [f32]) {
    let n = samples.len();
    let bands = output.len();
    if n == 0 || bands == 0 {
        output.fill(0.0);
        return;
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);

    // Hanning 窗
    let mut buffer: Vec<Complex<f32>> = samples.iter().enumerate().map(|(i, &s)| {
        let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos());
        Complex::new(s * w, 0.0)
    }).collect();

    fft.process(&mut buffer);

    let half_n = n / 2;
    let freq_per_bin = 48000.0f32 / n as f32;
    const FREQ_MIN: f32 = 20.0;
    const FREQ_MAX: f32 = 24000.0;

    let mut band_edges: Vec<usize> = Vec::with_capacity(bands + 1);
    for i in 0..=bands {
        let t = i as f32 / bands as f32;
        let freq = FREQ_MIN * (FREQ_MAX / FREQ_MIN).powf(t);
        let bin = ((freq / freq_per_bin) as usize).min(half_n);
        band_edges.push(bin);
    }

    const MIN_DB: f32 = -80.0;
    const MAX_DB: f32 = -10.0;

    for i in 0..bands {
        let start = band_edges[i];
        let end = band_edges[i + 1].max(start + 1);

        let mut peak_mag = 0.0f32;
        for bin in start..end {
            if bin >= half_n { break; }
            let mag = (buffer[bin].re * buffer[bin].re + buffer[bin].im * buffer[bin].im).sqrt();
            let normalized = mag / n as f32;
            if normalized > peak_mag {
                peak_mag = normalized;
            }
        }

        let db = if peak_mag > 1e-10 {
            20.0 * peak_mag.log10()
        } else {
            MIN_DB
        };

        let val = ((db - MIN_DB) / (MAX_DB - MIN_DB)).max(0.0).min(1.0);
        output[i] = val;
    }
}

/// 灏?f32 鏍锋湰鍐欏叆鐩戝惉璁惧锛堥潪闃诲锛屼娇鐢ㄩ鍒嗛厤鐨?monitor_buffer锛?
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

    // 绛夊緟璁惧浜嬩欢锛堥潪闃诲锛?ms 瓒呮椂锛?
    if let Some(evt) = event_opt {
        let _ = evt.wait_for_event(2);
    }

    // 妫€鏌ュ彲鐢ㄧ┖闂达紝閬垮厤缂撳啿鍖烘孩鍑?
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

    // 璁＄畻姣忓抚瀛楄妭鏁?
    let bytes_per_sample = bits_per_sample as usize / 8;
    let bytes_per_frame = channels as usize * bytes_per_sample;
    let total_bytes = frames_to_write * bytes_per_frame;
    if total_bytes > monitor_buffer.len() {
        return;
    }

    // mono f32 鈫?璁惧鍘熺敓鏍煎紡锛堝澹伴亾澶嶅埗锛?
    match bits_per_sample {
        32 => {
            // f32 鏍煎紡
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
            // i16 鏍煎紡
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
            // 鍏朵粬鏍煎紡 fallback 鍒?f32
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
        
        // 绗竴娆★細璁板綍璧峰鏃堕棿锛屾墦鍗板畬鏁撮敊璇?
        if count == 0 {
            MONITOR_FAIL_FIRST_TIME.store(now_sec, Ordering::Relaxed);
            MONITOR_FAIL_LAST_LOG.store(now_sec, Ordering::Relaxed);
            crate::debug::debug_log(&format!(
                "Monitor write FAILED: frames={} channels={} bits={} err={:?}",
                frames_to_write, channels, bits_per_sample, e
            ));
        } else {
            let last_log = MONITOR_FAIL_LAST_LOG.load(Ordering::Relaxed);
            // 姣?10 绉掓墦鍗颁竴娆℃眹鎬?
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

/// 绾挎€ф彃鍊奸噸閲囨牱锛堝啓鍏ラ鍒嗛厤鐨?output buffer锛岃繑鍥炲疄闄呭啓鍏ョ殑鏍锋湰鏁帮級
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

/// 灏嗗師濮嬪瓧鑺傛牱鏈紙i16 鎴?f32锛夎浆鎹负 f32 褰掍竴鍖栧€硷紙鍐欏叆棰勫垎閰?buffer锛岃繑鍥炴牱鏈暟锛?
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

/// 灏嗗師濮嬪瓧鑺傛牱鏈紙i16 鎴?f32锛夎浆鎹负 f32 褰掍竴鍖栧€硷紙鍐欏叆棰勫垎閰?buffer锛岃繑鍥炴牱鏈暟锛?
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