# 语音输入法 执行计划（v5）

## 一、产品定位

**核心**：实时快速语音输入法，按住快捷键说话，松开后文本自动粘贴到当前光标位置。

**关键设计原则**：
- **默认链路 = ASR 直出 → 粘贴**：最短路径，按住即说、松开即贴，无任何后处理
- **ASR 引擎可选 = 端侧 / 云端**：端侧用于离线、隐私场景；云端用于更高准确率与方言扩展。两条路径都是一等公民，用户按场景选
- **创新方向（待设计）= AI 改写**：参考 Wispr Flow / Superwhisper，把口语化原文再过一层 LLM，输出更适合当前场景的 prompt / 邮件 / commit 等。具体形态见 Step 10 草稿

近期开发顺序先服务 Windows 上的实时输入链路（Step 1-7）。云端引擎和 AI 改写在基础链路稳定后并行推进。

## 二、平台策略

- **Linux 只作为开发与核心逻辑验证环境**：当前机器用于写 Rust core / ASR / CLI、跑纯逻辑测试、跑文件回放与模型转写验证；不在 Linux 上安装或调试桌面外壳依赖，也不把 Linux 桌面可用性作为近期目标。
- **Windows 是桌面产品验收目标**：全局快捷键、麦克风、剪贴板、模拟粘贴、Tauri 悬浮窗和设置面板都以 Windows 原生环境为准；Step 5 之后尽快切到 Windows 上实测和继续开发。
- **架构保留多端余地**：核心仍保持平台无关，把录音源、ASR 引擎、状态事件、文本输出、配置存储抽象清楚。Windows 桌面、未来手机端或 Web 插件都只做 adapter，不复制核心链路。
- **移动端 / Web 插件后置探索**：如果后续要做手机端或浏览器插件，优先复用同一套"录音 → ASR → 文本处理 → 插入目标"核心模型；当前不为它们提前引入运行时或依赖。

## 三、技术栈

| 层 | 选型 | License |
|---|---|---|
| 核心引擎 | Rust | - |
| 音频采集 | `cpal` | Apache-2.0/MIT |
| **端侧 ASR**（默认） | sherpa-onnx + Streaming Zipformer-bilingual zh-en | Apache-2.0 |
| ASR 绑定 | 官方 `sherpa-onnx` Rust binding | Apache-2.0 |
| **云端 ASR**（可选增强，后期接入） | 备选阿里 DashScope Paraformer-realtime-v2 | - |
| 全局快捷键 | `global-hotkey` | - |
| 剪贴板/模拟粘贴 | `arboard` + `enigo` | MIT |
| Windows 桌面外壳 | Tauri v2（Windows-first） | Apache-2.0/MIT |
| 多端 adapter 预留 | Windows desktop / mobile / web extension | - |

## 四、项目结构

```
voice-flow/
├── README.md / LICENSE / .gitignore
├── Cargo.toml (workspace)
├── crates/
│   ├── voice-core/        ← 录音 + ASR 抽象 + 文本管道
│   ├── voice-asr-local/   ← sherpa-onnx 端侧引擎
│   ├── voice-asr-cloud/   ← 云端 ASR 适配（Step 8）
│   ├── voice-rewrite/     ← AI 改写管道（Step 10，草稿）
│   └── voice-cli/         ← CLI 验证工具
├── apps/desktop/          ← Windows-first Tauri 应用（Linux 不做桌面验收）
├── apps/mobile/           ← 后续预留，不在当前里程碑实现
├── apps/web-extension/    ← 后续预留，不在当前里程碑实现
├── models/.gitkeep        ← gitignore 实际权重
└── docs/                  ← 演示脚本
```

## 五、开发步骤（Step → PR）

> **粒度原则**：Step 是里程碑，每个 Step 拆成若干个**只做一件事**的细粒度 PR。
> 主线优先把实时输入链跑通并尽快在 Windows 桌面实测（Step 1-7）。Step 8 接入云端 ASR 作为引擎选项；Step 10 落地 AI 改写功能（具体设计在 §十三 草稿中）。

---

### Step 1：仓库初始化（地基）

**目标**：公开仓库、构建系统、空壳 crate，`cargo build` 通过。

| PR | 标题（示例） | 单一职责 |
|---|---|---|
| 1.1 | `chore: add .gitignore` | 仅添加 .gitignore（Rust + Tauri + 模型权重 + .env） |
| 1.2 | `chore: add Apache-2.0 LICENSE` | 仅添加 LICENSE 文件 |
| 1.3 | `docs: add README skeleton` | README 框架（项目介绍 / 依赖列表 / 构建说明占位） |
| 1.4 | `chore: init Cargo workspace` | 根 Cargo.toml workspace 配置 |
| 1.5 | `chore: scaffold voice-core crate` | 空 crate 1 |
| 1.6 | `chore: scaffold voice-asr-local crate` | 空 crate 2 |
| 1.7 | `chore: scaffold voice-cli crate` | 空 crate 3（CLI 入口） |

**Step 验收**：`cargo build` 通过，仓库 push 到 GitHub 公开访问。

---

### Step 2：录音能力

**目标**：CLI 能用 cpal 录音到 WAV。

