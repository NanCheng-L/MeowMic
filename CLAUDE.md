# MeowMic（喵咪麦克）

直播/游戏麦克风降噪桌面应用。Tauri 2 + Vue 3 + TypeScript + Rust。

## 技术栈

- **前端**：Vue 3 + TypeScript + Vite
- **后端**：Rust (Tauri 2)
- **音频 API**：WASAPI (Windows Audio Session API)
- **降噪**：nnnoiseless（RNNoise）+ DeepFilterNet3（FFI 调用预编译 DLL）

## 降噪模型架构

- **RNNoise**：原生 Rust 实现（nnnoiseless），<1ms，擅长持续噪声（风扇、空调）
- **DeepFilterNet3**：FFI 调用 `deepfilter_runtime_bridge.dll`，擅长瞬态噪声（键盘、鼠标），降噪能力更强
- 模型选择：RNNoise / DeepFilterNet3

## 音频线程架构（三线程解耦）

音频引擎使用三线程架构（参考 noisegate-ref），解决游戏等高 CPU 负载场景下的颤音问题：

```
Capture 线程 (wasapi_capture.rs, windows crate)
  │ WASAPI event → GetBuffer → downmix/resample → 推入 Ring A
  ▼
DSP 线程 (audio_engine.rs::dsp_thread)
  │ Ring A → denoise → strength → gain → EQ → explode → BGM mix → limiter → 推入 Ring B + Ring M
  ▼
Render 线程 (wasapi_render.rs, windows crate)
  │ Ring B → GetBuffer → upmix/resample → ReleaseBuffer → VB-Cable
  ▼
Monitor Render 线程 (wasapi_render.rs, 同 render 代码路径)
  │ Ring M → 耳机（监听，仅在用户开启时推入数据）
```

- **启动顺序**：DSP → capture → render（和 noisegate-ref 一致）
- **模型预加载**：模型在主线程加载完再启动 DSP 线程，避免启动时原始音频直通产生回声
- **监听**：用独立 WasapiRender 线程（和主 render 完全一样的代码路径），不用 wasapi crate
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
  audio_engine.rs       # WASAPI 音频引擎（三线程架构 + DSP 处理 + 配置结构体）
  audio_init.rs         # 音频设备初始化（输入/输出/监听 WASAPI 客户端配置）
  audio_utils.rs        # 音频工具函数（格式转换、重采样、监听写入）
  bgm.rs                # BGM 进程捕获（WASAPI Loopback）
  debug.rs              # 调试日志（%TEMP%\meowmic\debug.log）
  device.rs             # 设备查找
  denoise/              # 降噪模型（mod.rs trait + rnnoise.rs + deepfilter.rs FFI）
  dsp/                  # DSP 模块（统一 DspModule trait）
    mod.rs              # DspModule trait 定义（name/process/reset）
    hpf.rs              # 高通滤波器（80Hz，切除次声波）
    limiter.rs          # 软限幅器（阈值0.73, 10:1, 硬限0.92）
    noise_gate.rs       # 噪声门（平滑 attack/release，基于信号电平）
    vad.rs              # 语音活动检测（噪声底追踪 + 自适应阈值 + 连续帧计数）
  agc.rs                # AGC 自动增益控制（使用独立 VadState + NoiseGate 模块）
  eq.rs                 # EQ 均衡器（Biquad IIR 滤波器 + 10 段 Peaking EQ）
  explode.rs            # 爆炸模式（方波失真/电流声/白噪音/机器人声/恶魔声）
  device_watcher.rs     # 设备热拔插检测（后台轮询 + Tauri 事件）
  lib.rs                # Tauri 命令注册 + 系统托盘 + 设置管理
docs/                   # 文档
  eq-spec.md            # 均衡器功能规格（参考 SteelSeries GG Sonar）
scripts/                # 构建/发布辅助脚本
  generate-update-json.cjs # 生成更新所需的 latest.json（输出到安装包同目录，自动读取 .sig）
  set-signing-env.ps1      # 设置签名环境变量（构建前运行）
  tavily.cjs               # Tavily API 搜索/提取/爬虫工具（key 在 .tavily-key，已 gitignore）
