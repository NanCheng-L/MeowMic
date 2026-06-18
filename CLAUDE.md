# MeowMic（喵咪麦克）

直播/游戏麦克风降噪桌面应用。Tauri 2 + Vue 3 + TypeScript + Rust。

## 技术栈

- **前端**：Vue 3 + TypeScript + Vite
- **后端**：Rust (Tauri 2)
- **音频 API**：WASAPI (Windows Audio Session API)
- **降噪**：nnnoiseless（RNNoise）+ MeowMic（自训练 ONNX 模型，Rust 原生推理）

## 降噪模型架构

- **RNNoise**：原生 Rust 实现（nnnoiseless），<1ms，擅长持续噪声（风扇、空调）
- **MeowMic**：自训练模型（ONNX），Rust 原生推理（ort crate），擅长瞬态噪声（键盘、鼠标）
- 模型选择：RNNoise / MeowMic（本地 ONNX）
- **虚拟音频设备**：VB-Audio Virtual Cable
- **全局快捷键**：tauri-plugin-global-shortcut
- **配置持久化**：tauri-plugin-store
- **开机自启动**：tauri-plugin-autostart
- **自动更新**：tauri-plugin-updater（GitHub Releases）

## 开发命令

```bash
pnpm tauri dev          # 启动开发模式
pnpm tauri build        # 构建安装包
cargo check             # 检查 Rust 编译（在 src-tauri/ 下）
```

## 项目结构

