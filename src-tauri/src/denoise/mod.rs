mod rnnoise;
mod deepfilter;

use rnnoise::RnnoiseModel;
use deepfilter::DeepFilterFFI;

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
    /// 更新降噪强度（0-1），仅 DeepFilterNet 支持
    fn update_strength(&mut self, _strength: f32) {}
    /// 是否由模型内部控制强度（如果是，外部不做 strength mixing）
    fn has_internal_strength_control(&self) -> bool { false }
}

/// 帧大小常量（480 samples = 10ms @ 48kHz）
pub const FRAME_SIZE: usize = 480;

/// 列出所有可用模型
pub fn list_models() -> Vec<&'static str> {
    vec!["RNNoise", "DeepFilterNet3"]
}

/// 根据名称创建模型实例
pub fn create_model(name: &str, resource_dir: Option<&std::path::Path>) -> Box<dyn DenoiseModel> {
    match name {
        "DeepFilterNet3" => {
            if let Some(dir) = resource_dir {
                match DeepFilterFFI::new(dir) {
                    Ok(model) => {
                        log::info!("DeepFilterNet model loaded from {}", dir.display());
                        Box::new(model)
                    }
                    Err(e) => {
                        log::warn!("DeepFilterNet load failed: {}, falling back to RNNoise", e);
                        Box::new(RnnoiseModel::new())
                    }
                }
            } else {
                log::warn!("No resource_dir, falling back to RNNoise");
                Box::new(RnnoiseModel::new())
            }
        }
        _ => Box::new(RnnoiseModel::new()),
    }
}
