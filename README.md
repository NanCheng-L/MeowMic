# 喵咪麦克（MeowMic）

> 🐱🎤 实时麦克风降噪桌面应用，为直播和游戏场景打造。

基于 WASAPI 实时音频处理，支持 RNNoise 降噪模型，开箱即用。

## 下载

[![GitHub Release](https://img.shields.io/github/v/release/NanCheng-L/MeowMic)](https://github.com/NanCheng-L/MeowMic/releases/latest)

👉 [**点击下载最新版本**](https://github.com/NanCheng-L/MeowMic/releases/latest)

> 进入下载页面后，找到 `.exe` 后缀的文件（如 `MeowMic_0.2.3_x64-setup.exe`），点击下载并双击运行安装。

<table>
  <tr>
    <td><img width="380" alt="d63096ca-beae-4286-ab34-781599ef8131" src="https://github.com/user-attachments/assets/01a89f2d-ad96-4e58-8675-bcf860c727de" /></td>
    <td><img width="380" alt="41e71233-b375-4440-ae58-29a64a9b0c6d" src="https://github.com/user-attachments/assets/4a282f7c-2960-4c34-beac-a7435b7262b8" /></td>
  </tr>
</table>


## 功能

- 实时麦克风降噪（WASAPI Shared 模式）
- 多降噪模型架构（可扩展）
  - RNNoise：轻量级，擅长去除风扇、空调等持续噪音，CPU 占用极低
  - MeowMic：自训练深度学习模型，擅长去除键盘、鼠标等瞬态噪音（训练中）
- 预设模式（安静/标准/嘈杂/直播 + 自定义）
- 输入/输出设备选择 + 设备热拔插自动检测
- 降噪强度可调（0-100%）
- 电平表 + 频谱可视化
- 系统托盘（最小化到托盘）
- 全局快捷键（5 个可自定义：降噪/EQ/BGM/炸麦/监听，支持单键或组合键）
- 开机自启动
- BGM 混音（按进程捕获音乐播放器音频，混合到麦克风输出）
- 一键炸麦（恶搞模式，增益 + 方波失真，强度可调 1-100%）
- 监听模式（5 个监听点可切换：原始输入/降噪后/增益后/EQ后/最终输出，自动检测同 USB 设备冲突）
- 均衡器（10 段可视化 EQ，参考 SteelSeries GG Sonar 设计，Canvas 曲线图 + 可拖拽圆点 + 7 个预设 + 自定义曲线持久化）
- 设置界面（独立窗口，5 个快捷键配置、开机自启开关、语言切换）
- 使用教程（独立窗口，设备说明 + FAQ + 社交链接）
- VB-Audio Virtual Cable 虚拟设备引导
- 多语言支持（中文 / English）
- 主题切换（深色 / 浅色，默认浅色暖色调）
- 自动更新（检测新版本 → 下载 → 静默安装）

## 架构

### 前端组件

| 组件 | 说明 | 窗口 |
|------|------|------|
| DeviceSelector | 设备选择 | 主窗口 |
| DenoiseControl | 降噪控制（开关+强度+预设） | 主窗口 |
| BgmMixer | BGM 混音（按进程捕获） | 主窗口 |
| EqControl | 均衡器开关按钮 | 主窗口 |
| ExplodeButton | 一键炸麦 | 主窗口 |
| AudioMeter | 电平表 + 频谱可视化 | 主窗口 |
| EqPage | 均衡器界面（Canvas 曲线图） | 独立窗口 |
| SettingsPage | 设置界面 | 独立窗口 |
| TutorialPage | 使用教程 | 独立窗口 |

### 后端模块

| 模块 | 说明 |
|------|------|
| audio_engine.rs | WASAPI 音频引擎（采集→处理→输出） |
| denoise/ | 降噪模型（RNNoise / MeowMic） |
| eq.rs | EQ 均衡器（Biquad IIR 滤波器） |
| device_watcher.rs | 设备热拔插检测 |
| lib.rs | Tauri 命令注册 + 系统托盘 |

### 降噪架构

```
┌─────────────────┐                ┌─────────────────┐
│ RNNoise (原生)   │                │ MeowMic (ONNX)  │
│ <1ms, 持续噪声   │                │ ~3ms, 瞬态噪声  │
└─────────────────┘                └─────────────────┘
         │                                  │
         └──────────┬───────────────────────┘
                    │
              Rust 音频引擎
              (WASAPI 采集/输出)
```

### 音频处理管线

```
麦克风采集 → 重采样 → 降噪 → 强度混合 → 增益 → [EQ] → 炸麦 → BGM混音 → 软限制 → 输出
                                                       ↑
                                               音乐播放器进程
```

### 多窗口架构

```
主窗口 (main)
  ├── DeviceSelector (设备选择)
  ├── DenoiseControl (降噪控制)
  ├── BgmMixer (BGM混音)
  ├── EqControl (均衡器开关)
  ├── ExplodeButton (炸麦)
  └── AudioMeter (电平表)

设置窗口 (settings)        教程窗口 (tutorial)       EQ窗口 (eq)
  ├── 快捷键配置             ├── 设备说明              ├── Canvas曲线图
  ├── 开机自启               ├── FAQ                   ├── 可拖拽圆点
  ├── 语言切换               └── 社交链接              ├── 预设切换
  ├── 主题切换                                       └── 悬停提示框
  └── 检查更新

所有窗口通过 Tauri IPC (invoke/listen) 与 Rust 后端通信
```

## 为什么选 MeowMic

- **资源占用极低** — RNNoise 降噪 CPU 占用 < 1%，不抢游戏帧数
- **功能灵活可扩展** — 开源架构，BGM 混音、监听、炸麦想加就加
- **不挑硬件** — 任意麦克风 + VB-Cable 即可使用，不需要特定显卡或品牌设备
- **不绑生态** — 没有厂商锁定，没有账户系统，完全本地运行

## 环境要求

- Windows 10/11
- Node.js 18+
- pnpm
- Rust + Cargo
- [VB-Audio Virtual Cable](https://vb-audio.com/Cable/)（输出端需要）

## 兼容性

| 项目 | 说明 |
|------|------|
| 操作系统 | Windows 10 (1809+) / Windows 11 |
| 架构 | x64（ARM64 需自行编译） |
| 音频 API | WASAPI Shared 模式 |
| 虚拟音频设备 | [VB-Audio Virtual Cable](https://vb-audio.com/Cable/)（免费，需单独安装） |
| 已知限制 | 不支持 macOS / Linux（依赖 Windows WASAPI） |
| 已知限制 | 部分 USB 声卡在 WASAPI Shared 模式下延迟较高 |

### 音频格式支持

| 格式 | 输入解析 | 输出写入 |
|------|----------|----------|
| 8-bit Integer | ✅ | ✅ |
| 16-bit Integer | ✅ | ✅ |
| 24-bit Integer | ✅ | ✅ |
| 32-bit Integer | ✅ | ✅ |
| 32-bit Float | ✅ | ✅ |
| 64-bit Float | ✅ | ✅ |

### 采样率处理

内部统一使用 **48kHz** 进行音频处理，输入/输出时自动重采样：

```
输入设备 (44.1k/48k/96k/192k...) → 重采样到 48kHz → 降噪/EQ/增益 → 重采样到设备采样率 → 输出设备
```

- **44.1kHz 设备**：无损处理，完全兼容
- **48kHz 设备**：无损处理，最佳兼容
- **96kHz+ 设备**：高频部分（24kHz 以上）会被滤除，但人声范围（80Hz-8kHz）完全保留，对直播/游戏场景无影响

## 安装与运行

```bash
# 安装依赖
pnpm install

# 开发模式
pnpm tauri dev

# 构建安装包
pnpm tauri build
```

## 使用方法

1. 安装 [VB-Audio Virtual Cable](https://vb-audio.com/Cable/)
2. 运行 `pnpm tauri dev`
3. 在设置中选择主题（深色/浅色）和语言
4. 选择输入设备（麦克风）和输出设备（CABLE Input）
5. 调整降噪强度，降噪自动开始
6. 在 OBS/直播软件中选择 "CABLE Output" 作为音频输入

## 项目结构

```
src/                    # Vue 3 前端
├── components/         # UI 组件
│   ├── AudioMeter      # 电平表
│   ├── BgmMixer        # BGM 混音控制
│   ├── DenoiseControl  # 降噪控制
│   ├── DeviceSelector  # 设备选择
│   ├── EqControl       # 均衡器开关按钮
│   ├── EqPage          # 均衡器界面（独立窗口，Canvas 曲线图）
│   ├── ExplodeButton   # 一键炸麦
│   ├── SettingsPage    # 设置界面（独立窗口）
│   ├── TutorialPage    # 使用教程（独立窗口）
│   ├── SpectrumVisualizer # 频谱可视化
│   └── StatusBadge     # 状态徽章
├── composables/        # 组合式函数
│   ├── useAudioEngine  # 音频引擎接口
│   ├── useAudioStats   # 统计轮询
│   └── useSettings     # 设置读写
src-tauri/src/          # Rust 后端
├── audio_engine.rs     # WASAPI 音频引擎
├── denoise/            # 降噪模型架构（trait + 适配器）
├── eq.rs               # EQ 均衡器（Biquad IIR 滤波器）
├── device_watcher.rs   # 设备热拔插检测
└── lib.rs              # Tauri 命令 + 系统托盘 + 设置管理
docs/                   # 文档
└── eq-spec.md          # 均衡器功能规格（参考 SteelSeries GG Sonar）
```

## 快捷键

- 默认 `Ctrl+Shift+D`：切换降噪开关
- 可在设置界面自定义 5 个全局快捷键（降噪/EQ/BGM/炸麦/监听）
- 支持单键（如 F1）或组合键（如 Ctrl+D）

## 技术栈

- [Tauri 2](https://tauri.app/) - 桌面应用框架
- [Vue 3](https://vuejs.org/) - 前端框架
- [Vite](https://vitejs.dev/) - 构建工具
- [WASAPI](https://docs.microsoft.com/en-us/windows/win32/coreaudio/core-audio-interfaces) - Windows 音频 API
- tauri-plugin-global-shortcut - 全局快捷键
- tauri-plugin-store - 配置持久化
- tauri-plugin-autostart - 开机自启动
- tauri-plugin-updater - 自动更新（GitHub Releases）

## 免责声明

- 本软件仅供学习和个人使用，作者不对其造成的任何后果负责
- 降噪效果因设备和环境而异，不保证在所有场景下均有理想效果
- 一键炸麦功能仅供娱乐，请勿长时间使用，以免损伤听力或设备
- VB-Audio Virtual Cable 为第三方软件，其使用遵循其自身的许可协议
- 本软件不收集任何用户数据，音频处理完全在本地完成
