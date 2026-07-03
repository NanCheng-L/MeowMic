# Changelog

本文档记录 MeowMic 的版本更新历史。

## [0.2.15] - 2026-07-03

### 新增

- **模型热切换**：切换 RNNoise / DeepFilterNet3 不再需要重启引擎，后台线程创建模型，DSP 线程微秒级替换，无卡顿无静音
- **启动丢帧预热期**：前 100 帧（约 2 秒）不计入丢帧统计，启动时显示 0
- **调试日志分级**：新增 `debug_log_dev()` 函数，仅 debug 构建打印，release 构建零开销

### 修复

- **DeepFilterNet3 reduce_mask**：从 0 (NONE) 改为 1 (MAX)，瞬态噪声（鼠标点击等）处理更稳定

### 改进

- **调试日志优化**：生产环境只保留关键信息（设备配置、健康统计、错误日志），开发环境保留详细日志
- **DeepFilterNet3 参数注释**：添加详细的初始化参数说明（channels/atten_lim_db/post_filter_beta/reduce_mask）

## [0.2.14] - 2026-06-30

### 修复

- **音频热路径堆分配**：`AudioStats.spectrum` 从 `Vec<f32>` 改为 `[f32; 32]` 固定数组，消除每 5 帧的堆分配
- **输出线程静音路径堆分配**：预分配 `silence_buf`，无数据时复用
- **debug_log 阻塞音频线程**：`Mutex::lock()` 改为 `try_lock()`，拿不到锁就跳过
- **HP 滤波器 denormal 累积**：静音时 `hp_x_prev`/`hp_y_prev` 累积极小值，添加 flush to zero
- **SLOW ITER 不计入丢帧**：WASAPI 事件等待超时也计入 `frames_dropped` 计数器

### 改进

- **wasapi crate 升级 0.16 → 0.23**：API 更新，使用 `DeviceEnumerator` + `StreamMode`
- **输出缓冲区优化**：使用 20ms 缓冲区（平衡延迟和稳定性）
- **输出线程预缓冲**：启动时攒够 3 帧再开始写，缓冲区空时重新预缓冲
- **帧处理积压限制**：每次迭代最多处理 2 帧，积压超过时跳过旧帧
- **丢帧检测**：新增 `frames_dropped` 计数器，前端显示丢帧数和警告
- **MMCSS Pro Audio 提权**：音频线程注册为 Windows Pro Audio 类别，获得最高调度优先级

## [0.2.13] - 2026-06-27

### 修复

- **监听丢帧**：输出缓冲区空时写静音填充，防止 WASAPI 欠载
- **输出缓冲区空转卡顿**：按 wasapi-rs 官方 playsine 示例重写输出线程循环顺序

## [0.2.12] - 2026-06-24

### 修复

- **Biquad denormal 阈值**：从 1e-10 提高到 1e-4，防止长时间运行 denormal 累积拖慢 CPU

## [0.2.11] - 2026-06-24

### 修复

- **降噪输出次声波能量**：RNNoise/DeepFilterNet 说话后在 80Hz 以下产生次声波残留，频谱表显示 -50dB 跳动但人耳听不到。添加一阶 IIR 高通滤波器（fc=80Hz）切除该频段
- **denormal flush 检查不完整**：补全 `is_finite()` 检查，防止 NaN/Inf 穿透音频链路

## [0.2.10] - 2026-06-23

### 修复

- **output_thread usize 减法下溢**：缓冲区溢出保护中 `output_buf.len() - read_pos` 改为 `saturating_sub`，防止极端竞争下 panic
- **爆炸模式双 flag 原子操作不一致**：`set_explode_mode` 的两个 `AtomicBool` 从 `Relaxed` 改为 `Release`，确保 ARM64 上可见性一致
- **BGM 混音 bgm_read_pos 越界**：BGM 数据不足时 `bgm_read_pos` 可能超过缓冲区长度，添加边界检查并只消费可用部分
- **BGM 线程每帧堆分配**：`Vec<i16>` 的 `.collect()` 改为 buffer 池轮换，消除高频堆分配
- **Biquad 滤波器 denormal 累积**：`x1/x2` 状态变量添加 denormal flush，消除长时间静音后的刺啦声

