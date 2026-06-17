mod rnnoise;

use rnnoise::RnnoiseModel;

/// 降噪模型统一接口
/// 所有模型约束：480 samples/帧、48kHz、f32 输入输出
pub trait DenoiseModel: Send {
    /// 模型显示名称
    fn name(&self) -> &str;
    /// 处理一帧音频（480 samples）
    fn process_frame(&mut self, output: &mut [f32], input: &[f32]);
    /// 保存模型内部状态（归一化统计等），用于切换模型时保持适应性
    fn save_state(&self) -> Option<Vec<u8>> { None }
    /// 恢复模型内部状态
    fn load_state(&mut self, _state: &[u8]) {}
}

/// 帧大小常量（480 samples = 10ms @ 48kHz）
pub const FRAME_SIZE: usize = 480;

/// 列出所有可用模型
pub fn list_models() -> Vec<&'static str> {
    vec!["RNNoise"]
}

/// 根据名称创建模型实例
pub fn create_model(name: &str, _resource_dir: Option<&std::path::Path>) -> Box<dyn DenoiseModel> {
    match name {
        "RNNoise" => Box::new(RnnoiseModel::new()),
        _ => Box::new(RnnoiseModel::new()),
    }
}