| PR | 标题 | 单一职责 |
|---|---|---|
| 2.1 | `feat(core): add audio capture trait` | 定义 `AudioCapture` trait（不实现） |
| 2.2 | `feat(core): implement cpal capture backend` | cpal 后端实现，输出 PCM 流到内存缓冲 |
| 2.3 | `feat(core): write WAV file from PCM buffer` | PCM → WAV 文件写入 |
| 2.4 | `feat(cli): add record subcommand` | `voice-cli record out.wav --duration 5s` |
| 2.5 | `feat(core): add PCM 16-bit WAV reader` | 与 writer 对称，遍历 chunk，跳过 LIST 等未知 chunk |
| 2.6 | `feat(core): add file capture backend` | `FileCapture` 实现 `AudioCapture`，按真实采样率节奏回放 WAV |
| 2.7 | `feat(cli): record --input flag` | `voice-cli record --input file.wav` 走 FileCapture，用于无声卡环境与 demo 复现 |

**Step 验收**：`voice-cli record out.wav` 录 5 秒可播放的 WAV。

**实际验收（2026-05-23）**：

- **真麦克风路径（CpalCapture）**：当前 Linux 开发环境只做编译与纯逻辑验证，端到端录音留待 Windows 原生环境跑 demo 时再验。代码路径已编过，单元测试覆盖 trait/格式协商/i16 转换的纯逻辑部分。
- **文件回放路径（FileCapture）**：在 Linux 上完整验证通过。流程：用脚本生成 2s 16kHz mono 440Hz 正弦 WAV → `voice-cli record out.wav --input sine.wav --duration 3s` → FileCapture 按 100ms/chunk 节奏发送 20 个 chunk → 写出 WAV 与原文件**字节完全一致**（`cmp` 验证）。
- **单元测试**：`cargo test -p voice-core` 共 9 个测试全过（WAV writer × 2、reader × 4、FileCapture × 3）。
- **Step 2 实际拆分超出预想**：原计划 4 个 PR，实际 7 个。FileCapture 三个 PR（2.5/2.6/2.7）是 plan §6.6 提到的"能拆就拆"——用文件回放后端解决 Linux 无声卡环境下的端到端测试问题，同时让 demo 视频能用固定音频做可复现对照。
- **Step 1 也有偏离**：插入了一个独立 PR `chore: configure rsproxy mirror for crates.io`（清华 git 索引偶尔挂、改用 rsproxy sparse 镜像）。

---

### Step 3：端侧 ASR 引擎（默认）

**目标**：CLI 接入 sherpa-onnx，端侧识别一段 WAV 出文本。这是项目的核心默认能力。

| PR | 标题 | 单一职责 |
|---|---|---|
| 3.1 | `feat(asr): add AsrEngine trait` | 定义引擎抽象（trait + 错误类型） |
| 3.2 | `chore(asr-local): spike sherpa binding choice` | 验证 Rust binding、构建方式与离线 archive 方案 |
| 3.3 | `chore(asr-local): add sherpa-onnx dependency` | 仅加依赖、跑通最简调用 |
| 3.4 | `feat(asr-local): load streaming zipformer model` | 模型加载与初始化 |
| 3.5 | `feat(asr-local): implement non-streaming transcribe` | WAV → 文本（非流式接口先打通） |
| 3.6 | `feat(asr-local): implement streaming chunk decode` | PCM chunk 流式解码 |
| 3.7 | `feat(cli): add transcribe subcommand` | `voice-cli transcribe out.wav` 出文本 |
| 3.8 | `docs: add model download script` | `scripts/download-models.sh` + README 用法 |

**Step 验收**：`voice-cli transcribe out.wav` 端侧出中文文本，无需联网。

**实际进展（2026-05-23）**：

- **3.3 已完成**：`7af4564 chore(asr-local): add sherpa-onnx dependency`。`voice-asr-local` 引入官方 `sherpa-onnx = 1.13.2`，同步更新 `Cargo.lock`，并把 README / plan 中的 ASR 绑定从 `sherpa-rs` 改为官方 `sherpa-onnx` Rust binding。
- **3.4 已完成**：`d040d37 feat(asr-local): load streaming zipformer model`。新增 `StreamingZipformerModel` / `StreamingZipformerOptions` / `StreamingZipformer::from_model_dir`，校验 `encoder` / `decoder` / `joiner` / `tokens` 必需文件，并实际创建 sherpa `OnlineRecognizer`。
- **验证通过**：`SHERPA_ONNX_ARCHIVE_DIR=/home/scn/.cache/voice-flow/sherpa-onnx cargo build -p voice-asr-local`、`SHERPA_ONNX_ARCHIVE_DIR=/home/scn/.cache/voice-flow/sherpa-onnx cargo test --workspace`。
- **真实模型验证通过**：将模型 archive 解压到 `/tmp/voice-flow-models` 后，设置 `VOICE_FLOW_SHERPA_ZIPFORMER_MODEL_DIR=/tmp/voice-flow-models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20`，`cargo test -p voice-asr-local loads_real_model_when_env_is_set -- --nocapture` 能完成 recognizer 初始化并创建 stream。
- **遗留点**：`cargo fmt --all --check` 仍会因历史 `voice-core` 文件换行格式失败；未混入 3.3 / 3.4，建议后续单独开小 PR 处理。
- **3.5 已完成**：`4c145f8 feat(asr-local): implement non-streaming transcribe`。`StreamingZipformer` 实现 `AsrEngine::transcribe`，支持 PCM 16-bit → mono f32 转换、格式校验、整段 WAV 非流式识别。
- **3.6 已完成**：`184da38 feat(asr-local): implement streaming chunk decode`。新增 `StreamingSession`，支持 `accept_pcm` / `finish` / `text`，按 chunk 推进 sherpa online decoder。
- **3.7 已完成**：`23caa66 feat(cli): add transcribe subcommand`。`voice-cli transcribe <wav> --model-dir <dir>` 可读取 PCM 16-bit WAV 并输出识别文本；未传 `--model-dir` 时读取 `VOICE_FLOW_SHERPA_ZIPFORMER_MODEL_DIR`。
- **3.8 已完成**：`ae51b2a docs: add model download script`。新增 `scripts/download-models.sh`，缓存 sherpa 静态库和模型 archive 到 `$HOME/.cache/voice-flow/sherpa-onnx`，默认解压模型到 `models/`，并更新 README 构建/转写用法。
- **Step 3 最终验证通过**：`SHERPA_ONNX_ARCHIVE_DIR=/home/scn/.cache/voice-flow/sherpa-onnx cargo test --workspace` 全过；`cargo run -p voice-cli -- transcribe models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20/test_wavs/0.wav` 输出文本：`昨天是 MONDAY TODAY IS LIBR THE DAY AFTER TOMORROW是星期三`。

