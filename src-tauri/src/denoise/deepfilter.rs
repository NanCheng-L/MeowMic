use super::DenoiseModel;
use std::ffi::c_void;

/// DeepFilterNet3 降噪模型（通过 FFI 调用 deepfilter_runtime_bridge.dll）
///
/// 模型数据已编译进 DLL，不需要外部模型文件。
/// DLL 内部处理 STFT/iSTFT、ERB 特征提取、深度滤波推理。
pub struct DeepFilterFFI {
    _lib: libloading::Library,
    state: *mut c_void,
    frame_size: usize,
    #[allow(dead_code)]
    sample_rate: usize,
    channels: usize,
    set_atten_lim: FnSetAttenLim,
    _set_post_filter_beta: FnSetPostFilterBeta,
    /// 预分配缓冲区，避免音频热路径堆分配
    norm_input: Vec<f32>,
    norm_output: Vec<f32>,
}

// DLL 函数签名
type FnCreate = unsafe extern "C" fn(usize, f32, f32, i32) -> *mut c_void;
type FnFree = unsafe extern "C" fn(*mut c_void);
type FnGetFrameLength = unsafe extern "C" fn(*const c_void) -> usize;
type FnGetSampleRate = unsafe extern "C" fn(*const c_void) -> usize;
type FnGetChannelCount = unsafe extern "C" fn(*const c_void) -> usize;
type FnProcessFrame = unsafe extern "C" fn(*mut c_void, *const f32, *mut f32) -> f32;
type FnSetAttenLim = unsafe extern "C" fn(*mut c_void, f32);
type FnSetPostFilterBeta = unsafe extern "C" fn(*mut c_void, f32);

unsafe impl Send for DeepFilterFFI {}

impl DeepFilterFFI {
    /// 根据 strength (0-1) 更新 DeepFilterNet 内部降噪强度
    ///
    /// atten_lim_db 的含义是"最大衰减限制"：
    /// - atten_lim_db >= 100：不限制（最大降噪）
    /// - atten_lim_db = 15：限制在 15dB（保留更多原始信号）
    /// - 内部转换：lim = 10^(-dB/20)，atten_lim_db 越大降噪越强
    ///
    /// 线性映射：strength * 100
    /// - strength 0.0 → atten_lim_db 0（不降噪）
    /// - strength 0.5 → atten_lim_db 50（中等降噪）
    /// - strength 1.0 → atten_lim_db 100（最大降噪）
    pub fn update_strength(&self, strength: f32) {
        if !self.state.is_null() {
            // 非线性映射：低值区更敏感，让用户在常用区间有更精细的控制
            let atten_lim_db = strength * 100.0;
            unsafe {
                (self.set_atten_lim)(self.state, atten_lim_db);
            }
        }
    }

    pub fn new(resource_dir: &std::path::Path) -> Result<Self, String> {
        let dll_path = resource_dir.join("deepfilter").join("deepfilter_runtime_bridge.dll");
        if !dll_path.exists() {
            return Err(format!("DeepFilterNet DLL not found: {}", dll_path.display()));
        }

        let lib = unsafe {
            libloading::Library::new(&dll_path)
                .map_err(|e| format!("Failed to load DLL {}: {}", dll_path.display(), e))?
        };

        unsafe {
            let create: FnCreate = *lib.get(b"dfgui_create")
                .map_err(|e| format!("dfgui_create not found: {}", e))?;
            let get_frame_len: FnGetFrameLength = *lib.get(b"dfgui_get_frame_length")
                .map_err(|e| format!("dfgui_get_frame_length not found: {}", e))?;
            let get_sr: FnGetSampleRate = *lib.get(b"dfgui_get_sample_rate")
                .map_err(|e| format!("dfgui_get_sample_rate not found: {}", e))?;
            let get_ch: FnGetChannelCount = *lib.get(b"dfgui_get_channel_count")
                .map_err(|e| format!("dfgui_get_channel_count not found: {}", e))?;
            let set_atten_lim: FnSetAttenLim = *lib.get(b"dfgui_set_atten_lim")
                .map_err(|e| format!("dfgui_set_atten_lim not found: {}", e))?;
            let set_post_filter_beta: FnSetPostFilterBeta = *lib.get(b"dfgui_set_post_filter_beta")
                .map_err(|e| format!("dfgui_set_post_filter_beta not found: {}", e))?;

            // channels=1 (mono), atten_lim_db=100 (不限制), post_filter_beta=0.02, reduce_mask=0 (NONE, 与 GUI 默认一致)
            let state = create(1, 100.0, 0.02, 0);
            if state.is_null() {
                return Err("dfgui_create returned null".into());
            }

            let frame_size = get_frame_len(state as *const c_void);
            let sample_rate = get_sr(state as *const c_void);
            let channels = get_ch(state as *const c_void);

            log::info!(
                "DeepFilterNet FFI loaded: frame_size={}, sr={}, ch={}",
                frame_size, sample_rate, channels
            );

            Ok(Self {
                _lib: lib,
                state,
                frame_size,
                sample_rate,
                channels,
                set_atten_lim,
                _set_post_filter_beta: set_post_filter_beta,
                norm_input: vec![0.0f32; frame_size],
                norm_output: vec![0.0f32; frame_size],
            })
        }
    }
}

impl DenoiseModel for DeepFilterFFI {
    fn name(&self) -> &str {
        "DeepFilterNet3"
    }

    fn update_strength(&mut self, strength: f32) {
        // 调用 inherent 方法更新 atten_lim_db
        DeepFilterFFI::update_strength(self, strength);
    }

    fn has_internal_strength_control(&self) -> bool {
        true // DeepFilterNet 通过 atten_lim_db 内部控制强度，不需要外部 strength mixing
    }

    fn process_frame(&mut self, output: &mut [f32], input: &[f32]) {
        if self.state.is_null() {
            output.fill(0.0);
            return;
        }

        // DLL 期望 mono interleaved（单声道时就是顺序排列）
        // input/output 长度应为 frame_size * channels
        let expected_len = self.frame_size * self.channels;
        if input.len() < expected_len || output.len() < expected_len {
            log::warn!(
                "DeepFilterNet: frame size mismatch, input={} output={} expected={}",
                input.len(), output.len(), expected_len
            );
            // 尝试用实际长度
        }

        let len = input.len().min(output.len()).min(expected_len);

        unsafe {
            let process: FnProcessFrame = std::mem::transmute(
                *self._lib.get::<FnProcessFrame>(b"dfgui_process_frame")
                    .expect("dfgui_process_frame not found")
            );

            // 管线是 normalized f32 [-1.0, 1.0]，DLL 也期望此范围，直接传入
            for i in 0..len {
                self.norm_input[i] = input[i];
            }
            let _attenuation = process(self.state, self.norm_input.as_ptr(), self.norm_output.as_mut_ptr());

            for i in 0..len {
                output[i] = self.norm_output[i];
            }

            // 对于输出不足的部分，用 0 填充
            if len < output.len() {
                output[len..].fill(0.0);
            }
        }
    }
}

impl Drop for DeepFilterFFI {
    fn drop(&mut self) {
        if !self.state.is_null() {
            unsafe {
                if let Ok(free) = self._lib.get::<FnFree>(b"dfgui_free") {
                    free(self.state);
                }
            }
            self.state = std::ptr::null_mut();
        }
    }
}
