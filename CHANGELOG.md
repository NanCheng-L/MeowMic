# Changelog

本文档记录 MeowMic 的版本更新历史。

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
