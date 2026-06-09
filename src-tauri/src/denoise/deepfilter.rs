use super::DenoiseModel;
use std::collections::VecDeque;
use std::path::PathBuf;
use tract_onnx::prelude::*;

use df::Complex32;
use df::DFState;

/// DeepFilterNet3 降噪模型
///
/// 使用 T=3 多帧批处理，让编码器 GRU 获得跨帧时序上下文。
/// 初始化时用静音帧预热 GRU，减少启动延迟。
pub struct DeepFilterModel {
    df_state: DFState,
    enc: TypedRunnableModel<TypedModel>,
    erb_dec: TypedRunnableModel<TypedModel>,
    df_dec: TypedRunnableModel<TypedModel>,
    /// ERB 特征缓冲：固定 T 帧，每帧 NB_ERB 个 f32
    erb_frames: Vec<Vec<f32>>,
    /// 频谱特征缓冲：固定 T 帧，每帧 freq_size*2 个 f32
    spec_frames: Vec<Vec<f32>>,
    /// 频谱历史（深度滤波用）
    spec_history: VecDeque<Vec<Complex32>>,
    /// ERB 归一化状态
    erb_norm_state: Vec<f32>,
    /// 频谱归一化状态
    spec_norm_state: Vec<f32>,
}

const T: usize = 3;
const DF_ORDER: usize = 5;
const NB_DF: usize = 96;
const NB_ERB: usize = 32;

impl DeepFilterModel {
    pub fn new(resource_dir: &PathBuf) -> Result<Self, String> {
        let df_state = DFState::default();
        let freq_size = df_state.freq_size;

        let enc_path = resource_dir.join("enc.onnx");
        let erb_dec_path = resource_dir.join("erb_dec.onnx");
        let df_dec_path = resource_dir.join("df_dec.onnx");

        let enc = Self::load_model(&enc_path, &[
            (0, vec![1, 1, T, NB_ERB]),
            (1, vec![1, 2, T, 96]),
        ])?;

        let erb_dec = Self::load_model(&erb_dec_path, &[
            (0, vec![1, T, 512]),
            (1, vec![1, 64, T, 8]),
            (2, vec![1, 64, T, 8]),
            (3, vec![1, 64, T, 16]),
            (4, vec![1, 64, T, 32]),
        ])?;

        let df_dec = Self::load_model(&df_dec_path, &[
            (0, vec![1, T, 512]),
            (1, vec![1, 64, T, 96]),
        ])?;

        // MEAN_NORM_INIT: 从 -60 到 -90 线性插值（与 df crate 一致）
        let erb_norm_state: Vec<f32> = (0..NB_ERB)
            .map(|i| -60.0 + (-90.0 - (-60.0)) * i as f32 / (NB_ERB - 1) as f32)
            .collect();
        // UNIT_NORM_INIT: 从 0.001 到 0.0001 线性插值（仅前 NB_DF 个 bin）
        let spec_norm_state: Vec<f32> = (0..NB_DF)
            .map(|i| 0.001 + (0.0001 - 0.001) * i as f32 / (NB_DF - 1) as f32)
            .collect();

        let model = Self {
            df_state,
            enc,
            erb_dec,
            df_dec,
            erb_frames: vec![vec![0.0; NB_ERB]; T],
            spec_frames: vec![vec![0.0; freq_size * 2]; T],
            spec_history: VecDeque::with_capacity(DF_ORDER + 1),
            erb_norm_state,
            spec_norm_state,
        };

        // 跳过 warmup — ONNX 推理开销大，会阻塞音频线程
        // GRU 从零状态开始，几帧后自然适应
        Ok(model)
    }

    fn load_model(
        path: &PathBuf,
        input_shapes: &[(usize, Vec<usize>)],
    ) -> Result<TypedRunnableModel<TypedModel>, String> {
        let mut model = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(|e| format!("load {}: {}", path.display(), e))?;
        for &(idx, ref shape) in input_shapes {
            model = model
                .with_input_fact(idx, InferenceFact::dt_shape(f32::datum_type(), shape.as_slice()).into())
                .map_err(|e| format!("set fact {}: {}", idx, e))?;
        }
        model
            .into_optimized()
            .map_err(|e| format!("optimize {}: {}", path.display(), e))?
            .into_runnable()
            .map_err(|e| format!("runnable {}: {}", path.display(), e))
    }

