# xengineer · 语音输入法

按住快捷键说话，松开自动粘贴文本到光标位置。**端侧 ASR 默认开启**，离线可用、隐私不离开设备；云端引擎作为可选增强。

> 状态：开发中。本 README 为骨架，随各 Step 逐步完善。详细路线见 [`plan.md`](./plan.md)。

## 一、产品定位

- **默认模式 = 实时快速语音输入**：路径最短，ASR 直出 → 粘贴，无任何润色。
- **创新点 = 多模式架构**：实时模式 / 程序员模式 / 后续可扩展，不动核心。
- **端侧默认**：开箱即用、无网络依赖；云端为后期可选增强。

## 二、技术栈

| 层 | 选型 | License |
|---|---|---|
| 核心引擎 | Rust | - |
| 音频采集 | [`cpal`](https://crates.io/crates/cpal) | Apache-2.0 / MIT |
| 端侧 ASR（默认） | [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) + Streaming Zipformer-bilingual zh-en | Apache-2.0 |
| ASR 绑定 | 官方 [`sherpa-onnx`](https://crates.io/crates/sherpa-onnx) Rust binding | Apache-2.0 |
| 云端 ASR（后期可选） | 阿里云 DashScope Paraformer-realtime-v2 | - |
| 全局快捷键 | [`global-hotkey`](https://crates.io/crates/global-hotkey) | - |
| 剪贴板 / 模拟粘贴 | [`arboard`](https://crates.io/crates/arboard) + [`enigo`](https://crates.io/crates/enigo) | MIT |
| 桌面外壳 | [Tauri v2](https://tauri.app/) | Apache-2.0 / MIT |

## 三、项目结构

```
xengineer/
├── Cargo.toml              # workspace
├── crates/
│   ├── voice-core/         # 录音 + 引擎路由 + 模式系统
│   ├── voice-asr-local/    # 端侧 ASR（sherpa-onnx）
│   ├── voice-asr-cloud/    # 云端 ASR（后期）
│   └── voice-cli/          # CLI 验证工具
├── apps/desktop/           # Tauri 应用（Step 6 引入）
├── models/.gitkeep         # gitignore 实际权重，由脚本下载
└── docs/                   # 演示脚本
```

## 四、构建与运行

> 占位：随 Step 推进逐步补全。

```bash
# 编译（待 Step 1.4 workspace 落地后可用）
cargo build

# 录音 5 秒（待 Step 2 完成）
voice-cli record out.wav --duration 5s

# 识别（待 Step 3 完成）
voice-cli transcribe out.wav
```

## 五、模型下载

`models/` 目录下不入库实际权重，提供下载脚本（Step 3.7 引入）：

```bash
# 占位
bash scripts/download-models.sh
```

## 六、Demo 视频

待 Step 9 完成后补充 B 站链接。

## 七、许可证

[Apache License 2.0](./LICENSE)