```
src/                    # Vue 前端
  components/           # UI 组件（SettingsPage / TutorialPage / EqPage 等独立窗口组件）
  composables/          # Vue 组合式函数（useTheme / useSettings / useAudioEngine 等）
  locales/              # 多语言翻译（zh-CN.ts / en.ts / index.ts）
  main.ts               # 主窗口入口
  settings-main.ts      # 设置窗口入口（必须导入 main.css）
  tutorial-main.ts      # 教程窗口入口
  eq-main.ts            # 均衡器窗口入口
src-tauri/src/          # Rust 后端
  audio_engine.rs       # WASAPI 音频引擎
  denoise/              # 降噪模型（mod.rs trait + rnnoise.rs）
  eq.rs                 # EQ 均衡器（Biquad IIR 滤波器 + 10 段 Peaking EQ）
  device_watcher.rs     # 设备热拔插检测（后台轮询 + Tauri 事件）
  lib.rs                # Tauri 命令注册 + 系统托盘 + 设置管理
docs/                   # 文档
  eq-spec.md            # 均衡器功能规格（参考 SteelSeries GG Sonar）
scripts/                # 构建/发布辅助脚本
  generate-update-json.cjs # 生成更新所需的 latest.json（输出到安装包同目录，自动读取 .sig）
  set-signing-env.ps1      # 设置签名环境变量（构建前运行）
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
- **Tauri 字段命名**：Rust 端 `AppSettings` 用 snake\_case 字段名 + `#[serde(rename_all = "camelCase")]`，前端用 camelCase，Tauri 通过 serde 做转换
- **设备热拔插**：`wasapi` crate 不支持 `IMMNotificationClient`，用后台轮询枚举设备列表 + 哈希比对实现
- **多语言**：使用 vue-i18n，语言偏好存 localStorage `meowmic-lang`，选择后即时切换（不需要点保存）。每个独立窗口（主窗口/教程/设置/均衡器）必须：① onMounted 时读取 localStorage 调用 setLocale()；② setInterval 轮询同步；③ storage 事件监听跨窗口同步。详细规范见 `docs/eq-spec.md` §6.4
- **nnnoiseless DenoiseState**：`new()` 返回 `Box<DenoiseState<'static>>`，结构体有 phantom lifetime 参数 `'a`，字段类型需用 `Box<DenoiseState<'static>>`
- **前端引擎重启竞争**：设备切换、热拔插、模型切换都会触发 stop+start，多条路径并发调用导致 "Engine is already running"。必须用统一的 debounce restart 函数 + 锁；Vite HMR 重载时前端 ref 重置但 Rust 引擎仍在跑，`handleStart` 需捕获 `already running` 并同步状态
- **Tauri 资源打包**：资源按类型分目录（`resources/models/`、`resources/vb-cable/`），`tauri.conf.json` 的 `bundle.resources` 用 `resources/models/*`、`resources/vb-cable/*` 声明；运行时通过 `app.path().resource_dir()` 获取路径
- **ONNX 模型加载阻塞**：tract 加载 ONNX 文件可能需要几秒，在音频线程上执行会阻塞 WASAPI 导致炸麦。必须在音频线程启动前预加载，或用异步加载+直通模式过渡
- **VB-Audio Cable 驱动安装**：打包时必须包含完整驱动包（.inf + .sys + .cat + ARM64 .sys），缺少任一文件会导致安装静默失败；用 PowerShell `Start-Process -Verb RunAs` 触发 UAC 提权，配合 `CREATE_NO_WINDOW` 标志隐藏控制台窗口；本地找不到安装包时自动从官网下载兜底；安装后 WASAPI 设备列表可能有缓存延迟，需重启应用才能检测到新设备
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
- **WASAPI 设备断开回音**：设备断开时输入流失败但输出流继续播放残余数据导致回音。解决：连续 10 次读取失败后主动调用 `output_client.stop_stream()` 停止输出，避免残余音频播放
- **WASAPI 首次启动预热**：打包后首次启动，WASAPI 设备可能需要几帧才能进入稳定状态，前几帧可能是空数据。解决：启动后预热最多 3 次（每次 300ms），检测到非零信号才进入主循环；预热失败则重启流重试
- **打包调试日志**：`env_logger::init()` 在打包后无输出。用 `debug_log()` 写入 `%TEMP%\meowmic-debug.log`，格式 `[elapsed] message`
- **增益控制位置**：`mic_gain` 必须在降噪**之后**应用（`audio_loop` 中 denoise 输出 → strength mixing → mic_gain），放在降噪前会放大噪音导致降噪效果变差
- **EQ 均衡器位置**：EQ 在 `audio_loop` 中位于 mic_gain **之后**、爆炸模式**之前**（mic_gain → EQ → explode），EQ 调整的是增益后的音色
- **update_denoise_config 竞争**：每次调用都创建 `DenoiseConfig::default()` 会重置 strength 为 0.5。必须先 `get_config()` 读当前值再只更新传入的字段
- **Windows 版本信息读取**：`windows` crate 0.58 没有 `Win32_System_Diagnostics_Process` feature，`GetFileVersionInfoW`/`VerQueryValueW` 需用 raw FFI（`extern "system"` 声明）
- **进程名获取不可靠**：FileDescription 对国产软件（网易云、抖音、QQ浏览器）通常返回英文或截断值，必须用 exe 名映射表兜底；窗口标题包含动态内容（歌名、场景名），需清理 " - " 后缀和版本号
- **BGM 多选进程**：每个 PID 启动独立 WASAPI loopback 线程，通过同一 channel 发送混音数据，用 manager 线程 join 所有子线程
- **设备热拔插恢复**：`lastUserInput` 只在用户手动选设备和启动加载时更新，`devices-changed` 处理器绝不能覆盖；Vue watch 异步执行，不能用同步标志位区分用户/系统变更
- **EQ loadEqConfig 加载顺序**：后端 `EqConfig` 在 engine 重启后 bands 恢复为全 0（default）。`loadEqConfig` 必须优先从 `localStorage('meowmic-eq-preset')` 读取预设名，用 `presets.find()` 获取正确的 bands 值，不能直接用后端返回的 bands——否则预设名正确但曲线平坦。同时 `loadPresets()` 必须在 `loadEqConfig()` 之前完成（不能用 `Promise.all`），否则 presets 数组为空
- **EQ 跨窗口状态同步**：`loadEqConfig` 从 `localStorage('meowmic-config')` 读取 `eqEnabled` 并同步到后端，而非从后端读取（后端不持久化）。同时在 EqPage.vue 中监听 `eq-changed` Tauri 事件实时更新 toggle 状态
- **WASAPI IAudioSessionManager2**：枚举音频会话需要 `Win32_System_Com` + `Win32_Media_Audio` feature；活跃会话始终显示，非活跃会话仅保留映射表中的已知播放器（避免系统进程如 audiodg 出现）；同名进程按名称去重
- **WASAPI 输出编码格式**：输出编码必须用**输出设备**的 format（`output_bits` / `output_sample_type`），不能用输入设备的。VB-Audio Cable 通常是 32-bit float，麦克风通常是 16-bit int，混用会导致字节错位产生破音
- **NaN/Inf 穿透音频链路**：RNNoise 模型偶尔输出 NaN/Inf，会穿透 soft limiter（`NaN > 28000.0` 为 false 不压缩）直达输出。必须在 denoise 输出后、soft limiter 内、爆炸模式内逐样本检查 `is_finite()`
- **Soft limiter 阈值与压缩比**：阈值不能太高（28000 太接近 0dBFS），压缩比不能太温和（0.2 即 5:1 仍可能输出 30000+）。推荐阈值 24000、压缩比 0.1（10:1）、硬上限 30000
- **EQ 弹窗与 canvas 事件冲突**：弹窗 `position: fixed` 覆盖在 canvas 上方时，canvas 会触发 `mouseleave` 导致弹窗消失。解决：canvas 的 `mouseleave` 不隐藏弹窗，改用弹窗自身的 `@mouseleave` 处理隐藏
- **Realtek 内置声卡同名设备不是同一设备**："麦克风 (Realtek(R) Audio)" 和 "扬声器 (Realtek(R) Audio)" 共享型号名但物理端点不同，不会导致 WASAPI 死锁。same-device 检测只针对 USB 设备（正则 `/usb|cable/i`）
- **Azure Blob Storage 不支持断点续传**：`curl -C -` 失败，下载必须一次性完成或重新开始。DNS Challenge 数据从 `dns4public.blob.core.windows.net` 下载无 resume 支持
- **输出到扬声器导致电流麦（回声反馈）**：降噪音频 → 扬声器播放 → 麦克风拾取 → 再次降噪 → 循环产生嗡嗡声。必须使用 VB-Cable 虚拟声卡作为输出设备
- **WASAPI 监听同设备检测**：监听使用系统默认输出设备（`find_device(None, false)`），可能与输入是同一 USB 物理设备（如 K7 麦克风 + K7 耳机）。通过提取设备 ID 中的 USB VID/PID 比较，相同则跳过监听初始化，避免共享 USB 时钟导致的电流麦干扰
- **监听点启动同步**：`monitor_point` 后端默认为 0（关闭），前端 localStorage 保存的值需在引擎启动后调用 `setMonitor_point` 同步，否则监听不生效。需在 `handleStart` 和 HMR 热重载恢复路径中都同步
- **监听点必须在处理阶段之前**：监听点写入必须放在对应处理阶段**之前**（如点 2 在增益前、点 3 在增益后），否则所有点读到的是同一个变量（已被后续阶段覆盖）。常见错误：把所有监听点放在处理链路末尾
- **设置窗口模型列表兜底**：设置窗口首次打开时，`settings-init` 事件可能因窗口未加载完而丢失。需在 `onMounted` 中直接调用 `list_denoise_models` 兜底加载
- **WASAPI 监听设备自动跟随**：监听使用系统默认输出设备，但 WASAPI 客户端在创建时绑定设备，不会自动跟随系统默认设备变更。需要每秒检查 `find_device(None, false)` 的 ID 是否变化，变化时停止旧客户端、重新初始化新客户端。闭包捕获的变量（`monitor_format`、`input_device_id`）必须在闭包定义前提取为独立变量
- **系统声音输出不能选 VB-Cable**：安装 VB-Cable 后系统默认输出会变为 CABLE Input，用户需手动改回耳机/扬声器，否则听不到系统声音。教程页面需明确说明
- **WASAPI 输出缓冲区溢出**：`output_buffer` 按 `frame_size * output_bytes_per_frame` 分配，但重采样后 `out_frames` 可能膨胀（如输出设备 96kHz 时翻倍）。必须按 `frame_size * (output_sample_rate / 48000).ceil()` 分配最大可能的缓冲区大小
- **WASAPI 输出 padding 跳帧**：当缓冲区 > 20ms 时跳过写入会导致可听到的卡顿。应始终写入，让 WASAPI 处理背压。跳过帧 = 音频间隙 = 卡麦
- **EQ 后缺少 NaN/Inf 检查**：Biquad IIR 滤波器在极端参数下可能输出 NaN/Inf，穿透 BGM 混音污染输出。必须在 EQ 处理后添加 `is_finite()` 检查