### 改进

- **托盘菜单多语言支持**：根据语言设置显示中文/英文菜单（显示窗口/隐藏窗口/退出）
- **EQ 同步逻辑去重**：提取 `syncEqConfig()` 函数，消除 `handleStart` 中两处重复代码

### 文档

- 添加 GPL-3.0 开源协议（LICENSE + README 说明 + package.json/Cargo.toml 声明）

## [0.2.9] - 2026-06-22

### 修复

- **输出杂音和丢帧**：修复格式输出和音频处理链路中的多个问题
- **间歇性丢帧**：消除磁盘 I/O 尖刺导致的音频中断
- **BGM drain 性能**：改为 read_pos 索引追踪，消除每帧 O(n) memmove
- **音频热路径零分配**：爆炸模式、EQ、BGM 混音等处理函数改为写入预分配 buffer
- **echo 预设重做**：优化回音效果参数
- **output_thread 抽帧**：热路径零 drain，消除输出线程卡顿
- **爆炸模式失效**：修复 dual-flag 同步问题导致爆炸效果不生效
- **WebView2 GPU 占用**：频谱可视化和电平表降低 GPU 合成开销，解决鼠标卡顿

### 改进

- 模块拆分：audio_engine.rs 拆分为 audio_process.rs、audio_utils.rs 等独立模块
- 清理死代码：移除 upmix_to_stereo_into 和不可达 match 分支

## [0.2.8] - 2026-06-21

### 修复

- **音频断断续续**：修复 WASAPI 输出线程 recv_timeout 阻塞导致的音频断续
- **监听设备自动跟随**：每秒检测系统默认输出设备变更，变化时自动重启引擎

### 改进

- 优化输出线程：从 recv_timeout 改为 try_recv 非阻塞接收
- 补充爆炸模式 dual-flag 踩坑文档

## [0.2.7] - 2026-06-20

### 修复

- **游戏场景音频颤音**：三线程解耦架构（输入/处理/输出），解决高 CPU 负载下的音频卡顿
- **BGM 漂移补偿**：输出线程根据 pending 水位计算跳帧率，从源头减缓 BGM 数据流入

### 新增

- 集成 DeepFilterNet3 降噪模型（DLL FFI 调用）

### 改进

- 音频引擎模块化拆分，提取处理链函数
- BGM 音量持久化

## [0.2.6] - 2026-06-19

### 修复

- **音频格式解析错误导致电流麦**：K7 USB 麦克风等使用 24bit 格式的设备，因 `bytes_to_f32_samples` 缺少 24bit 处理导致字节错位产生电流声
- **监听设备格式不匹配**：监听设备错误使用输出设备（CABLE Input）的采样率，而非监听设备自身的格式，长时间运行后缓冲区积累异常数据
- **监听写入格式不匹配**：监听设备初始化可能使用非 16bit 格式，但写入代码固定按 i16 处理
- **输出写入格式不完整**：只处理了 32bit float，其他格式（24bit 等）会导致字节错位产生破音

### 改进

- 补全音频格式支持：8bit int、16bit int、24bit int、32bit int、32bit float、64bit float
- 监听设备初始化时获取自身采样率，固定使用 16bit int 格式写入
- 输出写入支持所有常见音频格式，未知格式 fallback 时打印警告日志

## [0.2.5] - 2026-06-12

### 修复

- EqPage 频率拖拽重置问题
- BGM buffer 误 drain 问题
- watcher 启动时序问题

### 改进

- EqPage DEFAULT_FREQUENCIES 与后端对齐

## [0.2.4] - 2026-06-10

### 修复

- 第四轮自检修复：EqPage 频率拖拽重置、BGM buffer 误 drain、watcher 启动时序等 5 项

### 文档

- 第四轮踩坑警示：EqPage emit 循环、BGM buffer drain、watcher 时序等