---

### Step 4：交互闭环（最简实时模式）

**目标**：按住快捷键说话，松开后文本自动出现在光标位置。

| PR | 标题 | 单一职责 |
|---|---|---|
| 4.1 | `feat(core): integrate global-hotkey` | 注册 `Ctrl+Alt+Space` 监听，仅打印事件 |
| 4.2 | `feat(core): wire hotkey to recording start/stop` | 按下开始录音、松开停止 |
| 4.3 | `feat(core): pipe recording into local asr` | 录音流送进端侧引擎，CLI 打印识别结果 |
| 4.4 | `feat(core): add clipboard write via arboard` | 识别结果写入系统剪贴板 |
| 4.5 | `feat(core): simulate paste via enigo` | 自动 `Ctrl+V` 粘贴到光标位置 |

**Step 验收**：在编辑器中按住快捷键说话，松开后文本自动出现在光标。

**实际进展（2026-05-23）**：

- **4.1 已完成**：`6f617e4 feat(core): integrate global-hotkey`。`voice-core` 新增 `hotkey` 模块，注册默认 `Ctrl+Alt+Space` 并输出 `pressed` / `released`；`voice-cli listen-hotkey` 用于人工验证。
- **4.2 已完成**：`d0cfdc3 feat(core): wire hotkey to recording start/stop`。新增 `PushToTalkRecorder` 状态机，按下启动 `AudioCapture`，松开 drop session 停止并返回 PCM；`voice-cli push-to-talk-record` 可松开后写 WAV。
- **4.3 已完成**：`49b3c11 feat(core): pipe recording into local asr`。新增 `voice-cli push-to-talk-transcribe`，松开后把本次录音送入端侧 `StreamingZipformer` 并打印文本。
- **4.4 已完成**：`6ec4d22 feat(core): add clipboard write via arboard`。新增 `SystemClipboard` / `ClipboardWriter`，识别结果写入系统剪贴板。
- **4.5 已完成**：`5f583d6 feat(core): simulate paste via enigo`。新增 `SystemPaste` / `PasteSimulator`，剪贴板写入后自动发送平台粘贴快捷键；Linux 依赖使用 `enigo` 的 `x11rb` feature，避免系统 `libxdo` 依赖。
- **自动验证通过**：`cargo test -p voice-core`（18 个测试）、`SHERPA_ONNX_ARCHIVE_DIR=/home/scn/.cache/voice-flow/sherpa-onnx cargo check -p voice-cli`、`SHERPA_ONNX_ARCHIVE_DIR=/home/scn/.cache/voice-flow/sherpa-onnx VOICE_FLOW_SHERPA_ZIPFORMER_MODEL_DIR=models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20 cargo run -p voice-cli -- transcribe models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20/test_wavs/0.wav`。
- **待人工验收**：当前 Linux 环境不承担桌面链路验收，无法真实判断 Windows 全局快捷键、系统剪贴板和模拟粘贴行为。需在 Windows 原生桌面运行 `voice-cli push-to-talk-transcribe --model-dir <model-dir>`，在编辑器里按住 `Ctrl+Alt+Space` 说话、松开后确认文本自动出现在光标。

---

### Step 5：Windows / 桌面实机验收优先

**目标**：先验证普通实时输入模式的真实可用性。重点不是新增模式，而是在 Windows 桌面环境里尽快跑通"按住快捷键说话 → 松开识别 → 写剪贴板 → 自动粘贴"。

> 这里要分清楚：
> - `transcribe`：只测端侧 ASR 能不能把音频转成文字
> - `record`：只测 Windows 麦克风录音
> - `listen-hotkey`：只测全局快捷键能不能收事件
> - `push-to-talk-transcribe`：这才是完整实时链路，包含麦克风、热键、ASR、剪贴板和自动粘贴

