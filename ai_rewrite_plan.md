# AI 改写层执行计划（voice-flow 增强）

> 配套：[plan.md](./plan.md) §十三 是这部分功能的占位草稿，本文件是落地版。完成后回写 plan.md 的 §13.7 待决项即可，**不要把这里的内容塞回 plan.md**。
>
> 参考：[wispr_flow_analysis.md](./wispr_flow_analysis.md)（产品分析）、[hackathon_ai_voice_input_plan.md](./hackathon_ai_voice_input_plan.md)（功能映射）。

---

## 1. 目标和不做什么

### 1.1 要做的

在现有 `按住说话 → ASR → 粘贴` 链路上挂一层**可选**的 AI 改写，让"自然口语"自动变成"可直接发送的成品文本"。

具体：

- **本地预处理**：去口头禅、应用用户词典、识别语音命令（regex 层，0 网络）
- **LLM 改写**：调用通用大模型，按"改写档"输出邮件/微信/prompt/commit 等
- **多版本一次出**：一次 LLM 调用通过结构化输出拿到 N 个候选，UI 切换
- **原文兜底**：LLM 失败 / 超时立刻退到预处理后的原文，不阻塞粘贴

### 1.2 明确不做的

三天黑客松，下面这些**全部不碰**：

| 不做 | 原因 |
|---|---|
| 微调任何模型 | 三天来不及，且效果不可控 |
| 训练 personalized ASR / personalized LLM | 同上 |
| 真正的 context-aware（读当前 App 输入框） | 需要 accessibility / UIA / atspi，三天踩不完 |
| 端侧 LLM（llama.cpp 跑 Qwen3B 之类） | 首字延迟过高，且模型权重让仓库膨胀 |
| 流式 token 渲染到光标 | 现有架构是"松开→一次性粘贴"，破坏现有 UX 不值得 |
| 自定义改写档 UI 编辑器 | 留给 TOML，写 system_prompt 字符串足够 |

### 1.3 不破坏的基线

- **默认 `rewrite.enabled = false`**：不开 AI 改写时链路完全等价于现在的 Step 5 行为
- **原文兜底**：开了 AI 改写，LLM 失败 / 超时（>3s）也必须粘贴预处理后的原文
- **桌面状态机不漏态**：新增 `Rewriting` 状态，UI 必须能渲染

---

## 2. LLM 选型

### 2.1 结论先行

**默认顶配跑通，再调优**：v1 默认 **Claude Opus 4.7** 走 Anthropic 兼容 OpenAI 端点；同时支持 OpenAI **GPT 顶配模型**；国内网络兜底用 DashScope **Qwen 顶配模型**（与 Step 8 复用同一个 API key 账户体系）。

| 角色 | 默认模型 | Provider |
|---|---|---|
| Demo / 验收主用 | `claude-opus-4-7` | Anthropic（OpenAI 兼容） |
| 备选（更快、英文偏好场景） | `gpt-5.5`（占位，按发布版调整） | OpenAI |
| 国内网络兜底 | `qwen-max`（顶配 Qwen） | DashScope（OpenAI 兼容） |

> **黑客松策略：先用最强的模型把质量天花板打出来，让 demo 改写效果最好看；后续 PR 再加"省钱档"切到 `claude-haiku-4-5` / `qwen-plus` / `qwen-flash`，作为延迟和成本优化点。**

### 2.2 统一抽象：一个 trait 罩住所有 provider

所有 provider 都通过同一个 `LlmClient` trait 调用：

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, LlmError>;
}

pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub user: String,
    pub response_format: ResponseFormat, // Text | JsonObject
    pub timeout: Duration,
}
```

Anthropic / OpenAI / DashScope 都有 OpenAI 兼容的 chat completions 端点，**统一用一份 reqwest 实现 `OpenAiCompatClient`**，只是 `base_url` 和 `model` 不同。这样换 provider = 改配置，**不改代码**。

| Provider | Base URL | 备注 |
|---|---|---|
| Anthropic | `https://api.anthropic.com/v1`（OpenAI 兼容） | header `x-api-key`，需要 `anthropic-version` |
| OpenAI | `https://api.openai.com/v1` | header `Authorization: Bearer` |
| DashScope | `https://dashscope.aliyuncs.com/compatible-mode/v1` | header `Authorization: Bearer` |

> **DashScope 的 OpenAI 兼容端点和 Step 8 用的 paraformer-realtime WebSocket 端点不是同一个。** 同一个 API key 可以复用，但 **client 类型完全分开**——Step 8 的 `voice-asr-cloud::DashScopeClient` 不要碰、不要复用。`voice-rewrite` 自己一套 HTTP client。

### 2.3 默认与切换

- **配置默认 `provider = "anthropic"`，`model = "claude-opus-4-7"`**
- 用户在设置面板可切到 `openai` 或 `dashscope`，模型名是文本框
- 设置面板录入对应 provider 的 API key（每个 provider 单独存）
- 如果当前 provider 的 key 未配置，桌面端给一个非阻塞提示，pipeline 自动兜底 `off`

### 2.4 价格 / 延迟参考（v1 不优化，仅记录）

| 模型 | 输入 | 输出 | 首 token 经验值 |
|---|---|---|---|
| `claude-opus-4-7` | ~$15/M | ~$75/M | ~500~1000ms |
| `gpt-5.5`（占位） | 按官方 | 按官方 | ~500~1000ms |
| `qwen-max` | ~¥40/M | ~¥120/M | ~400~800ms |

一段 200 字中文原文，input 大约 400 token、output 大约 400 token：

- Opus：每次 ~¥0.4 RMB
- Qwen-max：每次 ~¥0.05 RMB

