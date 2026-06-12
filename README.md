# 喵咪麦克（MeowMic）

> 🐱🎤 实时麦克风降噪桌面应用，为直播和游戏场景打造。

基于 WASAPI 实时音频处理，支持 RNNoise 降噪模型，开箱即用。
<img width="526" height="890" alt="d63096ca-beae-4286-ab34-781599ef8131" src="https://github.com/user-attachments/assets/01a89f2d-ad96-4e58-8675-bcf860c727de" />
<img width="526" height="890" alt="41e71233-b375-4440-ae58-29a64a9b0c6d" src="https://github.com/user-attachments/assets/4a282f7c-2960-4c34-beac-a7435b7262b8" />


## 功能

- 实时麦克风降噪（WASAPI Shared 模式）
- 多降噪模型架构（可扩展）
  - RNNoise：轻量级，擅长去除风扇、空调等持续噪音，CPU 占用极低
- 预设模式（安静/标准/嘈杂/直播 + 自定义）
- 输入/输出设备选择 + 设备热拔插自动检测
- 降噪强度可调（0-100%）
- 电平表 + 频谱可视化
- 系统托盘（最小化到托盘）
- 全局快捷键（可自定义，后台/最小化均可触发）
- 开机自启动
- BGM 混音（按进程捕获音乐播放器音频，混合到麦克风输出）
- 一键炸麦（恶搞模式，增益 + 方波失真，强度可调 1-100%）
- 监听模式（开关式，自动使用系统默认输出设备，实时听到降噪效果）
- 设置界面（独立窗口，快捷键配置、开机自启开关、语言切换）
- 使用教程（独立窗口，设备说明 + FAQ + 社交链接）
- VB-Audio Virtual Cable 虚拟设备引导
- 多语言支持（中文 / English）
- 主题切换（深色 / 浅色，默认浅色暖色调）
- 自动更新（检测新版本 → 下载 → 静默安装）

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
├── device_watcher.rs   # 设备热拔插检测
└── lib.rs              # Tauri 命令 + 系统托盘 + 设置管理
```

## 快捷键

- 默认 `Ctrl+Shift+D`：切换降噪开关
- 可在设置界面自定义全局快捷键

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
