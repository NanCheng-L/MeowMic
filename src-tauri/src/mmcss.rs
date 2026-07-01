/// MMCSS (Multimedia Class Scheduler Service) — 提升音频线程到 "Pro Audio" 优先级。
/// 没有这个，普通线程调度在 CPU 负载高时会产生 2-10ms 的随机停顿，导致可听到的音频毛刺。

#[cfg(windows)]
pub struct ProAudio(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ProAudio {
    pub fn set_for_current_thread() -> Option<Self> {
        use windows::core::w;
        use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;

        unsafe {
            let mut task_index: u32 = 0;
            match AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index) {
                Ok(h) if !h.is_invalid() => Some(Self(h)),
                _ => {
                    log::warn!("AvSetMmThreadCharacteristicsW failed; running at normal priority");
                    None
                }
            }
        }
    }
}

#[cfg(windows)]
impl Drop for ProAudio {
    fn drop(&mut self) {
        use windows::Win32::System::Threading::AvRevertMmThreadCharacteristics;
        unsafe {
            let _ = AvRevertMmThreadCharacteristics(self.0);
        }
    }
}
