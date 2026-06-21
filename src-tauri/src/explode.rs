/// 爆炸模式效果预设
///
/// 每个预设定义不同的音频失真效果，用于游戏/直播中的趣味变声。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// 爆炸模式效果类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ExplodeEffect {
    /// 经典爆音：方波失真 + 音量放大
    Classic = 0,
    /// 电流声：高频噪音叠加 + 轻微失真
    Electric = 1,
    /// 破音：削波失真，音量适中但声音很脏
    Distortion = 2,
    /// 白噪音：纯噪音叠加
    WhiteNoise = 3,
    /// 机器人声：金属质感变声
    Robot = 4,
    /// 恶魔声：低沉沙哑，类似贝利亚
    Demon = 5,
}

impl ExplodeEffect {
    pub fn from_u32(val: u32) -> Self {
        match val {
            0 => Self::Classic,
            1 => Self::Electric,
            2 => Self::Distortion,
            3 => Self::WhiteNoise,
            4 => Self::Robot,
            5 => Self::Demon,
            _ => Self::Classic,
        }
    }
}

/// 爆炸模式状态
pub struct ExplodeState {
    /// 是否启用
    pub enabled: AtomicBool,
    /// 强度 (1-100)
    pub intensity: AtomicU32,
    /// 效果类型
    pub effect_type: AtomicU32,
    /// 用于白噪音/电流声的简单伪随机状态
    noise_state: std::sync::Mutex<u32>,
    /// 延迟线状态（恶魔声用）
    delay_buf: std::sync::Mutex<Vec<f32>>,
    delay_pos: std::sync::Mutex<usize>,
}

impl ExplodeState {
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            intensity: AtomicU32::new(50),
            effect_type: AtomicU32::new(0),
            noise_state: std::sync::Mutex::new(12345),
            delay_buf: std::sync::Mutex::new(vec![0.0; 4800]), // 100ms @ 48kHz
            delay_pos: std::sync::Mutex::new(0),
        }
    }

    fn next_noise(&self) -> f32 {
        let mut state = self.noise_state.lock().unwrap_or_else(|e| e.into_inner());
        *state = state.wrapping_mul(1103515245).wrapping_add(12345);
        let val = (*state >> 16) & 0x7FFF;
        (val as f32 / 16384.0) - 1.0
    }
}

/// 处理爆炸模式效果
pub fn process_explode(samples: &[f32], state: &ExplodeState) -> Vec<f32> {
    if !state.enabled.load(Ordering::Relaxed) {
        return samples.to_vec();
    }

    let intensity = state.intensity.load(Ordering::Relaxed) as f32;
    let effect = ExplodeEffect::from_u32(state.effect_type.load(Ordering::Relaxed));

    match effect {
        ExplodeEffect::Classic => process_classic(samples, intensity),
        ExplodeEffect::Electric => process_electric(samples, intensity, state),
        ExplodeEffect::Distortion => process_distortion(samples, intensity),
        ExplodeEffect::WhiteNoise => process_white_noise(samples, intensity, state),
        ExplodeEffect::Robot => process_robot(samples, intensity),
        ExplodeEffect::Demon => process_demon(samples, intensity, state),
    }
}

/// 处理爆炸模式效果（写入预分配的输出 buffer，避免堆分配）
pub fn process_explode_into(samples: &[f32], output: &mut [f32], state: &ExplodeState) {
    if !state.enabled.load(Ordering::Relaxed) {
        output.copy_from_slice(samples);
        return;
    }

    // 复用已有的 process_explode，然后拷贝结果
    let result = process_explode(samples, state);
    let len = result.len().min(output.len());
    output[..len].copy_from_slice(&result[..len]);
}

/// 经典爆音：方波失真 + 音量放大
fn process_classic(samples: &[f32], intensity: f32) -> Vec<f32> {
    let mapped = 50.0 + intensity * 0.5;
    let gain = 1.0 + (mapped / 100.0).powf(0.6) * 49.0;
    let clip = 32000.0 - (mapped / 100.0).powf(1.5) * 31800.0;

    samples.iter().map(|&s| {
        let boosted = s * gain;
        let clipped = boosted.clamp(-clip, clip);
        clipped / clip * 32767.0
    }).collect()
}

/// 电流声：高频方波调制 + 噪音门限（滋滋声）
fn process_electric(samples: &[f32], intensity: f32, state: &ExplodeState) -> Vec<f32> {
    let mix = intensity / 100.0;
    let sample_rate = 48000.0;

    // 高频方波调制产生滋滋声
    let mod_freq = 1000.0 + mix * 2000.0; // 1-3kHz 高频
    let mod_amount = 0.5 + mix * 0.5;

    // 噪音门限：低于阈值时注入噪音
    let gate_threshold = 2000.0 - mix * 1500.0;
    let noise_inject = mix * 6000.0;

    samples.iter().enumerate().map(|(i, &s)| {
        let t = i as f32 / sample_rate;

        // 高频方波调制（产生金属滋滋声）
        let square = if (mod_freq * t).sin() > 0.0 { 1.0 } else { -1.0 };
        let modulated = s * (1.0 - mod_amount + mod_amount * square);

        // 噪音门限：小信号时注入电流噪音
        let noise = if s.abs() < gate_threshold {
            state.next_noise() * noise_inject
        } else {
            0.0
        };

        (modulated + noise).clamp(-32767.0, 32767.0)
    }).collect()
}

