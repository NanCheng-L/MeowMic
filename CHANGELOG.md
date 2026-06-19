# Changelog

本文档记录 MeowMic 的版本更新历史。

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
