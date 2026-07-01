use super::DenoiseModel;

pub struct RnnoiseModel {
    state: Box<nnnoiseless::DenoiseState<'static>>,
}

impl RnnoiseModel {
    pub fn new() -> Self {
        Self {
            state: nnnoiseless::DenoiseState::new(),
        }
    }
}

impl DenoiseModel for RnnoiseModel {
    fn name(&self) -> &str {
        "RNNoise"
    }

    fn process_frame(&mut self, output: &mut [f32], input: &[f32]) {
        // RNNoise 期望 i16 范围，管线是 normalized [-1, 1]
        // 参考 noisegate-ref crates/dsp/src/rnnoise.rs:40-56
        const I16_SCALE: f32 = 32_768.0;
        let mut scaled_in = [0.0f32; 480];
        for (dst, &src) in scaled_in.iter_mut().zip(input.iter()) {
            *dst = src * I16_SCALE;
        }
        let mut scaled_out = [0.0f32; 480];
        self.state.process_frame(&mut scaled_out, &scaled_in);
        for (dst, &src) in output.iter_mut().zip(scaled_out.iter()) {
            *dst = src / I16_SCALE;
        }
    }
}