**黑客松 demo 调用次数 ~50 次，总成本 < ¥30**，可以接受。后续做"省钱档"切到 haiku/flash 是 §10 删减线之外的优化项。

---

## 3. 数据流与模块边界

**核心原则：AI 改写是"文字→文字"，不接触音频。** 它和 ASR 是顺序流水线上的两个独立阶段，代码上严格分到不同 crate、不同 trait、不同测试集，互不知道对方的存在。

### 3.1 全链路数据流（按域分两段）

```
┌─────────────────────────── 音频域 (audio domain) ────────────────────────────┐
│                                                                              │
│   [麦克风] → cpal_backend → AudioCapture trait → push_to_talk recorder      │
│                                                              │               │
│                                                              ▼               │
│                                              Vec<i16>  (16kHz mono PCM)      │
│                                                              │               │
│                                                              ▼               │
│   AsrEngine::transcribe(pcm)  ←─ voice-asr-local  or  voice-asr-cloud       │
│                                                              │               │
└──────────────────────────────────────────────────────────────┼───────────────┘
                                                               │
                                                  String  (原始 ASR 文本)
                                                               │
┌──────────────────────────── 文本域 (text domain) ─────────────┼──────────────┐
│                                                              ▼               │
│   voice-rewrite::Preprocessor::run(text)                                     │
│       ├─ filler 移除（正则）                                                  │
│       ├─ 用户词典替换（HashMap）                                              │
│       └─ 语音命令识别 → DetectedCommand?                                      │
│                          │                                                   │
│                          ▼                                                   │
│              (cleaned_text, command)                                         │
│                          │                                                   │
│                          ▼                                                   │
│   voice-rewrite::Pipeline::route(profile_default, command) → Profile        │
│                          │                                                   │
│              ┌───────────┴───────────┐                                       │
│              ▼                       ▼                                       │
│      profile == off            profile != off                                │
│              │                       │                                       │
│              │                       ▼                                       │
│              │           LlmClient::complete(system_prompt, user_text)       │
│              │                       │                                       │
│              │                       ▼                                       │
│              │           String  or  HashMap<String,String> (multi)          │
│              │                       │                                       │
│              ▼                       ▼                                       │
│   voice-rewrite::PostProcess::validate(orig, rewritten)                      │
│       └─ 长度 / 数字 / 专有名词检查；不通过 → 原文兜底                          │
│                          │                                                   │
│                          ▼                                                   │
│                  RewriteResult { main: String, variants: HashMap }           │
└──────────────────────────┼───────────────────────────────────────────────────┘
                           │
              ┌────── 输出域 (output domain) ──────┐
              │            ▼                       │
              │   ClipboardWriter::write(main)     │
              │   PasteSimulator::paste()          │
              │   RealtimeStateEvent::Completed    │
              └────────────────────────────────────┘
```

### 3.2 域 → crate 映射

| 域 | 输入类型 | 输出类型 | crate | 是否依赖系统 |
|---|---|---|---|---|
| 音频域 | 麦克风 / WAV 文件 | `Vec<i16>` PCM | `voice-core` (capture) | 依赖 cpal（仅 Windows / macOS 验证） |
| ASR 域 | `Vec<i16>` PCM | `String` 原始转写 | `voice-asr-local` / `voice-asr-cloud` | local 依赖 sherpa-onnx；cloud 依赖网络 |
| **文本域（本计划新增）** | **`String` 原始转写** | **`RewriteResult`** | **`voice-rewrite`** | **仅依赖网络（LLM），不依赖音频栈** |
| 输出域 | `String` 主版本 | 系统剪贴板 / 模拟按键 | `voice-core` (clipboard, paste) | 依赖 arboard / enigo |

**关键含义**：

1. **`voice-rewrite` 不知道音频存在**。`Cargo.toml` 里不能依赖 `voice-asr-local` / `voice-asr-cloud` / cpal / sherpa-onnx。它只接受 `&str`，吐出 `String`。
2. **Linux 上 `voice-rewrite` 全部测试可跑**。不需要 Windows、不需要麦克风、不需要 sherpa-onnx 静态库。
3. **替换 ASR 引擎不影响改写**，替换 LLM provider 不影响 ASR。两条独立的技术线，独立演进。
4. **`voice-core` 是唯一的"胶水"**：它在 `push_to_talk` 流程里同时持有 `Box<dyn AsrEngine>` 和 `Box<dyn RewritePipeline>`，按顺序调用。改写为 None 时退化到现有 Step 5 行为。

### 3.3 trait 边界（代码骨架）

```rust
// voice-asr-local / voice-asr-cloud（已有）
pub trait AsrEngine: Send + Sync {
    fn transcribe(&self, pcm: &[i16], sample_rate: u32) -> Result<String, AsrError>;
}

// voice-rewrite（新增）—— 文字进，文字出
#[async_trait]
pub trait RewritePipeline: Send + Sync {
    async fn process(&self, text: &str, ctx: RewriteContext)
        -> Result<RewriteResult, RewriteError>;
}

pub struct RewriteContext {
    pub default_profile: Profile,
    pub user_dictionary: Arc<UserDictionary>,
    pub api_credentials: Arc<dyn CredentialProvider>,
}

pub struct RewriteResult {
    pub main: String,                       // 拿去粘贴
    pub variants: HashMap<String, String>,  // multi 档时有 4 个；其他档为空
    pub trace: RewriteTrace,                // 预处理 / 命令 / LLM 各段耗时 + 是否兜底
}
```

`RewriteTrace` 是后面 UI（§7 的 R3.4）和测试断言都要用的，建议从 R1.2 就埋好字段，后续 PR 只往里填东西。