```

## 踩坑警示

- **WASAPI API**：`WaveFormat::new()` 参数顺序是 `(storebits, validbits, &SampleType, samplerate, channels, channel_mask)`，不是 channels 在前
- **WASAPI 初始化**：用 `initialize_mta()` 不是 `initialize()`
- **设备枚举**：用 `DeviceEnumerator::new()?.get_device_collection(&Direction)` + `get_device_at_index(i)` 或 `into_iter()`。`DeviceCollection` 不实现 `Send`，不能跨线程传递
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
- **WASAPI 监听格式**：监听设备必须用**自己的 mixformat** 初始化（包括采样率、位深、声道数），不能复用输出设备的格式或固定用 16-bit int。代码中 `init_monitor_client` 需调用 `m_client.get_mixformat()` 获取完整格式，`write_to_monitor` 需根据实际位深写入对应格式（i16 或 f32）。格式不匹配会导致 WASAPI 内部重采样异常，长时间运行后缓冲区积累异常数据产生电流声
- **WASAPI 音频格式解析**：`bytes_to_f32_samples` 必须支持所有常见格式：8bit int、16bit int、24bit int、32bit int、32bit float、64bit float。K7 USB 麦克风是 24bit，缺少支持会导致字节错位产生电流声。未知格式 fallback 时必须打印警告日志
- **WASAPI 共享模式默认端点**：共享模式下音频流绑定系统默认端点，拔耳机/切换默认设备会中断流。设备热拔插触发重启可恢复，但有短暂间隙
- **Tauri WebviewWindow**：构造函数 `new WebviewWindow(label, opts)` 没有 `.on()` 方法，用 `.listen()` 或 `.once()`；创建独立窗口需要在 `capabilities/default.json` 添加窗口名到 `windows` 数组并声明 `core:webview:allow-create-webview-window` 权限；窗口关闭用 `hide()` 代替 `close()` 避免 label 无法释放导致再次创建失败
- **Tauri 图标更新不生效**：替换 `icons/` 目录下的图标文件后，`pnpm tauri dev` 可能仍显示旧图标。需要清除 cargo 编译缓存（`cargo clean` 或删除 `src-tauri/target/`）再重新编译，图标才会更新。单纯重启 dev 服务不够
- **NSIS 安装器图标缓存**：Windows 对 exe 文件名缓存图标，替换 icon.ico 后重新打包，安装包图标可能仍显示旧图标。解决：改版本号（`tauri.conf.json` 的 `version`）让输出文件名变化，或手动清除 `%LocalAppData%\Microsoft\Windows\Explorer\iconcache*` 并重启资源管理器
- **多窗口 CSS 变量**：Tauri 每个窗口是独立 webview，各有自己的 JS 上下文。每个窗口的入口文件（`settings-main.ts`、`tutorial-main.ts`）必须导入 `main.css`，否则 CSS 变量不生效；需要调用 `useTheme()` 才会读取/应用主题
- **多窗口状态同步**：`localStorage` 的 `storage` 事件不会在同一文档中触发，只能跨窗口。同一窗口内的状态变更通过 Vue 响应式 ref 共享；跨窗口通过 Tauri 事件（`emit`/`listen`）或 `setInterval` 轮询 localStorage
- **WASAPI 监听停止**：关闭监听时仅跳过写入不够，WASAPI 缓冲区残余音频会继续播放。必须调用 `stop_stream()` 立即停止，并用 `monitor_was_streaming` 标记避免每帧重复 stop/start
- **BGM 混音开关**：无进程时应允许打开开关（仅不启动混音），选中进程后自动开始。否则用户会误以为功能损坏
- **WASAPI 同设备冲突**：同一 USB 设备的输入输出端点（如 K7 麦克风 + K7 耳机）同时打开会导致 `read_from_device` 持续返回 0 帧。原因：共享模式下同一物理设备的 Capture/Render 端点共享时钟，缓冲区竞争死锁。解决：检测连续 100 次 0 帧读取后返回 `AUDIO_DEVICE_CONFLICT` 错误，前端自动切换输出设备
- **WASAPI 设备断开回音**：设备断开时输入流失败但输出流继续播放残余数据导致回音。解决：连续 10 次读取失败后 break 退出循环，cleanup 代码关闭 output channel 通知输出线程退出
- **WASAPI 跨线程传递**：`wasapi` crate 的 COM 对象（`AudioClient`、`AudioRenderClient`）和 `Handle` 不实现 `Send`（含 `*mut c_void`）。在 COM MTA 模式下跨线程安全，需用 newtype wrapper + `unsafe impl Send` 封装（如 `OutputResources`）。noisegate 项目用 `windows` crate（raw COM）而非 `wasapi` crate 来实现三线程架构（capture/DSP/render），因为 `windows` crate 的 COM 对象更容易跨线程传递
- **WASAPI 首次启动预热**：打包后首次启动，WASAPI 设备可能需要几帧才能进入稳定状态，前几帧可能是空数据。解决：启动后预热最多 3 次（每次 300ms），检测到非零信号才进入主循环；预热失败则重启流重试
- **打包调试日志**：`env_logger::init()` 在打包后无输出。用 `debug_log()` 写入 `%TEMP%\meowmic\debug.log`，格式 `[elapsed] message`
- **增益控制位置**：`mic_gain` 必须在降噪**之后**应用（`audio_loop` 中 denoise 输出 → strength mixing → mic_gain），放在降噪前会放大噪音导致降噪效果变差
- **AGC 模式切换重置**：从手动切到 AGC 时必须重置 `AgcState`（`agc.reset()`），否则旧的 smoothed_rms 和 gain 状态会导致增益突变产生杂音
- **AGC VAD 门控**：AGC 使用独立 `VadState` 模块进行语音活动检测，基于噪声底追踪的自适应阈值（`VAD_ABOVE_NOISE=6.0`，`VAD_MIN_THRESHOLD=0.002`）。只在连续7帧检测到语音后才允许增益提升（防止噪声尖峰触发）。安静时增益冻结 + 噪声门关闭
- **EQ 均衡器位置**：EQ 在 `audio_loop` 中位于增益（手动 mic_gain 或自动 AGC）**之后**、爆炸模式**之前**（gain → EQ → explode），EQ 调整的是增益后的音色
- **update_denoise_config 竞争**：每次调用都创建 `DenoiseConfig::default()` 会重置 strength 为 0.5。必须先 `get_config()` 读当前值再只更新传入的字段
- **Windows 版本信息读取**：`windows` crate 0.58 没有 `Win32_System_Diagnostics_Process` feature，`GetFileVersionInfoW`/`VerQueryValueW` 需用 raw FFI（`extern "system"` 声明）
- **进程名获取不可靠**：FileDescription 对国产软件（网易云、抖音、QQ浏览器）通常返回英文或截断值，必须用 exe 名映射表兜底；窗口标题包含动态内容（歌名、场景名），需清理 " - " 后缀和版本号
- **BGM 单进程限制**：WASAPI 共享模式下多个 `new_application_loopback_client` 同时运行会互相干扰（一个拿到静音），BGM 只能选择单个进程。每个 PID 启动独立 WASAPI loopback 线程，通过同一 channel 发送混音数据，用 manager 线程 join 所有子线程
- **BGM start_bgm 不等待旧线程**：`start_bgm` 不能调 `stop_bgm().join()` 等旧线程退出，否则阻塞 Tauri 命令线程导致 UI 卡死。只 `bgm_running.store(false)` 标记停止，旧线程自行退出。`stop_bgm()` 的 join 逻辑仅在显式停止时使用
- **WASAPI start/stop 并发安全**：`start()` 和 `stop()` 必须用 `lifecycle_lock`（`std::sync::Mutex`）互斥保护，防止设备热拔插 + 用户操作并发调用导致双音频线程竞争 WASAPI 设备。`start()` 内部调 `stop_inner()`（不加锁版本），避免死锁
- **AtomicBool ordering**：`running` 标志在 `start()`/`stop()` 中必须用 `Ordering::Release` 存储，`audio_loop` 中用 `Ordering::Acquire` 加载。`Relaxed` 在 ARM64 上不保证及时可见性，可能导致停止信号延迟生效
- **WASAPI 监听设备变更提前退出**：`audio_loop` 检测到监听设备变更后 `return Ok(())` 前，必须显式调用 `input_client.stop_stream()` / `output_client.stop_stream()`，否则 WASAPI 缓冲区残余音频会继续播放
- **设备热拔插恢复**：`lastUserInput` 只在用户手动选设备和启动加载时更新，`devices-changed` 处理器绝不能覆盖；Vue watch 异步执行，不能用同步标志位区分用户/系统变更
- **EQ loadEqConfig 加载顺序**：后端 `EqConfig` 在 engine 重启后 bands 恢复为全 0（default）。`loadEqConfig` 必须优先从 `localStorage('meowmic-eq-preset')` 读取预设名，用 `presets.find()` 获取正确的 bands 值，不能直接用后端返回的 bands——否则预设名正确但曲线平坦。同时 `loadPresets()` 必须在 `loadEqConfig()` 之前完成（不能用 `Promise.all`），否则 presets 数组为空
- **EQ 跨窗口状态同步**：`loadEqConfig` 从 `localStorage('meowmic-config')` 读取 `eqEnabled` 并同步到后端，而非从后端读取（后端不持久化）。同时在 EqPage.vue 中监听 `eq-changed` Tauri 事件实时更新 toggle 状态
- **EQ frequencies 拖拽恢复**：EqPage.vue 的 `frequencies` 数组（10 段频率位置）支持水平拖拽修改，但 `bands`（增益值）按索引对应频率。切预设时必须重置 `frequencies` 为默认值 `[20, 60, 120, 250, 500, 1000, 2000, 4000, 8000, 16000]`，否则预设的 bands 增益会画在错误的频率位置上
- **WASAPI IAudioSessionManager2**：枚举音频会话需要 `Win32_System_Com` + `Win32_Media_Audio` feature；活跃会话始终显示，非活跃会话仅保留映射表中的已知播放器（避免系统进程如 audiodg 出现）；同名进程按名称去重
- **WASAPI 输出编码格式**：输出编码必须用**输出设备**的 format（`output_bits` / `output_sample_type`），不能用输入设备的。VB-Audio Cable 通常是 32-bit float，麦克风通常是 16-bit int，混用会导致字节错位产生破音
- **WASAPI render 设备查找**：`GetDevice(PCWSTR)` 需要设备 ID（`{0.0.0.00000000}.{guid}`），但前端传的是友好名称（如 "CABLE Input (VB-Audio Virtual Cable)"）。直接调用会失败回退到默认设备。解决：失败时枚举 `EnumAudioEndpoints` 按友好名称匹配，需要 `Win32_UI_Shell_PropertiesSystem` feature 访问 `PKEY_Device_FriendlyName`
- **WASAPI capture 设备查找同理**：`wasapi_capture.rs` 的 `find_device` 也用 `GetDevice(PCWSTR)` 查找设备，传入友好名称必然失败后回退到默认通讯设备（`eCommunications`），可能返回静音流。解决：和 render 端一样，失败时枚举设备按友好名称匹配
- **监听缓冲区溢出**：`write_to_monitor` 每次写固定帧数不检查可用空间，导致 `0x88890006`（`AUDCLNT_BUFFER_OVERFLOW`）。解决：写入前调 `client.get_available_space_in_frames()` 获取实际可写帧数，只写能写的量，空间不足时跳过
- **配置状态日志**：所有开关状态变更（降噪/EQ/AGC/爆炸/BGM/监听）必须用 `debug_log()` 写入 `%TEMP%\meowmic\debug.log`，方便排查用户问题。关键日志：`CONFIG_UPDATE`、`EQ_UPDATE`、`EXPLODE_MODE`、`BGM_START/STOP`、`set_monitor_mode/point`
- **NaN/Inf 穿透音频链路**：RNNoise 模型偶尔输出 NaN/Inf，会穿透 soft limiter（`NaN > 28000.0` 为 false 不压缩）直达输出。必须在 denoise 输出后、soft limiter 内、爆炸模式内逐样本检查 `is_finite()`
- **Soft limiter 阈值与压缩比**：阈值不能太高（28000 太接近 0dBFS），压缩比不能太温和（0.2 即 5:1 仍可能输出 30000+）。推荐阈值 24000、压缩比 0.1（10:1）、硬上限 30000
- **EQ 弹窗与 canvas 事件冲突**：弹窗 `position: fixed` 覆盖在 canvas 上方时，canvas 会触发 `mouseleave` 导致弹窗消失。解决：canvas 的 `mouseleave` 不隐藏弹窗，改用弹窗自身的 `@mouseleave` 处理隐藏
- **Realtek 内置声卡同名设备不是同一设备**："麦克风 (Realtek(R) Audio)" 和 "扬声器 (Realtek(R) Audio)" 共享型号名但物理端点不同，不会导致 WASAPI 死锁。same-device 检测只针对 USB 设备（正则 `/usb|cable/i`）
- **Azure Blob Storage 不支持断点续传**：`curl -C -` 失败，下载必须一次性完成或重新开始。DNS Challenge 数据从 `dns4public.blob.core.windows.net` 下载无 resume 支持
- **输出到扬声器导致电流麦（回声反馈）**：降噪音频 → 扬声器播放 → 麦克风拾取 → 再次降噪 → 循环产生嗡嗡声。必须使用 VB-Cable 虚拟声卡作为输出设备。监听设备变更时会自动触发完整引擎重启（emit `restart-needed` 事件 → 前端 `scheduleRestart`），确保 RNNoise 模型和 EQ 状态被重置
- **WASAPI 监听同设备检测**：监听使用 `find_monitor_device()` 枚举非 VB-Cable 的输出设备，可能与输入是同一 USB 物理设备（如 K7 麦克风 + K7 耳机）。通过提取设备 ID 中的 USB VID/PID 比较，相同则跳过监听初始化，避免共享 USB 时钟导致的电流麦干扰
- **监听点启动同步**：`monitor_point` 后端默认为 0（关闭），前端 localStorage 保存的值需在引擎启动后调用 `setMonitor_point` 同步，否则监听不生效。需在 `handleStart` 和 HMR 热重载恢复路径中都同步
- **监听点必须在处理阶段之前**：监听点写入必须放在对应处理阶段**之前**（如点 2 在增益前、点 3 在增益后），否则所有点读到的是同一个变量（已被后续阶段覆盖）。常见错误：把所有监听点放在处理链路末尾
- **设置窗口模型列表兜底**：设置窗口首次打开时，`settings-init` 事件可能因窗口未加载完而丢失。需在 `onMounted` 中直接调用 `list_denoise_models` 兜底加载
- **设置窗口启动竞争条件**：`openSettings()` 在 `loadSettings()` 完成前就能被调用，此时 `settings.value` 还是默认值（`hotkeyEnabled: true`），设置窗口会拿到错误状态。解决：`openSettings()` 加 `if (!initialized) return` 守卫，初始化完成前不打开设置窗口；SettingsPage 的 `settings-init` 监听加 `settingsInitialized` 标记，首次接收填充全部配置，后续只同步设备/模型列表
- **WASAPI 监听设备自动跟随**：监听使用系统默认输出设备，但 WASAPI 客户端在创建时绑定设备，不会自动跟随系统默认设备变更。需要每秒检查 `find_device(None, false)` 的 ID 是否变化，变化时 emit `restart-needed` 事件触发完整引擎重启（跟切换输入设备一样），确保 RNNoise 模型和 EQ 状态被重置
- **系统声音输出不能选 VB-Cable**：安装 VB-Cable 后系统默认输出会变为 CABLE Input，用户需手动改回耳机/扬声器，否则听不到系统声音。教程页面需明确说明
- **WASAPI 输出缓冲区溢出**：`output_buffer` 按 `frame_size * output_bytes_per_frame` 分配，但重采样后 `out_frames` 可能膨胀（如输出设备 96kHz 时翻倍）。必须按 `frame_size * (output_sample_rate / 48000).ceil()` 分配最大可能的缓冲区大小
- **WASAPI 输出 padding 跳帧**：当缓冲区 > 20ms 时跳过写入会导致可听到的卡顿。应始终写入，让 WASAPI 处理背压。跳过帧 = 音频间隙 = 卡麦
- **WASAPI 输出线程循环顺序**：必须按 wasapi-rs 官方 `playsine` 示例的顺序：`get_available_space → write_to_device → wait_event`。不能先 `wait_event` 再写——空缓冲区时事件不会触发，导致死锁。首次 `get_available_space` 返回整个缓冲区大小，自然填满，不需要预填充
- **EQ 后缺少 NaN/Inf 检查**：Biquad IIR 滤波器在极端参数下可能输出 NaN/Inf，穿透 BGM 混音污染输出。必须在 EQ 处理后添加 `is_finite()` 检查
- **RNNoise 模型被回声打废**：回声反馈导致输入能量飙升（>1000），RNNoise 内部归一化统计被污染后会将正常语音全部压制为 0，且损坏状态会被 `save_state()` 保存后下次启动又加载。注意：模型损坏检测/自动重建逻辑已移除（会误判正常安静帧为损坏导致永久绕过），如遇模型损坏需用户手动重启应用
- **Tauri 托盘左键点击**：`TrayIconBuilder` 默认左键也会弹右键菜单。用 `.show_menu_on_left_click(false)` 禁止左键弹菜单，配合 `.on_tray_icon_event` 处理左键单击打开主窗口
- **BGM 自动恢复标记**：`bgm_was_active` 必须在 `start_bgm()` 中设为 `true`，在 `stop_bgm()` 中也设为 `true`（记录用户主动开启的状态），在 `cancel_bgm_auto_restart()` 中设为 `false`（用户手动停止 BGM）。`start()` 检查 `bgm_was_active.swap(false)` 后自动重启 BGM。如果 `start_bgm()` 不设置此标记，引擎重启后 BGM 永远不会恢复
- **BGM 设备切换恢复**：`stop_inner()` 会清 `bgm_was_active`，导致设备切换（`scheduleRestart`）后 BGM 不会自动恢复。前端通过 localStorage 持久化 `meowmic-bgm-enabled` 和 `meowmic-bgm-pids`，重启后延迟 1 秒恢复 BGM。恢复失败时清除 localStorage 避免反复重试
- **std::sync::Mutex 中毒防护**：`app_handle`、`thread_handle`、`bgm_thread_handle` 的 `.lock().unwrap()` 必须改为 `.lock().unwrap_or_else(|e| e.into_inner())`，与 `lifecycle_lock` 保持一致。否则任何线程 panic 持有锁后，所有后续 lock 调用都会 panic，导致级联崩溃
- **BGM 目标进程退出后线程自退出**：`bgm_process_loop` 中 `capture.read_from_device()` 返回错误时，必须用连续错误计数器（≥10 次）break 退出循环，否则线程会以 1000 次/秒空转烧 CPU 直到引擎手动停止
- **f32::clamp NaN panic**：`f32::clamp()` 遇到 NaN 会 panic。`update_bgm_config` 等接收前端浮点值的函数必须先 `is_finite()` 检查再 clamp，或用 `.max().min()` 替代
- **SettingsPage 快捷键录制 listener 泄漏**：`startRecording()` 必须先调 `stopRecording()` 清理旧 handler，否则切换录制目标时旧 `window.addEventListener` 的回调无法移除，每次按键都会触发过期闭包
- **SettingsPage 快捷键注册失败不隐藏窗口**：`handleSave` 中 `registerHotkey` 抛异常后必须 `return`，不能继续执行 `getCurrentWindow().hide()`，否则用户看不到错误提示
- **EqPage 自身 emit 事件循环**：Tauri `emit` 会分发到所有窗口（包括发送方）。EqPage.vue 监听 `eq-changed` 时，自身发出的事件也会被处理，导致拖拽频率位置时被重置回默认值。解决：用 `isSelfEmitting` 标记，emit 前后设置，监听器开头检查跳过
- **BGM buffer 无条件 drain**：`audio_loop` 中消费 BGM 样本的 `bgm_buf.drain()` 必须在 `bgm_running` 为 true 时才执行。否则 BGM 关闭/重启期间，drain 会丢弃 channel 中残余的有效数据，导致 BGM 恢复时出现音频断裂
- **Vue watcher 启动时序**：`onMounted` 中 `loadConfig()` 设置 ref 值会触发 watcher，但此时引擎可能尚未启动。watcher 中调用 `updateConfig()` 发送 invoke 到未运行的后端会产生未处理的 Promise rejection。必须加 `initialized` 守卫
- **bgm_was_active 设置时机**：`bgm_was_active.store(true)` 必须在线程全部 spawn 成功后执行，不能在 spawn 前。否则任何 spawn 失败导致 `start_bgm` 返回 `Err` 后，标记残留为 true，下次引擎重启会反复尝试重启失败的 BGM
- **DeepFilterNet 输入范围**：DLL 期望 normalized f32 [-1.0, 1.0]。三线程重构后管线已是 normalized f32，直接传入 DLL 即可，不需要缩放。`has_internal_strength_control()` 返回 true 避免外部 strength mixing 双重衰减。⚠️ 不要加 `*32768` 缩放，DLL 期望的是 normalized 不是 i16 范围
- **DeepFilterNet reduce_mask**：DLL 的 `reduce_mask` 参数 0=NONE(Independent), 1=MAX, 2=MEAN。GUI 默认用 0，不要传 2
- **nnnoiseless 输入范围**：RNNoise 期望 i16 范围 [-32768, 32767]，内部静音阈值按此校准。归一化到 [-1, 1] 会导致所有帧被判定为静音直接跳过，完全丧失降噪能力
- **爆炸模式 dual-flag**：`explode_enabled`（AudioEngine 控制是否调用 `process_explode_into`）和 `explode_state.enabled`（ExplodeState 内部控制是否实际处理）是两个独立 flag。`set_explode_mode()` 必须同时同步两者，否则爆炸效果被跳过但调用链正常执行，表现为"开关打开了但没效果"
- **音频热路径禁止堆分配**：音频线程 48kHz 每秒处理 50 帧，任何 `Vec::new()` / `.collect()` / `.to_vec()` / `.clone()` 都会在帧间产生堆分配，饿死 WASAPI 回调导致整个音频冻结（频谱也停止）。所有效果函数必须写入预分配的 output buffer（`process_explode_into` 的 `output: &mut [f32]`），`process_input` 必须用 `_into` 版本的工具函数写入预分配 buffer，输出 channel 发送用 buffer 池轮换而非 `.to_vec()`
- **音频热路径禁止阻塞等待**：`write_to_monitor` 中的 `event.wait_for_event(2)` 最多阻塞 2ms（远小于 10ms 帧预算），避免监听写入拖慢主循环导致输出丢帧。原 `wait_for_event(20)` 会阻塞 20ms 导致持续丢帧
- **爆炸模式 echo 效果**：60% 强度 = 最大效果（`mix = (intensity / 60.0).min(1.0)`），延迟 10~80ms，反馈 0~0.4，湿声增益 0~0.8。delay_buf 大小 24000 samples（500ms @ 48kHz），延迟不能超过 buf_len 否则 usize 减法下溢
- **format_output_bytes 单声道→立体声扩展缺失**：v0.2.8 的 `format_output_bytes` 有 `upmix_to_stereo()` 先扩展再写字节，模块拆分（`5218730`）后丢失这一步，`samples.chunks(output_channels)` 把相邻 mono 样本错拆到左右声道。同时 32-bit float 路径缺少 `/32767` 归一化。症状：输出有刺啦杂音，安静时明显说话时被语音掩盖。修复：iterate mono frames × duplicate to channels，float 归一化
- **RNNoise/Biquad denormal 浮点数累积**：RNNoise 静音帧输出极小值（~1e-30），Biquad 滤波器 `y1/y2` 状态在长时间静音后累积 denormal。经 gain/EQ 放大后产生可闻刺啦声。修复：降噪+strength mixing 后 `abs() < 1e-10` flush to zero；Biquad `y1/y2` 同理
- **WebView2 GPU 进程占用**：Tauri 2 默认启用 GPU 硬件加速，WebView2 GPU 进程可能占用较高导致鼠标卡顿。优化手段：① `SpectrumVisualizer` 的 `requestAnimationFrame` 循环在无频谱数据时停止，避免 60fps 空转；② `AudioMeter` 从 32 个 DOM div 改为 Canvas 绘制，消除 class 切换 + CSS transition + box-shadow 的 GPU 合成开销；③ `useAudioStats` 轮询间隔从 50ms 放宽到 100ms，减少 invoke 和重绘频率
- **wasapi 0.16 → 0.23 API 变化**：`DeviceCollection::new()` 改为 `DeviceEnumerator::new()?.get_device_collection()`；`get_periods()` 改为 `get_device_period()`；`initialize_client` 参数从 5 个变为 3 个：`(format, direction, &StreamMode::EventsShared { autoconvert, buffer_duration_hns })`
- **输出缓冲区大小**：10ms 太小导致持续丢帧（处理速度跟不上实时要求）；50ms 可以但延迟大；20ms 是平衡点。偶发延迟靠重新预缓冲恢复
- **输出线程重新预缓冲**：缓冲区空了之后必须重新进入预缓冲状态（等攒够 3 帧再写），否则会持续写静音导致卡顿。`prebuffered` flag 在缓冲区空时重置为 false
- **帧处理积压限制**：每次迭代最多处理 2 帧，积压超过时跳过旧帧。否则处理慢时输入缓冲区溢出，导致持续卡顿
- **丢帧计数预热期**：启动前 1000 帧（约 10 秒）不计入丢帧计数，跳过 WASAPI 初始化、模型加载、EQ 配置等延迟
- **debug_log try_lock**：音频线程用 `Mutex::try_lock()` 而非 `lock()`，拿不到锁就丢弃日志，避免阻塞。BufWriter 8KB 缓冲区满时同步磁盘 I/O 会阻塞数毫秒
- **AudioStats.spectrum 不用 Vec**：`[f32; 32].to_vec()` 每 5 帧堆分配 128 字节，长期运行导致堆碎片化。改为 `[f32; 32]` 固定数组 + `copy_from_slice()`
- **模型必须在主线程预加载**：`denoise::create_model()` 必须在 `audio_loop` 中、spawn DSP 线程之前调用。在 DSP 线程内部加载模型会导致启动时原始音频直通产生回声（模型加载期间 capture 帧堆积，DSP 开始处理后时序混乱）。参考 noisegate-ref：模型在主线程加载完再启动管线
- **启动顺序 DSP → capture → render**：必须和 noisegate-ref 一致。先启动 render 会导致预填充静音后等待时间过长，先启动 capture 会导致帧堆积。DSP 线程先 spawn 并等待数据，capture 启动后立即被消费
- **监听用独立 render 线程**：监听功能用第二个 `WasapiRender` 线程（和主 render 完全一样的 `windows` crate 代码路径），不用 `wasapi` crate 的 `write_to_device`。DSP 线程把帧推入独立的 ring M，监听 render 线程从 ring M 拉取。避免 `wasapi` crate 和 `windows` crate 混用导致的缓冲区管理不一致
- **debug_log 文件需要 UTF-8 BOM**：`debug_log()` 写入 `%TEMP%\meowmic\debug.log`，新文件必须写入 BOM `[0xEF, 0xBB, 0xBF]`，否则 Windows 用 ANSI/Big5 编码读取导致中文设备名乱码（如"鑰虫満"代替"耳机"）
- **debug_log 文件大小限制**：5MB 硬上限（`MAX_LOG_SIZE_BYTES`），每 200 次写入检查文件大小，超限裁剪到 2000 行。引擎停止时兜底检查。`flush_debug_log()` 改为按文件大小（不再按行数）判断是否轮转
- **清理日志必须重置句柄**：`clear_logs` 删除 `debug.log` 后必须调 `reset_log_file()` 关闭缓存的 `BufWriter`。否则后续 `debug_log` 调用不会创建新文件（`guard.is_none()` 为 false），日志全部丢失
- **监听开关必须同步 monitorPoint**：前端 `handleMonitorChange` 开启监听时，必须同时调用 `setMonitorPoint(monitorPoint.value)`。否则 `mon_point` 为 0，监听流不会启动，用户听不到声音
- **WASAPI 物理设备启动回声**：输出到 Realtek 耳机等物理音频设备时，启动后有 1-2 秒回声然后自动消失。输出到 VB-Cable（虚拟声卡）无此问题。原因未确定，已发 Issue：https://github.com/NanCheng-L/MeowMic/issues/3
- **DeepFilterNet3 reduce_mask 参数**：`reduce_mask` 控制频带掩码合并方式，0=NONE(Independent)/1=MAX/2=MEAN。MAX 对瞬态噪声（鼠标点击）处理更稳定，推荐用 1
- **模型热切换**：切换模型不需要重启引擎。AudioEngine 通过 channel 传递 `(String, Box<dyn DenoiseModel>)` 给 DSP 线程，后台线程创建模型避免阻塞音频处理
- **调试日志分级**：`debug_log_dev()` 仅在 debug 构建（`cfg!(debug_assertions)`）打印，release 构建零开销。BGM 线程关键状态（启动/停止/peak level/错误）用 `debug_log()` 在 release 也写日志，方便排查用户问题
- **启动丢帧预热期**：DSP 线程前 100 帧的丢帧不计入统计（render 初始化期间 ring B 短暂满溢），启动时显示 0
- **WASAPI 监听多声道设备无声**：`init_monitor` 必须用 `autoconvert: true`（和 main render 一致），否则多声道设备（如 7.1 声道游戏耳机 IKF V11 Pro）会因格式不匹配报 `DataLengthMismatch`。代码写死 stereo f32（8 bytes/帧），设备期望原生格式（如 8ch × 4 bytes = 32 bytes/帧）
- **VB-Cable 污染默认设备**：安装 VB-Cable 后系统默认输入/输出设备都变为虚拟线缆（CABLE Input / CABLE Output）。`init_audio_devices` 和 `init_monitor` 必须自动跳过 VB-Cable 设备：输入端用 `find_first_real_capture_device()` 找真实麦克风，监听端用 `find_monitor_device()` 枚举非 VB-Cable 的输出设备。`is_virtual_cable()` 通过设备名匹配 "cable input" / "cable output" / "vb-audio"

## 版本号管理

版本号需同时更新三处：

- `package.json` 的 `version`
- `src-tauri/Cargo.toml` 的 `version`
- `src-tauri/tauri.conf.json` 的 `version`（Tauri 打包使用的实际版本）

## 版本公告模板

用户说"整理版本公告"时，执行以下逻辑：

1. 用 `git log --oneline <上一版本号对应commit>..HEAD` 获取变更列表
2. 按 修复/新增/改进/文档 分类整理
3. 用以下模板输出中文公告：

```
## MeowMic vX.X.X 更新公告

---

**🐱🎤 MeowMic vX.X.X 已发布！**

<一句话概述本次更新重点>

### 🔧 修复
- <修复项>

### ✨ 改进
- <改进项>

### 📜 文档
- <文档变更>（如有）

---

📥 **下载**：[GitHub Releases](https://github.com/NanCheng-L/MeowMic/releases/latest)
💬 **反馈**：[GitHub Issues](https://github.com/NanCheng-L/MeowMic/issues)
```

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
- **git add、git commit 只有用户明确说"提交 git"时才执行，不要主动提交**
- 删除文件、目录或 git 历史必须先问
- git push、git rebase、git reset --hard、强制推送必须先问

