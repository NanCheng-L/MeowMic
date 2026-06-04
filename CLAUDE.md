# pico-denoise

直播/游戏麦克风降噪桌面应用。Tauri 2 + Vue 3 + TypeScript + Rust。

## 技术栈

- **前端**：Vue 3 + TypeScript + Vite
- **后端**：Rust (Tauri 2)
- **音频 API**：WASAPI (Windows Audio Session API)
- **降噪**：nnnoiseless（RNNoise 的纯 Rust 实现）
- **虚拟音频设备**：VB-Audio Virtual Cable
- **全局快捷键**：tauri-plugin-global-shortcut
- **配置持久化**：tauri-plugin-store
- **开机自启动**：tauri-plugin-autostart

## 开发命令

```bash
pnpm tauri dev          # 启动开发模式
pnpm tauri build        # 构建安装包
cargo check             # 检查 Rust 编译（在 src-tauri/ 下）
```

## 项目结构

```
src/                    # Vue 前端
  components/           # UI 组件
  composables/          # Vue 组合式函数
src-tauri/src/          # Rust 后端
  audio_engine.rs       # WASAPI 音频引擎
  lib.rs                # Tauri 命令注册 + 系统托盘 + 设置管理
```

## 踩坑警示

- **WASAPI API**：`WaveFormat::new()` 参数顺序是 `(storebits, validbits, &SampleType, samplerate, channels, channel_mask)`，不是 channels 在前
- **WASAPI 初始化**：用 `initialize_mta()` 不是 `initialize()`
- **设备枚举**：用 `DeviceCollection::new(&Direction)` + `get_device_at_index(i)`，没有 `.iter()` 方法
- **WASAPI Direction**：`Direction::Capture` = 录音设备（麦克风），`Direction::Render` = 播放设备（扬声器/VB-Cable）。`initialize_client` 的 direction 参数要和设备方向一致
- **Windows PATH**：pnpm 通过 npm 全局安装后，bash 环境需通过 `node.exe pnpm.mjs` 调用，或在 PowerShell 中设置 PATH
- **Tauri autostart 插件**：API 方法名是 `autolaunch()` 不是 `autostart()`（v3 会改名），`ManagerExt` trait 必须 `use` 到作用域
- **Tauri global-shortcut 插件**：没有 `init()` 函数，用 `Builder::new().build()`；权限名用连字符 `allow-is-registered` 不是下划线
- **WASAPI Process Loopback**：`new_application_loopback_client(pid, true)` 的 `get_mixformat()` 和 `get_periods()` 返回 `E_NOTIMPL`，必须用固定格式 `WaveFormat::new(32, 32, &SampleType::Float, 48000, 2, None)` + `initialize_client` period 传 0
- **Windows ToolHelp API**：枚举进程用 `CreateToolhelp32Snapshot` + `Process32FirstW/NextW`，需要 `Win32_System_Diagnostics_ToolHelp` feature

## 红线

- 密钥、token、密码不进代码
- 不注释报错来绕过问题
- 大改动前出方案确认