/// 破音：削波失真，音量适中
fn process_distortion(samples: &[f32], intensity: f32) -> Vec<f32> {
    let threshold = 32767.0 - (intensity / 100.0) * 28000.0;
    let gain = 1.0 + (intensity / 100.0) * 3.0;

    samples.iter().map(|&s| {
        let boosted = s * gain;
        if boosted.abs() > threshold {
            let sign = if boosted >= 0.0 { 1.0 } else { -1.0 };
            let excess = boosted.abs() - threshold;
            let compressed = threshold + excess * 0.1;
            sign * compressed
        } else {
            boosted
        }
    }).collect()
}

/// 白噪音：均匀噪音叠加（像收音机无信号）
fn process_white_noise(samples: &[f32], intensity: f32, state: &ExplodeState) -> Vec<f32> {
    let noise_amount = intensity / 100.0 * 12000.0;

    samples.iter().map(|&s| {
        let noise = state.next_noise() * noise_amount;
        // 白噪音混合到原始信号中
        let mixed = s * 0.5 + noise * 0.5;
        mixed.clamp(-32767.0, 32767.0)
    }).collect()
}

/// 机器人声：金属质感变声（通过 ring modulation）
fn process_robot(samples: &[f32], intensity: f32) -> Vec<f32> {
    let mod_freq = 50.0 + (intensity / 100.0) * 150.0;
    let sample_rate = 48000.0;
    let mod_amount = intensity / 100.0;

    samples.iter().enumerate().map(|(i, &s)| {
        let t = i as f32 / sample_rate;
        let modulator = (2.0 * std::f32::consts::PI * mod_freq * t).sin();
        let modulated = s * (1.0 - mod_amount + mod_amount * modulator);
        modulated.clamp(-32767.0, 32767.0)
    }).collect()
}

/// 恶魔声：低沉沙哑 + 金属质感 + 延迟回声（类似贝利亚）
fn process_demon(samples: &[f32], intensity: f32, state: &ExplodeState) -> Vec<f32> {
    let mix = intensity / 100.0;
    let sample_rate = 48000.0;

    // 1. Ring modulation 产生金属质感（低频调制）
    let mod_freq = 30.0 + mix * 40.0; // 30-70Hz 低频调制
    let ring_mix = 0.3 + mix * 0.4; // 调制深度 30%-70%

    // 2. 延迟回声参数
    let delay_ms = 30.0 + mix * 20.0; // 30-50ms 延迟
    let delay_samples = (delay_ms / 1000.0 * sample_rate) as usize;
    let feedback = 0.2 + mix * 0.3; // 反馈 20%-50%
    let delay_mix = 0.15 + mix * 0.25; // 干湿比

    // 3. 轻微失真让声音沙哑
    let drive = 1.0 + mix * 2.0; // 1x-3x 增益

    let mut delay_buf = state.delay_buf.lock().unwrap_or_else(|e| e.into_inner());
    let mut delay_pos = state.delay_pos.lock().unwrap_or_else(|e| e.into_inner());
    let buf_len = delay_buf.len();

    let mut output = Vec::with_capacity(samples.len());

    for (i, &s) in samples.iter().enumerate() {
        let t = i as f32 / sample_rate;

        // Ring modulation
        let modulator = (2.0 * std::f32::consts::PI * mod_freq * t).sin();
        let ring_out = s * ((1.0 - ring_mix) + ring_mix * modulator);

        // 轻微失真（软削波）
        let driven = ring_out * drive;
        let distorted = if driven > 8000.0 {
            8000.0 + (driven - 8000.0) * 0.2
        } else if driven < -8000.0 {
            -8000.0 + (driven + 8000.0) * 0.2
        } else {
            driven
        };

        // 延迟回声
        let read_pos = (*delay_pos + buf_len - delay_samples) % buf_len;
        let delayed = delay_buf[read_pos];

        // 写入延迟线（输入 + 反馈）
        delay_buf[*delay_pos] = distorted + delayed * feedback;
        *delay_pos = (*delay_pos + 1) % buf_len;

        // 混合干湿信号
        let out = distorted * (1.0 - delay_mix) + delayed * delay_mix;
        output.push(out.clamp(-32767.0, 32767.0));
    }

    output
}