### 3.4 voice-core 串联点

只在 **一个位置** 调改写：

```rust
// crates/voice-core/src/push_to_talk.rs（伪代码示意）
let pcm = recorder.stop()?;                                  // 音频域结束
let raw = asr_engine.transcribe(&pcm, 16_000)?;              // ASR 域结束
let final_text = match rewrite_pipeline {                    // 文本域开始
    Some(p) => p.process(&raw, ctx).await?.main,
    None => raw,
};
clipboard.write(&final_text)?;                               // 输出域
paste.simulate()?;
```

`rewrite_pipeline = None` 时整条链路退化到现在的 Step 5 行为，零开销。

### 3.5 测试视角下的边界

| 测试集 | 跑在哪 | 依赖 |
|---|---|---|
| `voice-rewrite` 单测 / 集成 | Linux / Windows 都行 | 仅 mock LLM，不需要任何模型 / 音频文件 |
| `voice-rewrite` + 真 LLM（可选） | 本地手动 | 需要 `ANTHROPIC_API_KEY` 等环境变量，CI 默认 skip |
| `voice-core::push_to_talk` 改写集成 | Linux 也能跑 | 用 `FileCapture` + `MockAsrEngine` + `MockRewritePipeline` |
| 完整链路 | 仅 Windows | 真麦克风 + 真 ASR + 真 LLM |

测试样例细节见 §12。

---

## 4. 架构落点

### 4.1 新增 crate

新建 `crates/voice-rewrite/`（plan.md §13 已经占位）：

```
voice-rewrite/
├── Cargo.toml            ← 不依赖 cpal / sherpa-onnx / voice-asr-* ，仅 reqwest / tokio / regex / serde
├── src/
│   ├── lib.rs
│   ├── pipeline.rs       ← RewritePipeline trait + 编排
│   ├── preprocess.rs     ← 本地预处理（filler / dictionary / command detect）
│   ├── profile.rs        ← Profile enum + 元数据
│   ├── llm/
│   │   ├── mod.rs        ← LlmClient trait + ChatRequest/Response/Error
│   │   ├── openai_compat.rs  ← 一份 OpenAI 兼容 HTTP 实现，复用给 Anthropic/OpenAI/DashScope
│   │   ├── anthropic.rs  ← 仅做 header 差异封装
│   │   ├── openai.rs     ← 仅做 header 差异封装
│   │   └── dashscope.rs  ← 仅做 base_url 差异封装
│   ├── prompts.rs        ← system prompt 常量（include_str! 进二进制，不读运行时文件）
│   ├── postprocess.rs    ← 长度 / 数字 / 专有名词 sanity check
│   ├── trace.rs          ← RewriteTrace（各段耗时、是否兜底、命令命中）
│   └── error.rs
└── tests/
    ├── preprocess_test.rs
    ├── pipeline_mock_test.rs
    └── integration.rs    ← 用 mock LLM 跑端到端（音频不参与）
```

放在 `Cargo.toml workspace.members` 第 5 个成员。**严禁**在 `[dependencies]` 出现 `voice-asr-local` / `voice-asr-cloud` / `cpal` / `sherpa-onnx`。

### 4.2 voice-core 改动

- `RealtimeState` 新增 `Rewriting`（在 `Transcribing` 和 `Completed` 之间）
- `engine.rs` 不动；改写不是 ASR engine 的一部分，**挂在 ASR 之后**
- 新增 `text_pipeline.rs`：把 `transcript: String` + `RewriteSettings` 喂给 `RewritePipeline`，返回最终文本
- 现有 push_to_talk 流程里，在 ASR 出文本后、写剪贴板前插入一次 `pipeline.process()`

### 4.3 桌面壳改动

- 设置面板新增"AI 改写"分组（开关、Provider、Model、Profile、API key、custom prompt）
- 悬浮窗状态加 `Rewriting`（前端字串："改写中..."）
- 悬浮窗加"最近文本"多版本切换标签（multi 档启用时显示）

### 4.4 配置形态

`%APPDATA%\voice-flow\app.toml` 扩展：

```toml
[rewrite]
enabled = false                          # 默认关；用户自己显式开
default_profile = "clean"                # 默认档
provider = "anthropic"                   # anthropic | openai | dashscope
model = "claude-opus-4-7"                # 默认顶配；可换 gpt-5.5 / qwen-max
timeout_ms = 5000                        # 顶配模型稍宽容，5s 兜底
api_key_ref = "system-keyring"           # 实际密钥进 keyring，不写明文

# 不同 provider 的 key 单独存（设置面板每个 provider 一个输入框）
# keyring 里的 key 名称约定：voice-flow:anthropic、voice-flow:openai、voice-flow:dashscope

[rewrite.user_dictionary]
# 用户专有名词 / 易错词替换。preprocess 在 LLM 之前 apply
# 左 = ASR 可能出的错，右 = 正确写法
"克劳德" = "Claude"
"我推" = "Vue"
"赛恩" = "Shen"

[rewrite.profiles.custom]
system_prompt = ""                       # 高级用户自己写
```

API key 不写 TOML 明文。Windows 用 `keyring` crate 走 Windows Credential Manager。

---

## 5. 改写档（Profile）清单

下面是 v1 落地的档位。`off` 之外的每档都对应一个 system prompt（§7 给出）。

**v1 阶段所有档共用配置里的同一个 `model`**（默认 `claude-opus-4-7`），不分快档/慢档。后续 PR 再加 `fast_model` / `quality_model` 路由，做"省钱档"优化。