## 版本号管理

版本号需同时更新三处：

- `package.json` 的 `version`
- `src-tauri/Cargo.toml` 的 `version`
- `src-tauri/tauri.conf.json` 的 `version`（Tauri 打包使用的实际版本）

## 自动更新

使用 Tauri updater 插件（`tauri-plugin-updater`），通过 GitHub Releases 分发更新。

### 发布流程（手动）

1. 更新三处版本号
2. 在 PowerShell 中设置签名环境变量：`. .\scripts\set-signing-env.ps1`
3. `pnpm tauri build` 构建（会自动签名，生成安装包 + `.sig` 签名文件）
4. 运行 `node scripts/generate-update-json.cjs <版本号> <安装包路径>` 生成 `latest.json`（自动读取 `.sig`，输出到安装包同目录）
5. 在 GitHub 创建 Release `v<版本号>`，上传安装包 + `latest.json`

### 密钥生成

首次配置需生成 minisign 密钥对：

```bash
pnpm tauri signer generate -w tauri.key
```

- 公钥填入 `tauri.conf.json` 的 `plugins.updater.pubkey`
- 私钥文件 `tauri.key` 不要提交到 git

## 红线

- 密钥、token、密码不进代码
- 不注释报错来绕过问题
- 大改动前出方案确认