| PR | 标题 | 单一职责 |
|---|---|---|
| 5.1 | `docs: add Windows manual test checklist` | 写 Windows 首轮手测清单、模型/静态库准备、预期现象 |
| 5.2 | `fix(core): harden Windows paste shortcut` | 如实测发现 `enigo` 粘贴键序或焦点问题，仅修 Windows 粘贴 |
| 5.3 | `fix(core): harden Windows hotkey lifecycle` | 如实测发现按下/松开事件、重复触发或释放异常，仅修快捷键生命周期 |
| 5.4 | `fix(core): harden Windows microphone capture` | 如实测发现 cpal 设备协商、采样率或权限问题，仅修录音路径 |
| 5.5 | `docs: record Windows realtime smoke result` | 回写 Windows 实测结果、可复现命令、已知限制 |

**Step 验收**：在 Windows 的记事本 / VS Code 中运行 `voice-cli push-to-talk-transcribe --model-dir <model-dir>`，按住 `Ctrl+Alt+Space` 说普通中文，松开后文本自动出现在当前光标位置。

**实际进展（2026-05-23）**：

- **5.1 已完成**：`908e344 docs: add Windows testing guide`。新增 `windows_testing.md`，覆盖 Windows 原生环境准备、Windows 版 sherpa 静态库与模型准备、CLI 编译、WAV 转写、麦克风录音、全局快捷键和完整链路手测步骤。
- **5.1 补充完成**：Windows 指南已补充离线 archive 准备方式，测试机无法访问 GitHub 时由用户在可联网环境下载 `sherpa-onnx-v1.13.2-win-x64-static-MT-Release-lib.tar.bz2` 和模型 archive 后复制到本机缓存目录。
- **5.2 / 5.3 / 5.4 已由 Windows 实测驱动修正到桌面链路**：桌面端改用 Tauri 官方 `global-shortcut` 插件注册全局快捷键；Tauri v2 补 `capabilities/default.json`，允许前端监听状态事件；`CpalCapture` 从“必须单声道”改为“优先单声道，不支持时回退到设备可用通道数”，解决 Windows 默认麦克风只暴露双声道输入时报 `no input config matches channels=1` 的问题。
- **5.5 已完成首轮桌面实测记录**：Windows 原生 Git Bash / MSVC 环境下，`apps/desktop/src-tauri` 运行 `cargo run`，按住 `Ctrl+Alt+Space` 能调用麦克风，松开后端侧 ASR 转写，悬浮窗显示最近文本，并自动写入剪贴板 / 粘贴到当前光标位置。
- **CLI 完整链路仍可后续补测**：本轮人工确认的是桌面常驻链路；`voice-cli push-to-talk-transcribe` 仍保留为底层排障入口。

---

### Step 6：Windows-first 桌面外壳

**目标**：在 Windows 原生环境落地 Tauri 悬浮窗，显示普通实时输入链路状态，不引入模式切换。这里是把 Step 5 已经跑通的实时链路包成一个窗口程序，不是把“实时转文字”这件事首次做出来。Linux 继续只跑 core / CLI 验证，不安装或调试 Tauri Linux 系统依赖。

| PR | 标题 | 单一职责 |
|---|---|---|
| 6.1 | `chore(desktop): scaffold tauri v2 app` | Windows-first Tauri 项目骨架；不加入 Linux 主验证路径 |
| 6.2 | `feat(desktop): floating window layout` | 静态 HTML 悬浮窗 |
| 6.3 | `feat(core): add realtime state events` | core 定义"待机/录音/转写/完成"状态事件，供桌面/手机/Web adapter 复用 |
| 6.4 | `feat(desktop): bind state to UI indicator` | 前端监听并渲染状态 |
| 6.5 | `feat(desktop): show last transcript` | 显示最近一次识别文本 |

**Step 验收**：在 Windows 原生环境启动桌面应用，悬浮窗显示待机 / 录音 / 转写 / 完成状态和最近一次识别文本。Linux 本地只验证 core 事件类型和非桌面 CLI 不退化。

**实际进展（2026-05-23）**：

- **6.1 / 6.2 已完成**：Windows-first Tauri v2 应用骨架已在 `apps/desktop/` 落地，包含无边框置顶悬浮窗、状态区域、最近一次识别文本区域和设置入口。
- **6.3 已完成核心类型**：`voice-core` 新增 `RealtimeState` / `RealtimeStateEvent`，CLI 已开始输出状态日志，供桌面 / 手机 / Web adapter 复用。
- **6.4 / 6.5 已完成首版接入**：桌面后端常驻运行快捷键、录音、端侧 ASR、剪贴板和自动粘贴链路；前端监听 `realtime-state` / `runtime-error`，显示待机 / 录音 / 转写 / 完成 / 出错状态和最近一次识别文本。
- **自动验证通过**：Windows MSVC 环境下 `apps/desktop/src-tauri` 可完成 `cargo build`，并能启动 `target/debug/voice-flow-desktop.exe` 保持运行；`cargo test --workspace` 通过。
- **Windows 人工验收通过**：启动桌面应用后，按下 `Ctrl+Alt+Space` 会调用麦克风并进入录音链路；松开后完成端侧转写，悬浮窗“最近文本”更新，文本自动写入剪贴板并粘贴到当前光标位置。
- **已知修复项**：补齐 Tauri v2 capability 后解决 `event.listen not allowed`；桌面全局快捷键改走 Tauri `global-shortcut` 插件，避免后台线程直接注册 Windows 热键；录音后端支持麦克风通道数回退。