| Profile | 用途 | 输出形态 |
|---|---|---|
| `off` | 关闭，原文直出（兜底链路） | 原文 |
| `clean` | 去口头禅 + 补标点 + 修语序，保留口语风格 | 单段 |
| `polish` | 中等强度润色，更书面但仍自然 | 单段 |
| `email` | 邮件正文，礼貌、有称呼和结尾 | 多段 |
| `wechat` | 微信聊天语气，自然 | 短句 |
| `prompt` | 给 AI agent 用的 prompt，结构化、明确 | 单段 |
| `commit` | Conventional Commit 风格 | 一行标题 + 可选 body |
| `bullets` | 提取要点 | bullet 列表 |
| `multi` | 一次出 4 个版本（clean / polish / wechat / bullets），结构化 JSON | JSON |
| `custom` | 用户自定义 system_prompt | 由用户控制 |

> **demo 主推 `multi`**：一次说话出 4 个版本，UI 左右切换。视觉冲击最强，编排价值最直观。

---

## 6. 编排管道（创新点）

### 6.1 完整流程

```
ASR 原始文本
  ↓
[1] 本地预处理 (preprocess.rs，纯 Rust，~5ms)
    - filler 移除：嗯/啊/那个/然后那个/就是说 等正则匹配剔除
    - 用户词典替换：apply [rewrite.user_dictionary]
    - 命令检测：识别"改成XX""更YY""翻译成ZZ"等模式
    - 输出 (text, Option<DetectedCommand>)
  ↓
[2] Profile 路由 (pipeline.rs)
    - 默认 Profile = 用户设置
    - 如果检测到语音命令，命令 > 用户设置（用户说"改正式一点"等同切换到 polish）
  ↓
[3] LLM 改写 (llm/*.rs)
    - Profile == off：跳过
    - Profile != multi：单次调用，单段 text 输出
    - Profile == multi：单次调用，response_format=json_object，4 字段
    - 超时 = timeout_ms，超时即原文兜底
  ↓
[4] 后处理 (postprocess.rs)
    - 校验数字、人名、专有名词没被吃掉
    - reject 条件：LLM 输出长度 < 原文 * 0.3 且原文 > 20 字；或原文里的纯数字 / 英文专有名词在输出里全部丢失
    - 兜底则用预处理后的文本
  ↓
[5] 剪贴板 + 粘贴（输出域）
    - 主版本（multi 档默认取 clean）立即粘贴
    - 多版本面板异步显示，用户可点击复制任一版本
```

### 6.2 三个值得喊出口号的编排细节

| 编排点 | 为什么是创新 |
|---|---|
| **本地预处理 + LLM 两段** | LLM 输入更短更干净，省 token、降延迟、保留专有名词。等价于"零训练个性化" |
| **语音命令在 ASR 流里识别，不靠 UI** | 用户说"改正式一点，写邮件"就能切档，不用切换 Profile 下拉 |
| **多版本一次 LLM 调用** | 用 JSON 结构化输出，1 次调用拿 4 个版本，比 4 次并行调用便宜 + 快 |

### 6.3 延迟 budget

目标（中文 200 字原文，默认顶配模型 `claude-opus-4-7`）：

| 阶段 | 预算 |
|---|---|
| ASR（已实现） | 本地 200~500ms |
| 预处理 | <10ms |
| LLM 首 token | 顶配 500~1000ms |
| LLM 完整生成 | 1500~3000ms |
| 后处理 | <5ms |
| 粘贴 | <50ms |
| **改写档关闭** | 与现在持平 |
| **clean / polish / wechat 等单段** | ASR + 2~3s |
| **multi（JSON 4 字段）** | ASR + 3~4s |

`timeout_ms` 默认 5000，超时全部兜底原文。后续切到 haiku/flash 后预算可以砍半。

---

## 7. Prompt 设计

### 7.1 通用规则

- 每个 Profile 一份 system prompt，**编译进二进制**（`include_str!`），不读运行时文件
- 用户原文作为单独 user message，不和 system 拼接（防 prompt injection）
- system prompt 末尾固定加："只输出改写后的文本本体。不要解释、不要 markdown 包裹、不要前缀。如果你无法改写，原样输出输入。"
- 不传任何对话历史 / 上下文，**每次都是 zero-shot**

### 7.2 示例：clean profile system prompt

```
你是一个把口语自动写成清晰文本的助手。

输入是一段语音转写的中文（可能混杂英文专有名词），可能有口头禅、自我修正、断句不清。

你要做的：
- 去掉"嗯""啊""那个""就是说"等口头禅
- 修正明显的语序问题
- 补充标点和断句
- 保留原意，不要发挥
- 保留所有人名、地名、英文专有名词、数字

不要做的：
- 不要扩写，不要润色风格
- 不要总结
- 不要改写成正式书面语

只输出改写后的文本本体。不要解释、不要 markdown 包裹、不要前缀。
```

### 7.3 示例：multi profile（结构化）

```
你是一个把口语转写成多种成品文本的助手。

输入是一段口语转写。请输出 4 个版本，并按以下 JSON 结构返回：

{
  "clean": "去口头禅、修语序、补标点，保留原口语风格",
  "polish": "中等强度润色，更书面但仍自然，长度大体相当",
  "wechat": "微信聊天语气，简短、口语",
  "bullets": "要点列表，每行以 - 开头"
}

约束：
- 保留所有人名、地名、英文专有名词、数字
- 不要编造原文没有的事实
- bullets 如果原文只有一件事，输出一条即可

只输出 JSON，不要任何其他内容。
```

### 7.4 语音命令映射

预处理层用正则识别（顺序匹配，命中即停止）：

