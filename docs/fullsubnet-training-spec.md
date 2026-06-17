# FullSubNet+ 训练方案

## 目标

训练一个轻量级实时语音降噪模型，部署到 MeowMic 桌面应用中。

### 约束

| 参数 | 值 |
|------|-----|
| 采样率 | 48kHz |
| 帧大小 | 480 samples（10ms） |
| 延迟要求 | <20ms（含推理） |
| 推理设备 | CPU（Rust ort crate 加载 ONNX） |
| 模型大小 | <10MB |

## 架构：FullSubNet+

**论文**：*FullSubNet+: Full-Band and Sub-Band Fusion Model with Speaker Information for Real-Time Speech Enhancement* (ICASSP 2022)

### 核心思路

FullSubNet+ 有三条并行分支：

1. **Full-band Branch**：处理完整频谱（481 bins），用 BiLSTM 捕捉全局频谱结构
2. **Sub-band Branch**：每个子频带独立处理（25 个子频带），用 BiLSTM 捕捉频带间关系
3. **Fusion Layer**：合并全频带和子频带特征，输出每个频率 bin 的复数 mask

### 因果模式改造

实时推理需要将 BiLSTM 改为 causal 模式（只看过去帧，不看未来帧）：
- 移除反向 LSTM，只保留正向
- 或用单向 GRU 替代 BiLSTM
- 这会略微降低质量，但满足延迟要求

### 模型结构

```
Input: noisy_spec [B, 1, T, 481]  (复数频谱)
       ↓
   ┌───┴───┐
   │ Full-band │  → LSTM → Linear → full_feat [B, T, 481]
   └───┬───┘
       │
   ┌───┴───┐
   │ Sub-band │  → 每个子频带独立 LSTM → Linear → sub_feat [B, T, 481]
   └───┬───┘
       │
   Fusion Layer (concat + Linear) → complex_mask [B, T, 481, 2]
       ↓
   Apply mask → enhanced_spec
       ↓
   iSTFT → enhanced_audio [B, T, 480]
```

## 数据集

### 来源

Microsoft DNS Challenge 数据集：
- GitHub: https://github.com/microsoft/DNS-Challenge
- Azure Blob Storage: https://dns4public.blob.core.windows.net/dns4archive/datasets_fullband

### 存储位置

数据集存储在外置硬盘 `E:\dns-challenge-data\`，避免占用系统盘空间。

### 子集策略（先验证再扩大）

第一轮训练用小数据集验证方案可行性：

| 数据 | 大小 | 存储路径 | 用途 |
|------|------|----------|------|
| clean_fullband | ~50GB（子集） | `E:\dns-challenge-data\clean_fullband\` | 干净语音 |
| noise_fullband | ~10GB（子集） | `E:\dns-challenge-data\noise_fullband\` | 噪声 |
| impulse_responses | ~6GB（全部） | `E:\dns-challenge-data\impulse_responses\` | 混响 |

### 数据合成

使用 DNS Challenge 的 `noisyspeech_synthesizer_singleprocess.py` 生成训练对：
- 干净语音（`E:\dns-challenge-data\clean_fullband\`）+ 噪声（`E:\dns-challenge-data\noise_fullband\`）→ 带噪语音
- 可选：+ 混响（`E:\dns-challenge-data\impulse_responses\`）
- SNR 范围：-5dB ~ 20dB（随机采样）
- 输出：48kHz, 16-bit WAV

## 训练配置

### 超参数

| 参数 | 值 |
|------|-----|
| 优化器 | AdamW (lr=1e-3, weight_decay=1e-4) |
| 学习率调度 | CosineAnnealingLR, T_max=100 |
| 批大小 | 32（根据 GPU 显存调整） |
| 序列长度 | 48000 samples（1秒） |
| 训练轮数 | 200 epochs |
| 混合精度 | AMP (fp16) |

### 损失函数

多损失组合：

1. **SI-SNR Loss**（主损失）：尺度不变信噪比，语音增强标准指标
2. **Multi-Resolution STFT Loss**（辅助）：多分辨率 STFT 幅度损失，改善频谱质量
3. **Perceptual Loss**（可选）：基于 PESQ 的感知损失

```
total_loss = si_snr_loss + 0.5 * mrstft_loss
```

### 数据增强

- 随机裁剪（1秒片段）
- 随机翻转（时间反转，增加数据多样性）
- 随机移位（相位偏移）
- SpecAugment（频率掩码 + 时间掩码）

## 导出与部署

### ONNX 导出

```python
# PyTorch → ONNX
torch.onnx.export(
    model,
    dummy_input,  # [1, 1, 481] (real + imag concat)
    "fullsubnet_plus.onnx",
    opset_version=14,
    input_names=["input"],
    output_names=["output"],
    dynamic_axes={"input": {0: "batch"}, "output": {0: "batch"}}
)
```

### Rust 集成

实现实现新的 `DenoiseModel` trait，部署新模型：
- 保持 `DenoiseModel` trait 接口不变
- STFT/iSTFT 实现自定义或使用现有 crate
- 推理逻辑：STFT → 准备输入 tensor → ORT 推理 → apply mask → iSTFT

### 延迟预算

| 阶段 | 耗时 |
|------|------|
| STFT | ~0.5ms |
| ONNX 推理 | ~2-3ms |
| iSTFT | ~0.5ms |
| 总计 | ~3-4ms |

满足 <20ms 要求。

## 目录结构

```
training/
├── configs/
│   └── fullsubnet_plus.yaml      # 训练配置
├── data/
│   └── manifests/                 # 训练/验证文件列表（路径指向 E 盘）
├── models/
│   └── fullsubnet_plus.py         # 模型定义
├── utils/
│   ├── audio.py                   # 音频工具函数
│   ├── stft.py                    # STFT 实现
│   └── loss.py                    # 损失函数
├── train.py                       # 训练脚本
├── export_onnx.py                 # ONNX 导出脚本
├── evaluate.py                    # 评估脚本（PESQ/STOI/DNSMOS）
└── requirements.txt               # Python 依赖

E:\dns-challenge-data\             # 外置硬盘（数据集）
├── clean_fullband/                # 干净语音
├── noise_fullband/                # 噪声
└── impulse_responses/             # 混响
```

## 验证指标

| 指标 | 目标值 | 说明 |
|------|--------|------|
| PESQ | >3.0 | 语音质量（越高越好） |
| STOI | >0.90 | 语音可懂度 |
| DNSMOS | >3.5 | 非侵入式语音质量 |
| 推理延迟 | <5ms | CPU 单帧推理 |
| 模型大小 | <10MB | ONNX 文件大小 |

## 实施步骤

1. **环境准备**：安装 PyTorch、torchaudio、onnxruntime 等依赖
2. **数据下载**：从 ModelScope 下载 DNS Challenge 子集
3. **数据合成**：用 synthesizer 生成带噪-干净语音对
4. **模型实现**：实现 FullSubNet+ 的 PyTorch 代码 ✅
5. **训练**：在 GPU 上训练，监控 loss 和验证指标
6. **导出**：导出 ONNX 模型，验证数值一致性
7. **集成**：实现新的 DenoiseModel trait，端到端测试
8. **评估**：用 PESQ/STOI/DNSMOS 评估最终效果
