# pico-denoise

直播/游戏麦克风降噪桌面应用，基于 WASAPI 实时音频处理。

## 功能

- 实时麦克风降噪（WASAPI Shared 模式）
- 多降噪模型可选（RNNoise / DeepFilterNet3，可扩展）
  - RNNoise：轻量级，擅长去除风扇、空调等持续噪音，CPU 占用极低
  - DeepFilterNet3：深度学习模型，擅长去除键盘、鼠标等瞬态噪音，语音保真度更高
- 预设模式（安静/标准/嘈杂/直播 + 自定义）
- 输入/输出设备选择 + 设备热拔插自动检测
- 降噪强度可调（0-100%）
- 电平表 + 频谱可视化
- 系统托盘（最小化到托盘）
- 全局快捷键（可自定义，后台/最小化均可触发）
- 开机自启动
- BGM 混音（按进程捕获音乐播放器音频，混合到麦克风输出）
- 一键炸麦（恶搞模式，50x 增益 + 方波失真，10 秒自动关闭）
- 设置界面（快捷键配置、开机自启开关、语言切换）
- VB-Audio Virtual Cable 虚拟设备引导
- 多语言支持（中文 / English）

## 环境要求

- Windows 10/11
- Node.js 18+
- pnpm
- Rust + Cargo
- [VB-Audio Virtual Cable](https://vb-audio.com/Cable/)（输出端需要）

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
3. 选择输入设备（麦克风）和输出设备（CABLE Input）
4. 调整降噪强度
5. 点击"开始降噪"
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
│   ├── SettingsDialog  # 设置界面
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
