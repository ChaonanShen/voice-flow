# voice-flow · 语音输入法

按住快捷键说话，松开自动粘贴文本到光标位置。**端侧 ASR 默认开启**，离线可用、隐私不离开设备；云端引擎作为可选增强。

> 状态：开发中。本 README 为骨架，随各 Step 逐步完善。基础语音转文本路线见 [`plan.md`](./plan.md)，AI 改写路线见 [`ai_rewrite_plan.md`](./ai_rewrite_plan.md)，Windows GUI 提升路线见 [`desktop_gui_plan.md`](./desktop_gui_plan.md)。

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
voice-flow/
├── Cargo.toml              # workspace
├── crates/
│   ├── voice-core/         # 录音 + 引擎路由 + 模式系统
│   ├── voice-asr-local/    # 端侧 ASR（sherpa-onnx）
│   ├── voice-asr-cloud/    # 云端 ASR（后期）
│   ├── voice-rewrite/      # AI 改写管道（文字→文字）
│   └── voice-cli/          # CLI 验证工具
├── apps/desktop/           # Tauri 应用（Step 6 引入）
├── models/.gitkeep         # gitignore 实际权重，由脚本下载
└── docs/                   # 演示脚本
```

## 四、构建与运行

```bash
# 准备 sherpa-onnx 静态库 archive 和 Streaming Zipformer 模型
bash scripts/download-models.sh

# 编译。sherpa-onnx build.rs 会从本地 archive 读取静态库，不需要联网下载 release 包
export SHERPA_ONNX_ARCHIVE_DIR=$HOME/.cache/voice-flow/sherpa-onnx
cargo build

# 录音 5 秒
cargo run -p voice-cli -- record out.wav --duration 5s

# 识别 WAV
export VOICE_FLOW_SHERPA_ZIPFORMER_MODEL_DIR=models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20
cargo run -p voice-cli -- transcribe out.wav
```

桌面端开发运行：

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe run --manifest-path apps/desktop/src-tauri/Cargo.toml
```

桌面端当前包含两种输出模式：

- `悬浮窗模式`：结果输出到外部应用，走 `clipboard -> simulated paste`
- `文稿模式`：结果写入 voice-flow 内部编辑区，不自动粘贴到外部应用

桌面端当前也支持：

- ASR engine 选择：`local` / `cloud`
- rewrite provider keyring
- cloud ASR keyring
- trace overlay
- diagnostics 和打开日志目录

## 五、AI 改写

AI 改写是可选的文字管道，默认关闭。开启后链路变成：

```text
录音 → ASR → rewrite(profile) → 剪贴板 → 粘贴
```

纯文字调试最快：

```bash
export DEEPSEEK_API_KEY=...
echo "嗯，跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我，那个语气正式一点。" \
  | cargo run -p voice-cli -- rewrite --profile clean --provider deepseek
```

WAV 转写后改写：

```bash
cargo run -p voice-cli -- transcribe out.wav --rewrite clean --rewrite-provider deepseek
```

实时按住说话命令也支持改写，未传 `--rewrite` 时仍粘贴 ASR 原文：

```bash
cargo run -p voice-cli -- push-to-talk-transcribe --rewrite clean
```

可用 profile：`off`、`clean`、`polish`、`email`、`wechat`、`bullets`、`commit`、`prompt`、`multi`。可用 provider：`deepseek`、`dashscope`、`openai`；对应环境变量为 `DEEPSEEK_API_KEY`、`DASHSCOPE_API_KEY`、`OPENAI_API_KEY`。非默认 provider 建议显式传 `--model` / `--rewrite-model`。

本地测试不需要真实 LLM key：

```bash
cargo test -p voice-rewrite
cargo test -p voice-core text_pipeline
```

真实 DeepSeek smoke 测试默认 ignored，手动运行：

```bash
cargo test -p voice-rewrite --test live_deepseek_examples -- --ignored --nocapture --test-threads=1
```

更多演示步骤见 [`docs/demo-rewrite.md`](./docs/demo-rewrite.md)。当前阶段完成项、GUI 运行方式和测试记录见 [`docs/rewrite-stage-report.md`](./docs/rewrite-stage-report.md)。

## 六、桌面端文档

- Windows 手测与 QA：[`windows_testing.md`](./windows_testing.md)
- Desktop GUI 路线：[`desktop_gui_plan.md`](./desktop_gui_plan.md)
- Desktop 打包：[`docs/desktop-packaging.md`](./docs/desktop-packaging.md)

## 七、模型下载

`models/` 目录下不入库实际权重。下载脚本默认把 archive 缓存在 `$HOME/.cache/voice-flow/sherpa-onnx`，并把模型解压到 `models/`：

```bash
bash scripts/download-models.sh

# 只下载 archive，不解压模型
EXTRACT=0 bash scripts/download-models.sh
```

## 八、Demo 视频

待 Step 9 完成后补充 B 站链接。

## 九、许可证

[Apache License 2.0](./LICENSE)
