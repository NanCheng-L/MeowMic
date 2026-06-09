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
        self.state.process_frame(output, input);
    }
}
