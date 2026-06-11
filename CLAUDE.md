# MeowMic（喵咪麦克）

直播/游戏麦克风降噪桌面应用。Tauri 2 + Vue 3 + TypeScript + Rust。

## 技术栈

- **前端**：Vue 3 + TypeScript + Vite
- **后端**：Rust (Tauri 2)
- **音频 API**：WASAPI (Windows Audio Session API)
- **降噪**：nnnoiseless（RNNoise，可用）+ DeepFilterNet3（tract-onnx，代码已写但特征归一化未对齐训练管线，暂时隐藏）
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
  components/           # UI 组件（SettingsPage / TutorialPage 等独立窗口组件）
  composables/          # Vue 组合式函数（useTheme / useSettings / useAudioEngine 等）
  locales/              # 多语言翻译（zh-CN.ts / en.ts / index.ts）
  main.ts               # 主窗口入口
  settings-main.ts      # 设置窗口入口（必须导入 main.css）
  tutorial-main.ts      # 教程窗口入口
src-tauri/src/          # Rust 后端
  audio_engine.rs       # WASAPI 音频引擎
  denoise/              # 降噪模型（mod.rs trait + rnnoise.rs + deepfilter.rs）
  device_watcher.rs     # 设备热拔插检测（后台轮询 + Tauri 事件）
  lib.rs                # Tauri 命令注册 + 系统托盘 + 设置管理