    fn extract_erb(&self, spec: &[Complex32]) -> Vec<f32> {
        let mut erb = vec![0.0f32; NB_ERB];
        let mut cumsum = 0;
        for (erb_idx, &band_size) in self.df_state.erb.iter().enumerate() {
            let mut energy = 0.0f32;
            for j in 0..band_size {
                let idx = cumsum + j;
                if idx < spec.len() {
                    energy += spec[idx].norm_sqr(); // 功率 |x|^2
                }
            }
            // 转 dB：10*log10(power)，与 libdf erb(db=True) 一致
            erb[erb_idx] = 10.0 * (energy / band_size as f32 + 1e-10).log10();
            cumsum += band_size;
        }
        erb
    }

    fn extract_spec(&self, spec: &[Complex32]) -> Vec<f32> {
        let freq_size = self.df_state.freq_size;
        let mut features = Vec::with_capacity(freq_size * 2);
        for i in 0..freq_size {
            if i < spec.len() {
                features.push(spec[i].re);
                features.push(spec[i].im);
            } else {
                features.push(0.0);
                features.push(0.0);
            }
        }
        features
    }

    /// EMA alpha = exp(-hop_size / sr / tau) = exp(-480/48000/1.0) ≈ 0.99
    const NORM_ALPHA: f32 = 0.9900498;

    fn normalize_erb(features: &mut [f32], state: &mut [f32]) {
        for (f, s) in features.iter_mut().zip(state.iter_mut()) {
            *s = *f * (1.0 - Self::NORM_ALPHA) + *s * Self::NORM_ALPHA;
            *f -= *s;
            *f /= 40.0;
        }
    }

    /// 频谱归一化：仅前 NB_DF (96) 个 bin 做 EMA unit norm（与 Python unit_norm(spec[..., :nb_df]) 一致）
    fn normalize_spec_complex(spec: &mut [Complex32], state: &mut [f32]) {
        let n = spec.len().min(NB_DF).min(state.len());
        for i in 0..n {
            let s = &mut state[i];
            let x = &mut spec[i];
            *s = x.norm() * (1.0 - Self::NORM_ALPHA) + *s * Self::NORM_ALPHA;
            if *s > 1e-8 {
                *x /= s.sqrt();
            }
        }
    }

    /// 推入一帧特征到缓冲区（FIFO：新帧放末尾，丢弃最旧帧）
    fn push_frame(&mut self, erb: Vec<f32>, spec: Vec<f32>) {
        self.erb_frames.remove(0);
        self.erb_frames.push(erb);
        self.spec_frames.remove(0);
        self.spec_frames.push(spec);
    }

    /// 执行编码器 + 两个解码器的推理
    fn run_inference(&mut self) -> (Vec<f32>, Vec<f32>) {
        // feat_erb: [1, 1, T, 32]
        let mut erb_flat = Vec::with_capacity(T * NB_ERB);
        for frame in &self.erb_frames {
            debug_assert_eq!(frame.len(), NB_ERB);
            erb_flat.extend_from_slice(frame);
        }
        let feat_erb_tensor = Tensor::from_shape(&[1, 1, T, NB_ERB], &erb_flat).unwrap();

        // feat_spec: [1, 2, T, 96]
        let mut spec_flat = Vec::with_capacity(2 * T * 96);
        for frame in &self.spec_frames {
            for bin in 0..96 {
                spec_flat.push(frame.get(bin * 2).copied().unwrap_or(0.0));
            }
        }
        for frame in &self.spec_frames {
            for bin in 0..96 {
                spec_flat.push(frame.get(bin * 2 + 1).copied().unwrap_or(0.0));
            }
        }
        let feat_spec_tensor = Tensor::from_shape(&[1, 2, T, 96], &spec_flat).unwrap();

        // Encoder
        let enc_out = self.enc.run(tvec![
            feat_erb_tensor.into(),
            feat_spec_tensor.into(),
        ]).unwrap();

        // ERB Decoder
        let erb_out = self.erb_dec.run(tvec![
            enc_out[4].clone(), enc_out[3].clone(), enc_out[2].clone(),
            enc_out[1].clone(), enc_out[0].clone(),
        ]).unwrap();

        let erb_mask_data = erb_out[0].as_slice::<f32>().unwrap();
        let last_offset = (T - 1) * NB_ERB;
        let erb_mask: Vec<f32> = erb_mask_data[last_offset..last_offset + NB_ERB]
            .iter().map(|x| x.clamp(0.0, 1.0)).collect();

        // DF Decoder
        let df_out = self.df_dec.run(tvec![
            enc_out[4].clone(), enc_out[5].clone(),
        ]).unwrap();

        let df_coefs_data = df_out[0].as_slice::<f32>().unwrap();
        let df_offset = (T - 1) * NB_DF * DF_ORDER * 2;
        let df_coefs: Vec<f32> = df_coefs_data[df_offset..df_offset + NB_DF * DF_ORDER * 2].to_vec();

        (erb_mask, df_coefs)
    }

