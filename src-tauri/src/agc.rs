/// 简易 AGC（Automatic Gain Control）
///
/// 参考 dagc（sile/dagc）和 WebRTC AGC 的设计：
/// - 只在检测到人声时更新增益，安静时增益冻结（不放大底噪）
/// - 增益平滑过渡，避免突变产生杂音
/// - 内置噪声门，信号极小时衰减输出

/// AGC 状态（DSP 线程本地，无需线程安全）
pub struct AgcState {
    /// 当前增益（线性）
    gain: f32,
    /// 平滑后的 RMS（用于增益计算）
    smoothed_rms: f32,
    /// 噪声门状态：0.0=关闭，1.0=打开
    gate: f32,
    initialized: bool,
}

impl AgcState {
    pub fn new() -> Self {
        Self {
            gain: 1.0,
            smoothed_rms: 0.001,
            gate: 1.0,
            initialized: false,
        }
    }

    /// 重置状态（切换模式时调用）
    pub fn reset(&mut self) {
        self.gain = 1.0;
        self.smoothed_rms = 0.001;
        self.gate = 1.0;
        self.initialized = false;
    }

    /// 处理一帧，返回当前实际增益（用于 UI 显示）
    ///
    /// `input_rms`: 降噪后、增益前的帧 RMS（用于 VAD 判断）
    pub fn process_frame(
        &mut self,
        frame: &mut [f32],
        target_rms: f32,
        input_rms: f32,
    ) -> f32 {
        if frame.is_empty() {
            return self.gain;
        }

        // ── VAD：信号高于阈值才算有人声 ──
        // 阈值 = 目标电平的 1/20（如 -30dB 目标 → -54dB 阈值）
        let vad_threshold = target_rms * 0.05;
        let has_voice = input_rms > vad_threshold;

        // ── 只在有人声时更新增益（核心：安静时不放大）──
        if has_voice {
            // 平滑 RMS
            if !self.initialized {
                self.smoothed_rms = input_rms.max(1e-6);
                self.initialized = true;
            } else {
                let speed = if input_rms > self.smoothed_rms { 0.3 } else { 0.05 };
                self.smoothed_rms += (input_rms - self.smoothed_rms) * speed;
                self.smoothed_rms = self.smoothed_rms.max(1e-6);
            }

            // 计算需要的增益
            let target = target_rms.max(1e-6);
            let desired_gain = target / self.smoothed_rms;

            // 平滑增益过渡
            let error = (desired_gain - self.gain).abs();
            let gain_speed = if error > 1.0 {
                0.3
            } else if desired_gain > self.gain {
                0.15
            } else {
                0.05
            };
            self.gain += (desired_gain - self.gain) * gain_speed;
            self.gain = self.gain.clamp(0.1, 20.0);
        }
        // 非人声帧：增益冻结，不更新

        // ── 噪声门：信号极小时衰减 ──
        let gate_threshold = vad_threshold;
        let target_gate = if input_rms > gate_threshold { 1.0 } else { 0.0 };
        let gate_speed = if target_gate > self.gate { 0.15 } else { 0.05 };
        self.gate += (target_gate - self.gate) * gate_speed;

        // 应用增益 × 噪声门
        let g = self.gain * self.gate;
        for sample in frame.iter_mut() {
            *sample *= g;
        }

        g
    }
}