```

## 踩坑警示

- **WASAPI API**：`WaveFormat::new()` 参数顺序是 `(storebits, validbits, &SampleType, samplerate, channels, channel_mask)`，不是 channels 在前
- **WASAPI 初始化**：用 `initialize_mta()` 不是 `initialize()`
- **设备枚举**：用 `DeviceCollection::new(&Direction)` + `get_device_at_index(i)`，没有 `.iter()` 方法
- **WASAPI Direction**：`Direction::Capture` = 录音设备（麦克风），`Direction::Render` = 播放设备（扬声器/VB-Cable）。`initialize_client` 的 direction 参数要和设备方向一致
- **Windows PATH**：pnpm 通过 npm 全局安装后，bash 环境需通过 `node.exe pnpm.mjs` 调用，或在 PowerShell 中设置 PATH
- **Tauri autostart 插件**：API 方法名是 `autolaunch()` 不是 `autostart()`（v3 会改名），`ManagerExt` trait 必须 `use` 到作用域；必须在 `capabilities/default.json` 声明 `autostart:allow-is-enabled`、`autostart:allow-enable`、`autostart:allow-disable`，否则权限不足
- **Tauri global-shortcut 插件**：没有 `init()` 函数，用 `Builder::new().build()`；权限名用连字符 `allow-is-registered` 不是下划线
- **WASAPI Process Loopback**：`new_application_loopback_client(pid, true)` 的 `get_mixformat()` 和 `get_periods()` 返回 `E_NOTIMPL`，必须用固定格式 `WaveFormat::new(32, 32, &SampleType::Float, 48000, 2, None)` + `initialize_client` period 传 0
- **Windows ToolHelp API**：枚举进程用 `CreateToolhelp32Snapshot` + `Process32FirstW/NextW`，需要 `Win32_System_Diagnostics_ToolHelp` feature
- **WASAPI 线程管理**：`stop()` 必须 `join()` 等音频线程退出再返回，否则旧流残留会产生回音。BGM 线程同理
- **Tauri 字段命名**：Rust 端 `AppSettings` 用 snake_case 字段名 + `#[serde(rename_all = "camelCase")]`，前端用 camelCase，Tauri 通过 serde 做转换
- **设备热拔插**：`wasapi` crate 不支持 `IMMNotificationClient`，用后台轮询枚举设备列表 + 哈希比对实现
- **多语言**：使用 vue-i18n，语言偏好存 localStorage，选择后即时切换（不需要点保存），主窗口和教程窗口通过 setInterval 轮询同步
- **nnnoiseless DenoiseState**：`new()` 返回 `Box<DenoiseState<'static>>`，结构体有 phantom lifetime 参数 `'a`，字段类型需用 `Box<DenoiseState<'static>>`
- **前端引擎重启竞争**：设备切换、热拔插、模型切换都会触发 stop+start，多条路径并发调用导致 "Engine is already running"。必须用统一的 debounce restart 函数 + 锁；Vite HMR 重载时前端 ref 重置但 Rust 引擎仍在跑，`handleStart` 需捕获 `already running` 并同步状态
- **deep_filter crate lib 名**：Cargo.toml 包名是 `deep_filter`，但 `[lib] name = "df"`，代码中必须 `use df::DFState`，不能 `use deep_filter::DFState`
- **Tauri 资源打包**：资源按类型分目录（`resources/models/`、`resources/vb-cable/`），`tauri.conf.json` 的 `bundle.resources` 用 `resources/models/*`、`resources/vb-cable/*` 声明；运行时通过 `app.path().resource_dir()` 获取路径
- **ONNX 模型加载阻塞**：tract 加载 ONNX 文件可能需要几秒，在音频线程上执行会阻塞 WASAPI 导致炸麦。必须在音频线程启动前预加载，或用异步加载+直通模式过渡
- **DeepFilterNet 特征归一化**：Rust 端的 ERB 特征提取和归一化必须精确匹配原始 Python 训练管线（log-scale? fixed stats? EMA tau?），否则模型输出增益全线偏低（avg ~0.2），语音被压制。需要对照 `libdf` crate 或 Python 源码逐行对齐，不能靠猜
- **VB-Audio Cable 驱动安装**：打包时必须包含完整驱动包（.inf + .sys + .cat + ARM64 .sys），缺少任一文件会导致安装静默失败；用 `ShellExecuteW` + `"runas"` 触发 UAC 提权，`Command::new()` 不会自动提权会报 os error 740；安装后 WASAPI 设备列表可能有缓存延迟，需重启应用才能检测到新设备
- **Tauri dev 资源目录**：`app.path().resource_dir()` 在 dev 模式指向 `target/debug/`，需 fallback 到 `CARGO_MANIFEST_DIR/resources/models`（模型）和 `resources/vb-cable`（驱动）
- **WASAPI 多设备输出**：监听功能需要同时向两个设备写入音频，每个 WASAPI render client 必须独立设置事件句柄（`set_get_eventhandle`）并在写入前 `wait_for_event`，否则会出现 `0x88890006`（`AUDCLNT_BUFFER_OVERFLOW`）缓冲区溢出错误，导致无声
- **WASAPI 监听格式**：监听设备不能复用主输出的 `output_format`（可能是 32-bit float），必须用固定 `WaveFormat::new(16, 16, &SampleType::Int, ...)` 初始化，因为写入代码固定按 i16 处理。格式不匹配会导致无声
- **WASAPI 共享模式默认端点**：共享模式下音频流绑定系统默认端点，拔耳机/切换默认设备会中断流。设备热拔插触发重启可恢复，但有短暂间隙
- **Tauri WebviewWindow**：构造函数 `new WebviewWindow(label, opts)` 没有 `.on()` 方法，用 `.listen()` 或 `.once()`；创建独立窗口需要在 `capabilities/default.json` 添加窗口名到 `windows` 数组并声明 `core:webview:allow-create-webview-window` 权限；窗口关闭用 `hide()` 代替 `close()` 避免 label 无法释放导致再次创建失败
- **Tauri 图标更新不生效**：替换 `icons/` 目录下的图标文件后，`pnpm tauri dev` 可能仍显示旧图标。需要清除 cargo 编译缓存（`cargo clean` 或删除 `src-tauri/target/`）再重新编译，图标才会更新。单纯重启 dev 服务不够
- **NSIS 安装器图标缓存**：Windows 对 exe 文件名缓存图标，替换 icon.ico 后重新打包，安装包图标可能仍显示旧图标。解决：改版本号（`tauri.conf.json` 的 `version`）让输出文件名变化，或手动清除 `%LocalAppData%\Microsoft\Windows\Explorer\iconcache*` 并重启资源管理器
- **多窗口 CSS 变量**：Tauri 每个窗口是独立 webview，各有自己的 JS 上下文。每个窗口的入口文件（`settings-main.ts`、`tutorial-main.ts`）必须导入 `main.css`，否则 CSS 变量不生效；需要调用 `useTheme()` 才会读取/应用主题
- **多窗口状态同步**：`localStorage` 的 `storage` 事件不会在同一文档中触发，只能跨窗口。同一窗口内的状态变更通过 Vue 响应式 ref 共享；跨窗口通过 Tauri 事件（`emit`/`listen`）或 `setInterval` 轮询 localStorage
- **WASAPI 监听停止**：关闭监听时仅跳过写入不够，WASAPI 缓冲区残余音频会继续播放。必须调用 `stop_stream()` 立即停止，并用 `monitor_was_streaming` 标记避免每帧重复 stop/start
- **BGM 混音开关**：无进程时应允许打开开关（仅不启动混音），选中进程后自动开始。否则用户会误以为功能损坏
- **WASAPI 同设备冲突**：同一 USB 设备的输入输出端点（如 K7 麦克风 + K7 耳机）同时打开会导致 `read_from_device` 持续返回 0 帧。原因：共享模式下同一物理设备的 Capture/Render 端点共享时钟，缓冲区竞争死锁。解决：检测连续 100 次 0 帧读取后返回 `AUDIO_DEVICE_CONFLICT` 错误，前端自动切换输出设备
- **WASAPI 首次启动预热**：打包后首次启动，WASAPI 设备可能需要几帧才能进入稳定状态，前几帧可能是空数据。解决：启动后预热最多 3 次（每次 300ms），检测到非零信号才进入主循环；预热失败则重启流重试
- **打包调试日志**：`env_logger::init()` 在打包后无输出。用 `debug_log()` 写入 `%TEMP%\meowmic-debug.log`，格式 `[elapsed] message`

## 红线

- 密钥、token、密码不进代码
- 不注释报错来绕过问题
- 大改动前出方案确认