---

### Step 7：基础设置与持久化

**目标**：只做普通实时链路必需的基础配置，不做 mode 相关设置。配置读写先放在可复用 core/app config 层，Windows 桌面负责选择目录、展示和热更新；未来手机端或 Web 插件可替换存储 adapter。

| PR | 标题 | 单一职责 |
|---|---|---|
| 7.1 | `feat(core): add app config read/write` | TOML 配置文件读写，纯逻辑可在 Linux 测试 |
| 7.2 | `feat(desktop): persist model directory` | 模型目录持久化 |
| 7.3 | `feat(desktop): persist hotkey setting` | 快捷键配置持久化 |
| 7.4 | `feat(desktop): settings panel layout` | 设置面板 UI |
| 7.5 | `feat(desktop): hot reload realtime settings` | 模型目录 / 快捷键等实时配置热生效 |

**Step 验收**：Windows 桌面应用重启后模型目录和快捷键配置仍生效；Linux 本地只跑配置解析/写回单元测试，不涉及模式选择。

**实际进展（2026-05-23）**：

- **7.1 已完成核心实现**：`voice-core` 新增 `AppConfig` / `HotkeyConfig`，支持 TOML 读写、默认值回填和快捷键标签生成；对应单元测试已通过。
- **7.2 / 7.3 / 7.4 已完成首版接入**：桌面设置面板可编辑模型目录和快捷键组合，并通过 Tauri command 写入 `%APPDATA%\voice-flow\app.toml`。
- **7.5 已完成首版接入并通过基础链路验收**：保存设置后后端会请求 runtime 重启，重新加载模型目录并重新注册热键；无需关闭桌面程序。默认配置写入 `%APPDATA%\voice-flow\app.toml`。
- **后续增强**：当前模型目录先用文本框输入，后续可补原生目录选择器；热键冲突或非法按键会通过运行时错误展示，需要基于更多实测再决定是否做更强的保存前校验。

---

### Step 8：云端 ASR 引擎

**目标**：在端侧之外接入云端引擎，给用户按场景选择。端侧和云端是并列选项，不是默认/兜底关系。

| PR | 标题 | 单一职责 |
|---|---|---|
| 8.1 | `chore(asr-cloud): scaffold voice-asr-cloud crate` | 空 crate 结构 |
| 8.2 | `feat(asr-cloud): add dashscope http client` | HTTP 客户端 + auth |
| 8.3 | `feat(asr-cloud): paraformer-realtime websocket protocol` | 流式协议封装 |
| 8.4 | `feat(asr-cloud): implement AsrEngine for cloud` | 实现统一 trait |
| 8.5 | `feat(cli): transcribe --engine=cloud flag` | CLI 显式选择引擎 |
| 8.6 | `feat(core): engine router with manual selection` | 用户配置选 local/cloud，无静默 fallback |

**Step 验收**：在 CLI 和桌面端可显式选择 local / cloud，识别结果都能粘贴。

**实际进展（2026-05-23）**：

- **8.1 已完成**：`68c3181 chore(asr-cloud): scaffold voice-asr-cloud crate`。新增 `voice-asr-cloud` 空 crate，提供 `CloudProvider` / `CloudEngineConfig` 占位和 `CloudAsrError`（4 个单测）。
- **8.2 已完成**：`d68a9dc feat(asr-cloud): add dashscope http client`。`DashScopeClient` 提供 API key 校验、端点与 Bearer / `X-DashScope-DataInspection` 认证头；`map_http_status` 统一映射 HTTP 错误码（10 个单测）。
- **8.3 已完成**：`9e63340 feat(asr-cloud): paraformer-realtime websocket protocol`。`ClientEvent` / `ServerEvent` 编解码 `run-task` / `finish-task` / `task-started` / `result-generated` / `task-finished` / `task-failed`；未知 event 显式报 `Protocol`（11 个单测）。
- **8.4 已完成**：`e4393b0 feat(asr-cloud): implement AsrEngine for cloud`。`ParaformerCloudEngine` 用 `tokio-tungstenite` + per-call current-thread runtime 实现 `AsrEngine::transcribe`：建连 → run-task → 推 100ms 一片 PCM → finish-task → 收集所有最终句子；`CloudAsrError → AsrError` 映射（9 个单测）。
- **8.5 已完成**：`ad70338 feat(cli): transcribe --engine=cloud flag`。`transcribe` 子命令新增 `--engine local|cloud`、`--api-key`；cloud 缺 key 直接报错不静默回退。
- **8.6 已完成**：`c9a5006 feat(core): engine router with manual selection`。`voice-core::engine` 新增 `EngineKind` / `EngineSelection` / `resolve_engine_selection`，router 不读环境变量，env 政策留在 CLI 层（8 个单测）。
- **自动验证通过**：`SHERPA_ONNX_ARCHIVE_DIR=... cargo test --workspace` 全过；`cargo run -p voice-cli -- transcribe ... --engine local` 输出端侧识别文本与 Step 3 一致。
- **待人工验收**：Windows 上配置真实 `DASHSCOPE_API_KEY` 跑一次 `--engine cloud` 验证端到端识别；桌面端引擎切换在 Step 9 接入。

---

### Step 9：引擎相关设置

**目标**：把 Step 8 的引擎选择和云端 API key 接入桌面设置面板。

