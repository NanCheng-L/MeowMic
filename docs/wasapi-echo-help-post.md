# WASAPI Shared Mode: 物理音频设备启动时 1-2 秒回声

## 环境
- Windows + Rust + `windows 0.58` crate
- WASAPI Shared Mode，Event-driven
- 三线程架构：Capture → Ring → DSP → Ring → Render

## 问题
输出到**物理音频设备**（Realtek 耳机，3.5mm 接口）时，启动后有 **1-2 秒回声**（自己的声音被重复播放），然后自动消失。

## 关键发现
- 输出到 **VB-Cable（虚拟声卡）= 无回声**
- 输出到 **Realtek 耳机 = 有回声**（1-2 秒后消失）
- **同一个 WASAPI 代码**，CLI 应用无回声，Tauri 应用有回声
- 关闭 Windows 音频增强无效
- `Reset()` + 预填充静音 + 清空 capture 缓冲区，均无效

## 已尝试
- `IAudioClient::Reset()` 后再 `Start()`
- `ReleaseBuffer` 预填充 `AUDCLNT_BUFFERFLAGS_SILENT`
- `GetCurrentPadding()` 检查后再写入
- 使用 `GetMixFormat()` 原生格式
- 关闭 Windows 音频增强

## Render 初始化代码
```rust
client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0, mix_ptr, None)?;
let event = CreateEventW(None, false, false, PCWSTR::null())?;
client.SetEventHandle(event)?;
let render_client: IAudioRenderClient = client.GetService()?;
let buffer_frames = client.GetBufferSize()?;

// 预填充静音
let prefill = render_client.GetBuffer(buffer_frames)?;
std::ptr::write_bytes(prefill, 0, (buffer_frames * device_block_align) as usize);
render_client.ReleaseBuffer(buffer_frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
client.Start()?;

// 主循环
while running {
    WaitForSingleObject(event, 200);
    let padding = client.GetCurrentPadding()?;
    let writable = buffer_frames.saturating_sub(padding);
    if writable == 0 { continue; }
    let buf = render_client.GetBuffer(writable)?;
    // 写入 f32 stereo 数据
    render_client.ReleaseBuffer(writable, 0)?;
}
```

## 问题
1. 为什么虚拟设备无回声，物理设备有？
2. Realtek 驱动是否有内部缓冲区保留旧音频？
3. 如何强制清空硬件缓冲区？

## 参考
- 问题 Issue：https://github.com/NanCheng-L/MeowMic/issues/3
- 对比项目（无回声）：https://github.com/Yashsomalkar/noisegate