| 用户说 | 触发 |
|---|---|
| "改正式一点" / "正式一点" / "更正式" | profile = polish |
| "改短一点" / "压缩一下" | profile = clean + 提示 LLM 缩短 |
| "写成邮件" / "改成邮件" | profile = email |
| "写成微信" / "聊天语气" | profile = wechat |
| "改成要点" / "列点" | profile = bullets |
| "翻译成英文" / "改成英文" | profile = clean + 切换语言 |
| "写成 commit" / "提交信息" | profile = commit |

命中后，命令本身从 user message 中**移除**（避免 LLM 把命令也当成内容）。

---

## 8. 三天 PR 拆分

风格对齐 [plan.md](./plan.md) §6（单一职责、≈300 行、Conventional Commit）。下面每一行是一个 PR。

### Day 1：跑通最小链路（rewrite=clean，默认 Anthropic Opus）

| PR | 标题 | 单一职责 |
|---|---|---|
| R1.1 | `chore(rewrite): scaffold voice-rewrite crate` | 空 crate + workspace 注册 + 错误类型；显式禁止依赖音频/ASR crate |
| R1.2 | `feat(rewrite): add RewritePipeline trait + types` | `RewritePipeline` / `RewriteContext` / `RewriteResult` / `RewriteTrace` |
| R1.3 | `feat(rewrite): identity pipeline + mock llm` | 直通实现 + `MockLlmClient`（测试用），并写头一批单测 |
| R1.4 | `feat(rewrite): add filler removal preprocess` | 正则去口头禅 + 单测（覆盖嗯/啊/那个/就是说/然后那个） |
| R1.5 | `feat(rewrite): add user dictionary substitution` | 词典替换 + 单测（覆盖优先级、最长匹配） |
| R1.6 | `feat(rewrite): openai-compat http client` | `OpenAiCompatClient`：base_url / api_key / headers 参数化 + retry + timeout |
| R1.7 | `feat(rewrite): anthropic provider impl` | 在 R1.6 基础上加 Anthropic headers，默认 model `claude-opus-4-7` |
| R1.8 | `feat(rewrite): llm pipeline with clean profile` | clean system prompt + 超时兜底 + 集成测试（mock LLM） |
| R1.9 | `feat(core): add Rewriting realtime state` | 状态枚举 + 事件 |
| R1.10 | `feat(core): wire rewrite into push-to-talk` | ASR 后插入 pipeline，默认 off；改写为 None 时零开销 |
| R1.11 | `feat(cli): transcribe --rewrite=clean flag` | CLI 验证；从 env `ANTHROPIC_API_KEY` 读 key |

**Day 1 验收**：
- `ANTHROPIC_API_KEY=... voice-cli transcribe x.wav --rewrite=clean` 输出去口头禅 + 补标点的版本
- Linux 上 mock LLM 单测全过（**不需要联网**）
- 桌面端**不接 UI**，但状态机已经能切到 Rewriting（hidden in 设置）

### Day 2：做差异化（多 Profile + 命令 + 多 Provider + 桌面 UI）

| PR | 标题 | 单一职责 |
|---|---|---|
| R2.1 | `feat(rewrite): add email/wechat/prompt/commit/bullets profiles` | 五个 system prompt + Profile 路由 |
| R2.2 | `feat(rewrite): add polish profile` | 中度润色 prompt |
| R2.3 | `feat(rewrite): voice command detection in preprocess` | 7 个命令正则 + 命令移除 + 单测 |
| R2.4 | `feat(rewrite): multi-version profile with json output` | `response_format=json_object` + 4 字段 schema 解析 |
| R2.5 | `feat(rewrite): openai provider impl` | 复用 R1.6 client，加 OpenAI headers，默认 model `gpt-5.5` |
| R2.6 | `feat(rewrite): dashscope provider impl` | 复用 R1.6 client，base_url 切 `compatible-mode/v1`，默认 `qwen-max` |
| R2.7 | `feat(core): config schema for rewrite section` | `app.toml [rewrite]` 读写 + 单测 |
| R2.8 | `feat(desktop): rewrite settings panel` | 开关 + Provider 下拉 + Model 输入 + Profile 下拉 + 各 provider 的 API key 输入 |
| R2.9 | `feat(desktop): secure api key with keyring` | Windows Credential Manager 存储；每个 provider 一个 entry |
| R2.10 | `feat(desktop): show current profile in floating window` | 悬浮窗显示当前档 |
| R2.11 | `feat(desktop): render Rewriting state` | "改写中..." UI |
| R2.12 | `feat(desktop): multi-version variant tabs` | multi 档启用时，悬浮窗显示 4 个标签可切换复制 |

**Day 2 验收**：
- Windows 上：选 email 档说话 → 松开 → 粘贴出邮件正文
- 说"改正式一点，下午晚到十分钟" → 自动切到 polish 档输出正式句
- 选 multi 档 → 悬浮窗 4 个标签都填好，点击任一可复制对应版本
- 设置面板切换 Anthropic / OpenAI / DashScope 三个 provider，至少 Anthropic + DashScope 各成功跑一次（网络条件允许的话顺带跑 OpenAI）

### Day 3：包装、demo、收尾