| PR | 标题 | 单一职责 |
|---|---|---|
| 9.1 | `feat(desktop): persist asr engine choice` | local / cloud 选择持久化 |
| 9.2 | `feat(desktop): api key settings` | API key 录入 + 安全存储（系统 keyring 或加密文件） |
| 9.3 | `feat(desktop): show current engine` | 悬浮窗显示当前引擎和状态（待机/网络异常等） |

**Step 验收**：重启后引擎选择和 key 持久化；切换引擎无需重启进程。

---

### Step 10：AI 改写（创新点，详细设计见 §十三）

**目标**：参考 Wispr Flow / Superwhisper，在 ASR 之后挂一段可选的 LLM 改写管道：把口语化原文重写成 prompt / 邮件 / commit 等更可用的文本。

> 这是当前的**草稿 Step**，PR 拆分等用户确认设计后再细化。先占位、不开工。

骨架 PR（占位，待 §十三 设计确认）：

| PR | 标题 | 单一职责 |
|---|---|---|
| 10.1 | `chore(rewrite): scaffold voice-rewrite crate` | 空 crate + trait 占位 |
| 10.2 | `feat(rewrite): add RewritePipeline trait` | 定义 `process(text, profile) -> text` |
| 10.3 | `feat(rewrite): identity pipeline` | 直通实现，验证集成不破坏现有链路 |
| 10.4 | `feat(rewrite): llm-based rewrite` | 接入云端 LLM 改写 |
| 10.5 | `feat(desktop): rewrite profile switch` | 桌面端切换改写档（关闭 / 邮件 / prompt 等） |

**Step 验收**：待 §十三 设计明确后回填。

---

### Step 11：文档与 Demo

| PR | 标题 | 单一职责 |
|---|---|---|
| 11.1 | `docs: complete README usage guide` | 完整使用文档 |
| 11.2 | `docs: add demo script` | `docs/demo-script.md` 演示脚本 |
| 11.3 | `docs: link demo video in README` | B 站视频链接 |

---

## 六、PR 与 commit 提交规范

> 来源：assignment-requirements.md §2.3。粒度要细、信息要规范，全周期持续交付。

### 6.1 PR 单一职责

- **每个 PR 只做一件事**：一个 PR 只实现或修改一个单一功能/重构/修复
- 大功能必须拆成多个独立 PR 分步提交（见上方 Step 拆分）
- 合并后主分支必须保持可运行（评委任意时间检出都能跑）

### 6.2 PR 标题规范

格式：`<type>(<scope>): <一句话说明本 PR 做了什么>`

- type：`feat` / `fix` / `docs` / `refactor` / `test` / `chore` / `perf` / `style`
- scope：`core` / `asr-local` / `asr-cloud` / `cli` / `desktop` / `mode` 等
- 一句话 ≤ 50 字符，动词开头，描述结果而非过程

示例：`feat(asr-local): implement streaming chunk decode`

### 6.3 PR 描述模板

```markdown
## 功能描述
（这个 PR 给用户/系统带来了什么能力，怎么使用）

## 实现思路
- 技术选型：（用了什么库/方法，为什么）
- 核心逻辑：（关键文件 + 关键流程）
- 关联：（如果依赖前置 PR，列出 #编号）

## 测试方式
1. `cargo build -p <crate>`
2. `voice-cli <command>`
3. 期望输出：……

## 风险与影响
（如有破坏性变更、新增依赖、模型权重等，必须列出）
```

PR 描述空白或与代码变更严重不符 = **无效作品**（见 §2.2）。

### 6.4 Commit 规范