    fn apply_erb_mask(spec: &mut [Complex32], mask: &[f32], erb_fb: &[usize]) {
        // power=0.5 软化增益：0.25→0.5, 0.5→0.71, 保留噪声抑制方向但不过度压制
        let mut cumsum = 0;
        for (&band_size, &gain) in erb_fb.iter().zip(mask.iter()) {
            let adjusted = gain.powf(0.5);
            for j in 0..band_size {
                let idx = cumsum + j;
                if idx < spec.len() { spec[idx] *= adjusted; }
            }
            cumsum += band_size;
        }
    }

    fn apply_df(spec: &mut [Complex32], coefs: &[f32], history: &VecDeque<Vec<Complex32>>, current: &[Complex32]) {
        for freq_idx in 0..spec.len().min(NB_DF) {
            let base = freq_idx * DF_ORDER * 2;
            if base + DF_ORDER * 2 > coefs.len() { break; }
            let mut acc = Complex32::new(0.0, 0.0);
            for tap in 0..DF_ORDER {
                let coef = Complex32::new(coefs[base + tap * 2], coefs[base + tap * 2 + 1]);
                if tap == 0 {
                    if freq_idx < current.len() { acc += current[freq_idx] * coef; }
                } else {
                    let hist_idx = history.len().saturating_sub(tap);
                    if hist_idx < history.len() {
                        let frame = &history[hist_idx];
                        if freq_idx < frame.len() { acc += frame[freq_idx] * coef; }
                    }
                }
            }
            spec[freq_idx] = acc;
        }
    }
}

impl DenoiseModel for DeepFilterModel {
    fn name(&self) -> &str { "DeepFilterNet" }

    fn process_frame(&mut self, output: &mut [f32], input: &[f32]) {
        // 1. STFT → 原始频谱
        let mut raw_spec = vec![Complex32::default(); self.df_state.freq_size];
        self.df_state.analysis(input, &mut raw_spec);

        // 2. ERB 特征：从原始频谱提取功率(dB)，EMA 归一化
        let mut erb_feat = self.extract_erb(&raw_spec);
        Self::normalize_erb(&mut erb_feat, &mut self.erb_norm_state);

        // 3. 频谱归一化（仅用于编码器输入，mask/DF 作用于原始频谱）
        let mut norm_spec = raw_spec.clone();
        Self::normalize_spec_complex(&mut norm_spec, &mut self.spec_norm_state);

        // 4. 从归一化频谱提取 interleaved re/im 特征
        let spec_feat = self.extract_spec(&norm_spec);

        // 5. 推入缓冲（FIFO）
        self.push_frame(erb_feat, spec_feat);

        // 6. 推理
        let (erb_mask, df_coefs) = self.run_inference();

        // 7. 深度滤波（作用于原始频谱）
        let current_spec = raw_spec.clone();
        Self::apply_df(&mut raw_spec, &df_coefs, &self.spec_history, &current_spec);
        self.spec_history.push_back(current_spec);
        if self.spec_history.len() > DF_ORDER + 1 { self.spec_history.pop_front(); }

        // 8. ERB mask（power=0.5 软化增益）
        Self::apply_erb_mask(&mut raw_spec, &erb_mask, &self.df_state.erb);

        // 9. iSTFT
        self.df_state.synthesis(&mut raw_spec, output);
    }

    fn save_state(&self) -> Option<Vec<u8>> {
        let mut buf = Vec::new();
        for v in &self.erb_norm_state { buf.extend_from_slice(&v.to_le_bytes()); }
        for v in &self.spec_norm_state { buf.extend_from_slice(&v.to_le_bytes()); }
        Some(buf)
    }

    fn load_state(&mut self, state: &[u8]) {
        let expected = (self.erb_norm_state.len() + self.spec_norm_state.len()) * 4;
        if state.len() != expected { return; }
        let mut offset = 0;
        for v in self.erb_norm_state.iter_mut() {
            *v = f32::from_le_bytes([state[offset], state[offset+1], state[offset+2], state[offset+3]]);
            offset += 4;
        }
        for v in self.spec_norm_state.iter_mut() {
            *v = f32::from_le_bytes([state[offset], state[offset+1], state[offset+2], state[offset+3]]);
            offset += 4;
        }
    }
}