| PR | 标题 | 单一职责 |
|---|---|---|
| R3.1 | `feat(rewrite): post-process length sanity check` | 异常压缩兜底原文 |
| R3.2 | `feat(rewrite): preserve numbers and proper nouns guard` | 数字 / 英文专有名词丢失检测 |
| R3.3 | `feat(desktop): latency breakdown display` | UI 显示 ASR / 改写 / 粘贴各段耗时（读 `RewriteTrace`） |
| R3.4 | `feat(desktop): rewrite pipeline trace overlay` | 悬浮窗显示"预处理→命令→改写"流程链 |
| R3.5 | `feat(core): hot reload rewrite settings` | 设置改完不用重启就生效（复用现有 restart_requested） |
| R3.6 | `feat(desktop): custom profile system prompt editor` | 设置里多行文本框 |
| R3.7 | `docs: ai rewrite usage guide` | README 加章节 + 截图 |
| R3.8 | `docs: demo script for ai rewrite` | `docs/demo-rewrite.md` 演示步骤 |
| R3.9 | `chore: fixtures for ai rewrite demo` | 一段固定 WAV + 期望输出，让评委可复现 |

**Day 3 验收**：
- Demo 脚本走通（§10）
- 改设置不重启桌面即生效
- README 和 demo 文档完整

### 节奏与删减

如果 Day 1 跑不完到 R1.11，**优先保证 R1.6~R1.10 落地**（拿到 clean 档基础链路 + 串到 push_to_talk），R1.3~R1.5 可以放到 Day 2 头一两个 PR。

---

## 9. 配置 / 密钥 / 隐私

### 9.1 API key 存储

- **不**写 TOML 明文
- Windows：`keyring` crate（Rust 生态标准），底层 Windows Credential Manager
- TOML 里只存 `api_key_ref = "system-keyring"`
- keyring 里**每个 provider 单独一个 entry**：`voice-flow:anthropic` / `voice-flow:openai` / `voice-flow:dashscope`
- 设置面板：选中 provider 后显示对应 key 输入框；保存后立即写 keyring，输入框显示 `********`
- 删除 = "清除"按钮 → keyring delete 对应 entry

### 9.2 数据出境提示

- **改写默认关**。开启时设置面板必须有一行说明（按当前 provider 动态替换）：
  > 开启 AI 改写后，识别文本会发送到 {Anthropic | OpenAI | DashScope} 服务器进行处理。不要在涉密场景下开启。
- 关闭后，链路完全不联网（除非 ASR engine 选了 cloud）

### 9.3 日志

- LLM 调用入参 / 出参**不**写日志默认（仅 trace 级开关）
- 错误日志只记录错误类型 + 状态码，不记录 prompt / 转写文本

---

## 10. Demo 脚本

固定 WAV 文件 `docs/fixtures/demo-rewrite.wav`，内容：

> 嗯，我要写个邮件，就是跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我，那个语气正式一点。

### 演示顺序

1. **基础对比**：rewrite=off → 粘贴原始转写
2. **clean 档**：粘贴去口头禅 + 补标点的版本
3. **email 档**：粘贴正式邮件
4. **multi 档**：悬浮窗显示 4 个版本，逐个点击展示
5. **语音命令**：用户口语里"语气正式一点"被识别，自动切到 polish
6. **Provider 切换（可选）**：同一句话用 Anthropic Opus vs DashScope Qwen-max 对比

### 关键展示话术

> 我们没有训练任何模型，也没用 RAG。我们做的是 AI 编排层：
>
> 一段口语先经过本地预处理——去口头禅、应用用户词典——这是零训练的"个性化"；
> 然后正则在原文里识别用户的语音命令，自动选择改写档；
> 最后一次 LLM 调用通过结构化 JSON 输出拿到多个版本，比并发调用便宜也更快。
>
> ASR 负责听清楚，LLM 负责改写，编排层把它们串成一个延迟稳定的产品。

---

## 11. 删减线（进度落后按序砍）

| 序号 | 砍掉 | 影响 |
|---|---|---|
| 1 | R3.6 custom profile 编辑器 | 编辑 TOML 即可 |
| 2 | R3.4 trace overlay / R3.3 延迟显示 | demo 解说时口播 |
| 3 | R3.2 数字 / 专有名词保护 | 不致命，但 demo 时小心选词 |
| 4 | R2.12 multi-version 多标签 UI | 退化成 multi 档输出 JSON 到悬浮窗"最近文本"区，能看就行 |
| 5 | R2.9 keyring | 退化成 TOML 明文 + 警告文字 |
| 6 | R2.5 / R2.6 OpenAI / DashScope provider | 退化成只有 Anthropic 一家 |
| 7 | R2.3 语音命令识别 | 退化成仅靠 Profile 下拉 |
| 8 | R2.4 multi 档 | 退化成只有 clean / polish / email 三档 |
| 9 | **底线**：Day 1（R1.1~R1.11）必须完成 | clean 档单条链路跑通 = 创新点已立住，剩下都是加分 |

---

## 12. 测试策略与测试样例

### 12.1 分层

| 层 | 跑在哪 | 覆盖 | 工具 |
|---|---|---|---|
| L1 单测：preprocess | Linux / Windows / CI | filler / 词典 / 命令检测 | `cargo test -p voice-rewrite` |
| L2 单测：postprocess | 同上 | 长度 / 数字 / 专有名词检查 | 同上 |
| L3 集成：pipeline + MockLlmClient | 同上 | 完整文本域流程，不联网 | `cargo test -p voice-rewrite` |
| L4 集成：voice-core 串联 | 同上 | `MockAsrEngine` + `MockRewritePipeline` 串 push_to_talk | `cargo test -p voice-core` |
| L5 真 LLM 烟雾测试 | 本地手动 | 单档 + multi 档真实调用一次 | `cargo test -p voice-rewrite --features live-llm -- --ignored` |
| L6 端到端 | 仅 Windows | 麦克风→ASR→改写→粘贴 | 人工 |

L1~L4 **必须**在 CI 跑（一旦加 CI）。L5 默认 ignore，避免 CI 调用真实 API 烧钱。L6 是 demo 验收。

### 12.2 Mock LLM 设计

