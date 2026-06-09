# MeowMic（喵咪麦克）路线图

## 当前状态

Phase 0-11 + 设备热拔插 + i18n 已完成（2026-06-06）：
- ✅ Tauri 2 + Vue 3 项目初始化
- ✅ WASAPI 音频引擎
- ✅ Vue 组件（设备选择 / 降噪控制 / 电平表 / 频谱 / 状态徽章）
- ✅ 系统托盘 + 最小化到托盘
- ✅ 全局快捷键（可自定义）
- ✅ 开机自启动 + `--hidden` 静默启动
- ✅ 单实例限制
- ✅ 设置界面 + 配置持久化（tauri-plugin-store）
- ✅ BGM 混音（WASAPI Process Loopback 按进程捕获音频）
- ✅ 一键炸麦（增益 + 方波失真，强度 1-100% 可调，0% 即有效果）
- ✅ 频谱频率坐标标注
- ✅ 设备热拔插检测（后台轮询 + Tauri 事件通知）
- ✅ 多语言支持（vue-i18n，中文 / 英文）

## 迭代计划

### 第一阶段：能跑通

> WASAPI 采集 → RNNoise → VB-Audio 输出

- [x] WASAPI 设备枚举 + 音频流
- [x] 简易降噪（simple_denoise）
- [x] 替换为 RNNoise（nnnoiseless crate，即 RNNoise 纯 Rust 实现）
- [x] VB-Audio Virtual Cable 输出验证

### 第二阶段：能用

> Vue UI 完善 + 设备热切换 + 状态显示

- [x] 设备选择 + 刷新
- [x] 降噪强度调节
- [x] 电平表 + 频谱可视化
- [x] 状态徽章（运行中/延迟）
- [x] 设备热拔插检测

### 第三阶段：好用

> 多模型架构 + AI 模型替换 RNNoise + 预设模式 + 托盘常驻 + 多语言

- [x] 多模型架构（DenoiseModel trait + 模型选择 UI）— 2026-06-06
- [ ] DeepFilterNet 模型集成（tract-onnx 推理，3 个 ONNX 模型：enc + erb_dec + df_dec）— 代码已写好但炸麦，需异步加载
- [ ] 自定义模型训练（使用 DNS Challenge 数据集微调）
- [x] 预设模式（安静/标准/嘈杂/直播，滑块联动）— 2026-06-06
- [x] 托盘常驻 + 开机自启动
- [x] 全局快捷键切换
- [x] 多语言支持（中文 / 英文，vue-i18n）
- [x] 恶搞功能：一键炸麦（50x 增益 + 方波失真，10s 自动关闭）
- [x] BGM 混音（WASAPI Process Loopback 按进程捕获 + 混音输出）

#### BGM 混音功能详设

架构：
```
物理麦克风 → 降噪 ─┐
                   ├→ 混音 → 虚拟麦克风 → OBS/Discord
QQ 音乐/网易云 ────┘   (WASAPI Loopback 捕获)
```

- WASAPI Loopback 捕获当前播放设备的音频
- PCM 采样点混音（加权叠加 + soft limiter 防爆）
- 设置界面：BGM 开关、来源设备选择、麦克风/BGM 音量比例滑块
- 自动检测当前正在播放音频的设备
- 需处理采样率不同时的重采样

### 第四阶段：能卖

> 自定义虚拟驱动 + 打包分发 + 商业化

#### 分发方案：Steam

- [ ] 办个体工商户营业执照（几百块，几天）
- [ ] 注册 Steamworks 合作伙伴（$100 一次性）
- [ ] 填写税务和银行信息
- [ ] Steam 审核上架
- [ ] Steamworks SDK 集成（`steam_api64.dll`）

#### 虚拟音频驱动

当前依赖 VB-Audio Virtual Cable（用户需单独安装），商业化需要去掉这个依赖。

**选定方案：基于 Microsoft SysVAD 改造**

SysVAD 是微软官方 WDK 示例虚拟音频设备，原生支持 loopback（环回），MIT 协议。
仓库已克隆到 `D:\web\Windows-driver-samples`。

SysVAD loopback 架构：
```
System Pin 0 → Loopback → WASAPI 捕获（MeowMic 读取）
Offload Pin → Topology → Line Out（实际音频输出）
```

| 改造项 | 复杂度 | 说明 |
|---|---|---|
| 删除蓝牙/USB/HDMI 端点 | 低 | 只保留 Speaker 端点 |
| 删除 APO 插件 | 低 | 删掉 SwapAPO、DelayAPO、AecAPO、KwsAPO |
| 删除关键词检测 | 低 | 删掉 KeywordDetectorAdapter |
| 重命名设备 | 低 | INF 改名 "MeowMic Virtual Speaker" |
| 添加捕获端（可选） | 中 | 如需虚拟麦克风输出降噪后音频，需加 Capture pin |
| 驱动签名 | 无 | 微软免费证明签名（Partner Center） |

改造步骤：
1. 安装 WDK（Windows Driver Kit）
2. Visual Studio 打开 `sysvad.sln`
3. 删除不需要的子项目，只保留 Speaker + Loopback 端点
4. 编译测试
5. 微软 Partner Center 提交证明签名

#### 其他商业化基础设施

- [ ] 微软 Partner Center 注册（免费，用于驱动证明签名）
- [ ] NSIS/Inno Setup 安装包
- [ ] 自动更新（tauri-plugin-updater）
- [ ] 许可证/激活机制

## 降噪模型架构

多模型选择架构（2026-06-06 实现）：

```
src-tauri/src/denoise/
  mod.rs          # DenoiseModel trait + list_models() + create_model() 工厂
  rnnoise.rs      # RNNoise 适配器（nnnoiseless crate）
  deepfilter.rs   # DeepFilterNet3 适配器（tract-onnx + 3 个 ONNX 模型）
```

- **统一接口**：`DenoiseModel` trait，480 samples/帧、48kHz、f32 输入输出
- **工厂模式**：`create_model(name)` 按名称创建模型实例
- **前端选择**：DenoiseControl 组件模型下拉框，切换时自动重启引擎
- **扩展方式**：新建 `denoise/xxx.rs` + 实现 trait + 在 `mod.rs` 注册即可

待集成模型路线：
1. **自训模型** — DNS Challenge 数据集 + PyTorch 训练 → ONNX 导出 → tract 推理

## 关键踩坑记录

详见各项目 CLAUDE.md，此处汇总：

- **Tauri autostart 插件**：方法名是 `autolaunch()` 不是 `autostart()`
- **autostart disable 静默失败**：`disable()` 有时不删注册表，需启动时同步
- **Tauri 插件权限**：必须在 `capabilities/default.json` 声明，否则运行时报错
- **单实例互斥锁**：`Global\` 前缀的命名互斥锁，`FindWindowW` 激活已有窗口
- **`--hidden` 参数**：开机自启动时传入，窗口默认 `visible: false`，手动启动时才 `show()`