- 遵循 [Conventional Commits](https://www.conventionalcommits.org/)，与 PR 标题同格式
- **一个 commit = 一个原子改动**，不要把无关改动塞进一个 commit
- 拒绝 `WIP` / `update` / `fix bug` / `修改` 这类无信息 commit
- 同一 PR 内允许多个 commit（鼓励，便于代码审查）

### 6.5 持续交付节奏

- 评审看 commit 时间分布——避免最后一天突击提交
- 每完成一个 PR 立即合并到 main，保持主分支可运行
- 建议：每 1-2 个 PR 一次 push，每天至少有一次有意义的提交

### 6.6 执行时按需进一步拆分（重要）

上方第四节中列出的 Step / PR 划分**只是预想**，目的是给整体节奏一个骨架，不是合同。

实际执行中遵循以下原则：

- **能拆就拆**：开发过程中一旦发现某个 PR 还可以继续拆成更小的独立步骤（例如"加依赖"和"跑通最简调用"是两件事、"trait 定义"和"trait 实现"是两件事），就立即拆开，不要为了贴合本计划硬塞进一个 PR
- **粒度参考线 ≈ 300 行**：单个 PR 的 diff 尽量控制在 300 行以内（含新增 + 修改，不含自动生成的 lock 文件）。这是参考线不是硬性指标——逻辑上不可分割的小重构、纯文档、模板代码可以更大；牵涉多模块联动的功能应该再拆
- **拆分优先级高于贴合计划**：如果某个 Step 实际拆出来的 PR 数量比预想多 50% 甚至翻倍，是好事不是问题，说明执行得更细更稳
- **合并时回写**：如果实际 PR 编号、拆分方式与本计划偏离较大，在该 Step 完成后回到 plan.md 同步实际情况，保持文档与仓库一致

简单说：**计划是路线，执行是脚步**。脚步比路线密一点，永远比稀一点好。

---

## 七、创新点（评审 40%）

**AI 改写管道**——参考 Wispr Flow / Superwhisper，但围绕中文场景做深：

- **核心机制**：ASR 出文本后，按用户当前选择的"改写档"再过一层 LLM，输出可直接使用的成品文本（prompt / 邮件 / commit / 总结等），而不是逐字转写
- **不是 ASR 后处理**：和"标点修复"这种小动作不同——目标是把口语化、跳跃、自我修正的原始语音重写成结构清晰、可直接发送/粘贴的文本
- **架构隔离**：AI 改写独立成 `voice-rewrite` crate，挂在 ASR 输出和粘贴之间；关闭改写时链路完全透明
- **多端 adapter 余地**：核心链路保持平台无关，Windows 桌面、未来手机端或 Web 插件只替换输入/输出/配置 adapter

具体设计（改写档定义、prompt 模板、流式/非流式策略、错误兜底等）见 §十三 草稿。

**Demo 视频亮点**：
1. Windows 桌面里实时输入：在记事本 / VS Code 中按住快捷键说话，松开后自动粘贴
2. 端侧 vs 云端引擎切换：同一句话识别准确率对比
3. AI 改写演示：一段口语化、有自我修正的原始语音，分别在"关闭改写"和"改写为 prompt"两档下的输出差异

## 八、风险与应对

| 风险 | 应对 |
|---|---|
| sherpa-onnx 本地构建失败 | Step 3 关键卡点，优先使用本地静态库 archive；仍失败再降级到子进程调用 sherpa-onnx CLI |
| Windows 全局快捷键 / 粘贴行为与 Linux 不一致 | 不用 Linux 桌面结果推断 Windows；Step 5 起以 Windows 原生手测和修复为准 |
| Linux Tauri / WebKit / DBus 等系统依赖拖慢进度 | Linux 不安装或调试桌面外壳依赖；桌面壳在 Windows 环境开发和验证 |
| 端侧首字延迟过高 | INT8 量化模型 + 调小 chunk size；中端机实测后定型 |
| Tauri 学习曲线 | Windows-first 悬浮窗极简，逻辑全在 Rust，前端只用静态 HTML+少 JS |
| 手机端 / Web 插件路线不确定 | 不提前引入运行时；先把 core 状态事件、配置和文本输出抽象清楚，后续再做 adapter |
| 模型/数据体积 | gitignore，README 写下载脚本 |
| 云端 API key 泄漏（Step 8/10 启动后） | `.env` 文件 + .gitignore；桌面端 key 用系统 keyring 存储，不写明文 TOML |
| AI 改写延迟 / 失败影响实时体验 | 改写默认关闭，开启时也保留"原文兜底"——LLM 超时或失败时粘贴原始 ASR 文本，不阻塞 |

## 九、删减线（进度落后时按序砍）

1. 砍 Step 10（AI 改写）→ 创新点退化为引擎对比，作为遗憾说明
2. 砍 Step 9（引擎相关设置 UI）→ 引擎选择只保留 TOML / CLI flag
3. 砍 Step 8（云端 ASR）→ 端侧已经够用，作为遗憾说明
4. 砍 Step 7 设置面板 → 退化为 TOML 配置文件
5. 砍 Step 6 桌面外壳 → 退回 CLI + 日志
6. **底线**：Step 1-5 必须完成，构成实时输入的"快捷键 → 录音 → 端侧 ASR → 剪贴板 → 粘贴"最小可演示链，并完成 Windows 至少一次人工验收

## 十、启动时第一批操作

1. `git init` 已就绪 → 配置 `.gitignore` + Apache-2.0 LICENSE
2. 写 README 骨架（项目介绍、技术栈、依赖列表、构建说明占位）
3. 创建 Cargo workspace + 4 crate 空壳
4. `git remote add origin <用户提供的URL>` + 首次 push

每一步对应 Step 1 中的细粒度 PR，按 1.1 → 1.7 顺序提交。

## 十一、待确认事项

1. **GitHub 仓库 URL**：用户已建仓，需提供 URL（用于 `git remote add`）
2. **Windows 手测环境**：需要可运行 Windows 桌面的机器，提前准备 Rust toolchain、sherpa-onnx 静态库 archive、模型目录；外网下载由用户手动提供文件
3. **云端 ASR 厂商**：Step 8 之前确定。备选**阿里云 DashScope（Paraformer-realtime-v2）**——免费额度大、与端侧 Zipformer 同源、文档清楚。当前 Step 1-7 不阻塞
6. **AI 改写设计**：见 §十三 草稿，等用户敲定改写档清单、目标 LLM、prompt 形态后才能拆 Step 10 的 PR
4. **Windows 开发切换时机**：Step 5 实机验收后，桌面外壳和设置面板优先直接在 Windows 上继续开发；Linux 保留为 core / CLI 纯逻辑验证环境
5. **手机端 / Web 插件形态**：只作为后续探索方向。当前先保证 core 事件、配置和输出接口不和 Windows 桌面强耦合

## 十二、测试策略

跨平台音频项目按硬件依赖分四层，避免所有测试都需要真设备：

1. **无依赖纯逻辑层**（`cargo test`，三平台 + 任意 CI 都跑）
   - WAV 编解码、样本格式转换（f32/u16↔i16）、配置解析、后续 AI 改写的 prompt 构造与 mock LLM 测试
   - 当前覆盖：`voice-core` 18 个单元测试

2. **Mock / 文件回放后端**（同样跑在所有平台）
   - `FileCapture`（已落地，PR 2.6）从 WAV 按真实节奏喂数据，验证"录音→WAV→ASR→粘贴"整条管道
   - voice-cli 通过 `--input` 切换，绕开真硬件；这层覆盖 80% 业务逻辑

3. **平台编译矩阵**（GitHub Actions，分层执行）
   - Linux runner 只跑 core / ASR / CLI 的 `cargo build` + `cargo test` + 文件回放测试，不构建 Tauri 桌面壳
   - Windows runner 跑 core / ASR / CLI 编译，并在后续加入桌面壳构建检查
   - macOS 不作为近期目标；如果后续要支持，再单独补平台验证 PR
   - **未落地**，待 CI 配置 PR

4. **手动硬件验收**（demo 视频 + checklist）
   - Windows 优先：真麦克风、真快捷键、真剪贴板、真粘贴 —— Step 5 必须先落地
   - Windows Tauri 悬浮窗、引擎切换、AI 改写档切换后续补测
   - 写在 `docs/test-checklist.md`（Step 5 先引入 Windows 版，Step 11 完整化）

---

## 十三、AI 改写设计草稿（待用户细化）

**目标**：参考 Wispr Flow / Superwhisper，把 ASR 输出的原始口语文本，按用户选择的"档位"重写成可直接使用的成品文本。这是 Step 10 的产品形态草稿，PR 拆分等设计敲定后再做。

### 13.1 概念

- **改写档（Profile）**：一组预置的改写策略，决定 LLM 怎么处理这段文本。例：
  - `off` — 关闭改写，原样输出（等同 Step 5 行为）
  - `clean` — 只去口语化、自我修正、嗯啊词，保留原意和语序
  - `prompt` — 重写成一段结构清晰、给 AI agent 用的 prompt
  - `email` — 重写成正式邮件正文
  - `commit` — 重写成 Conventional Commit 风格的短句
  - 后续可加 `meeting-notes` / `idea-dump` 等
- **改写管道（RewritePipeline）**：trait，输入原文 + 当前 Profile，输出改写后文本。和 `AsrEngine` 一样 dyn-safe，方便切换实现（identity / LLM / 未来端侧小模型）。

### 13.2 调用时机与链路

```
hotkey → 录音 → ASR → rewrite(profile) → clipboard → paste
                          ↑
                可选；profile=off 时直通
```

关键约束：
- **改写默认关闭**。第一次跑就有 LLM 调用对新用户不友好，且会暴露需要 API key
- **改写失败必须 fallback 到原文**，不能因为 LLM 超时就什么都不粘贴
- **改写期间桌面状态 = `Rewriting`**（新加一个 `RealtimeState`，区别于 `Transcribing`）

### 13.3 LLM 选型考虑（待用户决定）

候选维度：
- **云端通用 LLM**：Claude / GPT / Gemini / DashScope Qwen 等。优势是质量高，劣势是延迟 + 费用
- **国内云端轻量模型**：Qwen-turbo / 豆包 / Kimi 等。延迟低、便宜，中文场景够用
- **端侧小模型**：llama.cpp + 量化 Qwen2.5-3B 之类。完全离线，但首字延迟在中端机上可能不可接受

**初步建议**：先接一个云端通用 LLM 走通形态，把"端侧小模型"作为后续优化路线。

### 13.4 Prompt 设计原则

- 每个 Profile 一份独立 system prompt，放在 `voice-rewrite/prompts/` 下，纯文本文件方便迭代
- 用户原文以单独 user turn 传入，**不和 system 拼接**，避免 prompt injection
- 输出**只要文本本体**，禁止解释、禁止 markdown 包裹、禁止"以下是改写后的：…"前缀。system prompt 里要明确约束
- 留一个 `custom` Profile 让高级用户写自己的 system prompt（持久化到配置）

### 13.5 配置形态

`%APPDATA%\voice-flow\app.toml` 扩展：

```toml
model_dir = "..."

[hotkey]
ctrl = true
alt = true
key = "Space"

[asr]
engine = "local"  # or "cloud"

[rewrite]
enabled = false
default_profile = "clean"
api_key_ref = "system-keyring"  # 实际 key 不写明文

[rewrite.profiles.custom]
system_prompt = "..."
```

### 13.6 桌面 UI 增量

- 悬浮窗加一个"改写档"小标签，显示当前 Profile
- 设置面板加：开关、默认 Profile 下拉、API key 录入、自定义 prompt 编辑器
- 状态机加 `Rewriting`，前端显示成"改写中..."

### 13.7 待用户决定的设计点

1. **改写档的具体清单**：上面列的 6 个够不够，要不要砍掉哪个，要不要加"翻译"类
2. **改写是否可热切档**：录音前选 vs 录音后再决定（后者意味着粘贴会延迟到用户选完）
3. **LLM 提供商**：先接哪家、是否复用 Step 8 的云端 ASR 厂商账号
4. **端侧改写路线**：是否预留 `voice-rewrite-local` crate，还是先不考虑
5. **自定义 Profile**：是否提供 UI 编辑器，还是只能改 TOML