```rust
// crates/voice-rewrite/src/llm/mock.rs（test-only，feature = "test-util"）
pub struct MockLlmClient {
    // 按 (model, system, user) 元组返回固定响应
    responses: HashMap<MockKey, MockResponse>,
    // 或：按 user message 前缀匹配
    pattern_responses: Vec<(Regex, MockResponse)>,
    // 模拟超时 / 错误
    behavior: MockBehavior,
}

pub enum MockBehavior {
    Ok,
    Timeout(Duration),
    HttpError(u16),
    InvalidJson,        // multi 档场景
}
```

测试通过 `MockLlmClient::with_response(profile, input, output)` 注入预期，断言 pipeline 行为。

### 12.3 具体测试样例（按 PR 对应）

> 每个样例都是"输入 → 期望"。LLM 部分用 mock，不需要真 key。

#### R1.4 filler 移除

| # | 原文 | 期望 |
|---|---|---|
| F1 | `嗯我今天有点忙` | `我今天有点忙` |
| F2 | `那个就是说我们明天再聊` | `我们明天再聊` |
| F3 | `啊啊啊我想想啊` | `我想想` |
| F4 | `这个那个 Claude 还挺好用` | `Claude 还挺好用`（专有名词保留） |
| F5 | `我嗯我想说`（两次 filler 夹真实内容） | `我我想说` |
| F6 | `嗯`（整句全 filler） | 空字符串，触发 caller 跳过 LLM |
| F7 | `没有口头禅的句子` | 原样返回 |
| F8 | `嗯，那么我们开始吧` | `我们开始吧`（保留标点处的逗号） |
| F9 | 100 字长句含 3 个 filler | 全部去除 |
| F10 | 英文 filler `um well so I think` | 不处理（v1 只覆盖中文） |

#### R1.5 用户词典替换

| # | 词典 | 原文 | 期望 |
|---|---|---|---|
| D1 | `克劳德→Claude` | `让克劳德帮我写` | `让 Claude 帮我写` |
| D2 | 多条 + 最长匹配 | `我推下一推荐` + `我推→Vue`、`我→I` | `Vue 下一 I 荐`（最长优先） |
| D3 | 空词典 | 任意 | 原样 |
| D4 | 词典命中但跨标点 | `克劳德。克劳德` | 两个都替换 |
| D5 | 大小写敏感（英文） | `claude→Claude` | 仅匹配小写 |
| D6 | 词典空 key | 配置 `""=X` | 应当被忽略，不 panic |

#### R2.3 语音命令检测

| # | 原文 | 期望 detected_command, 期望 cleaned |
|---|---|---|
| C1 | `下午晚到十分钟，改正式一点` | `Polish`, `下午晚到十分钟，` |
| C2 | `写成邮件，下午晚到十分钟` | `Email`, `下午晚到十分钟` |
| C3 | `下午晚到，写成微信` | `Wechat`, `下午晚到` |
| C4 | `改成要点：A B C` | `Bullets`, `A B C` |
| C5 | `没有命令的普通句子` | `None`, 原文 |
| C6 | 命令出现两次（取首个匹配） | 首个，cleaned 移除首个 |
| C7 | 命令在句中（不在首尾） | 命中，移除该子串 |
| C8 | 大小写 / 半角全角混排 `改正式 一点` | 命中（容忍空白） |

#### R1.8 / R2.x pipeline 集成（mock LLM）

| # | 配置 | mock LLM 返回 | 期望 main / variants / fallback |
|---|---|---|---|
| P1 | profile=clean | `"我今天下午晚到十分钟。"` | main = LLM 输出，variants 空，fallback=false |
| P2 | profile=clean，LLM 超时 5s | hang | main = 预处理后的原文，fallback=true |
| P3 | profile=clean，LLM 503 | error | main = 预处理后原文，fallback=true |
| P4 | profile=multi | `{"clean":..., "polish":..., "wechat":..., "bullets":...}` | main = clean 字段，variants = 4 字段全填 |
| P5 | profile=multi，LLM 返回非 JSON | `"对不起..."` | main = 预处理后原文，fallback=true |
| P6 | profile=multi，JSON 缺字段 | `{"clean":"x"}` | 已有字段填入 variants，缺失字段填空字符串，main=clean，fallback=false |
| P7 | profile=off | 不调用 LLM | main = 预处理后原文，LlmClient 调用次数 = 0 |
| P8 | profile=clean，用户原文 < 5 字 | 不调用 LLM | main = 预处理后原文（短文本跳过 LLM 节省时间） |
| P9 | profile=clean，mock 返回空字符串 | `""` | fallback 到原文 |
| P10 | profile=clean，mock 返回前缀污染 `这是改写后的：xxx` | LLM 输出 | postprocess 不剥前缀（v1），但 trace 标注 |

#### R3.1 postprocess 长度检查

| # | 原文长度 | LLM 输出长度 | 期望 |
|---|---|---|---|
| L1 | 50 | 45 | 通过 |
| L2 | 50 | 10 | reject，兜底原文（< 0.3 × 50 且原文 > 20） |
| L3 | 10 | 2 | 通过（原文 ≤ 20，不触发检查） |
| L4 | 50 | 200 | 通过（v1 只查过短，不查过长） |

#### R3.2 数字 / 专有名词保护

| # | 原文 | LLM 输出 | 期望 |
|---|---|---|---|
| N1 | `下午 3 点开会` | `下午开会` | reject，"3" 丢失 |
| N2 | `Claude 比 GPT 好用` | `它比那个好用` | reject，两个英文专有名词丢失 |
| N3 | `下午 3 点开会` | `下午三点开会` | 通过（数字以汉字形式保留） |
| N4 | `下午 3 点开会` | `下午 15:00 开会` | 通过（数字 3 出现在 15 里，子串包含；v1 简单实现） |

#### L4 voice-core 集成

| # | 测试 | 期望 |
|---|---|---|
| I1 | MockAsr 出文本 → MockRewrite=identity | clipboard 内容 = ASR 文本 |
| I2 | MockAsr → MockRewrite 永远 fallback | clipboard 内容 = MockAsr 文本 |
| I3 | MockAsr → MockRewrite 改写成 X | clipboard 内容 = X |
| I4 | rewrite_pipeline = None | 零 LLM 调用，链路等价 Step 5 |
| I5 | 状态事件序列 | `Idle → Recording → Transcribing → Rewriting → Completed`，Rewriting 仅在 rewrite enabled 时出现 |

#### L5 真 LLM 烟雾（用 `#[ignore]` 标记）

需要 `ANTHROPIC_API_KEY` 等环境变量；CI 不跑：

| # | profile | 原文 | 验证 |
|---|---|---|---|
| S1 | clean | `嗯我今天下午可能会晚到十分钟` | 输出非空、不含"嗯"、含"十分钟" |
| S2 | multi | 同上 | 解析 JSON 成功、4 字段都非空 |
| S3 | clean，timeout=1ms | 同上 | fallback=true |
| S4 | clean，key 错误 | 任意 | LlmError::Auth，fallback=true |

### 12.4 测试 fixtures

- `crates/voice-rewrite/tests/fixtures/preprocess/`：每个样例一个 `.in.txt` + `.expected.txt`
- `crates/voice-rewrite/tests/fixtures/multi_json/`：几个 LLM 返回的 JSON 样本（正确 / 缺字段 / 非法）
- `docs/fixtures/demo-rewrite.wav`：demo 用的固定音频（Day 3 引入）

### 12.5 跑测命令一览

```bash
# Linux / Windows 都能跑
cargo test -p voice-rewrite                 # 全部本地测试（mock LLM）
cargo test -p voice-rewrite --features live-llm -- --ignored \
    --test-threads=1                        # 真 LLM 烟雾测试
cargo test -p voice-core text_pipeline      # voice-core 串联集成

# 一键全部（不含真 LLM）
cargo test --workspace
```

---

## 13. 剩余决策点（启动前需用户确认）

下面这些**会影响代码**，启动 Day 1 前需要确认：

| # | 决策 | 我的建议（你可以否） |
|---|---|---|
| 1 | **是否真把默认 provider 设为 Anthropic Opus** | 是。如果你网络访问 Anthropic 不稳定，改成 DashScope `qwen-max` 当默认，Opus 留作可切换选项 |
| 2 | **API key 注入方式** | Day 1 通过环境变量（`ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `DASHSCOPE_API_KEY`）；Day 2 引入 keyring 持久化。Day 1 不写 keyring |
| 3 | **OpenAI 模型名占位 `gpt-5.5`** | Day 2 实际配上时再确认当前发布版（GPT-5 / o4 / 其他）；改一行默认配置即可 |
| 4 | **Anthropic OpenAI 兼容端点 vs 原生 `/v1/messages`** | 先试 OpenAI 兼容路径 `/v1/chat/completions`（共用 client）；如果 multi 档的 `response_format` 不被兼容，单独退到 Anthropic 原生 messages API（多写 50 行不复用 OpenAiCompatClient） |
| 5 | **是否提供 `voice-cli rewrite` 子命令**（纯文字→文字，用于 demo / 调试） | 强烈推荐做。一行字进、一行字出，不需要音频，调试改写最快。R1.11 顺手做了 |
| 6 | **是否要求 ASR engine ≠ off 才能开改写** | 不要求。`voice-cli rewrite` 直接输入文本就能跑 |
| 7 | **改写失败是否提示用户** | 静默兜底原文 + trace 标 `fallback=true`，UI 角标小红点（不弹窗）。demo 时演示"断网也能用" |
| 8 | **是否在 Day 1 接 SSE 流式** | 不接。粘贴是一次性动作，流式徒增复杂度 |
| 9 | **是否引入 `async-openai` 等高层 SDK** | 不引入。直接 reqwest + serde，三个 provider 共用 ~200 行足够，少一层依赖 |
| 10 | **Day 1 必须接 Anthropic Opus 吗（万一你拿不到 key）** | 如果你只能拿到 DashScope key，Day 1 把默认改成 `qwen-max`；架构不变，改一行配置 |

请你回我：
- **第 1 项**：默认 provider 选 Anthropic 还是 DashScope
- **第 10 项**：你目前能拿到哪几家的 key

其他项我已经做了合理默认，等你不同意时再讨论。

---

## 14. 与现有 plan.md 的衔接

完成 Day 1 时：
- plan.md §五 Step 10 的"占位 PR 表"用本文件 Day 1 的真实 PR 列表替换
- plan.md §十三 §13.7 待用户决定项逐项回写答案：
  - 改写档清单 → 见 §5
  - 是否热切档 → 录音前在设置面板切；说话中通过语音命令切；UI 切换的"重新生成"留到 Day 3 视进度
  - LLM 提供商 → 默认 Anthropic Opus，可切 OpenAI / DashScope（与 Step 8 复用账号）
  - 端侧改写 → 不做，留 `voice-rewrite-local` crate 占位
  - 自定义 Profile UI → 设置面板多行 textarea，不做语法高亮

完成 Day 3 时：
- plan.md §五 Step 10 加"实际进展（YYYY-MM-DD）"小节，按 Step 3 / Step 5 / Step 8 现有格式回写
