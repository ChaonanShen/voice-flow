# AI 改写层执行计划（voice-flow 增强）

> 配套：[plan.md](./plan.md) §十三 是这部分功能的占位草稿，本文件是落地版。完成后回写 plan.md 的 §13.7 待决项即可，**不要把这里的内容塞回 plan.md**。
>
> 参考：[wispr_flow_analysis.md](./wispr_flow_analysis.md)（产品分析）、[hackathon_ai_voice_input_plan.md](./hackathon_ai_voice_input_plan.md)（功能映射）、[desktop_gui_plan.md](./desktop_gui_plan.md)（Windows/Tauri GUI 提升计划）。

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

**默认走 DeepSeek**，国产、便宜、OpenAI 兼容、中文母语强。已在 2026-05-24 用真实 key 通过 curl 验证 chat completion + JSON mode 两种调用模式可用（实测记录见 §3）。

| 角色 | 默认模型 | Provider | 备注 |
|---|---|---|---|
| **默认 / demo 主用** | `deepseek-chat` | DeepSeek | 后端别名自动路由到当前最新版（实测打到 `deepseek-v4-flash`） |
| 备选（同源、与 Step 8 复用 key） | `qwen-plus` / `qwen-max` | DashScope | 阿里云百炼 OpenAI 兼容端点 |
| 可选（最强中文长文 / 英文） | `claude-opus-4-7` | Anthropic 原生 messages API | 仅当用户主动切到 anthropic 时启用 |
| 可选 | `gpt-5.5`（占位） | OpenAI | 同上 |

> **黑客松策略**：不在"贵但好"和"便宜够用"之间反复挣扎——DeepSeek 在改写任务上已经够用且便宜得"可以随意调"。架构层留多 provider 切换能力（demo 可以演示），但默认装出来就是 DeepSeek。

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

DeepSeek / OpenAI / DashScope 都暴露 **OpenAI 兼容**的 `/v1/chat/completions`，**统一用一份 reqwest 实现 `OpenAiCompatClient`**，只是 `base_url`、`model`、authorization header 略有差异。换 provider = 改配置，**不改代码**。

Anthropic 是唯一例外：它官方 `/v1/messages` 不是严格 OpenAI 兼容（请求体结构、stream 协议、`response_format` 字段都不同），第二实现单独写。但 Anthropic 是 §2.1 表里**最低优先级**，Day 1 / Day 2 不需要。

| Provider | Base URL | Auth header | OpenAI 兼容？ |
|---|---|---|---|
| **DeepSeek**（默认） | `https://api.deepseek.com/v1` | `Authorization: Bearer $KEY` | ✅ 完全兼容（含 `response_format: json_object`） |
| DashScope | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `Authorization: Bearer $KEY` | ✅ 完全兼容 |
| OpenAI | `https://api.openai.com/v1` | `Authorization: Bearer $KEY` | ✅ 本家 |
| Anthropic | `https://api.anthropic.com/v1/messages` | `x-api-key`, `anthropic-version` | ❌ 单独实现 |

> **DashScope 的 OpenAI 兼容端点和 Step 8 用的 paraformer-realtime WebSocket 端点不是同一个。** 同一个 API key 可以复用，但 **client 类型完全分开**——Step 8 的 `voice-asr-cloud::DashScopeClient` 不要碰、不要复用。`voice-rewrite` 自己一套 HTTP client。

### 2.3 默认与切换

- **配置默认 `provider = "deepseek"`，`model = "deepseek-chat"`**
- 用户在设置面板可切到 `dashscope` / `openai` / `anthropic`，模型名是文本框
- 设置面板录入对应 provider 的 API key（每个 provider 单独存；Day 1 用 env / `.env`，Day 2 用 keyring）
- 如果当前 provider 的 key 未配置，桌面端给一个非阻塞提示，pipeline 自动兜底原文输出

### 2.4 价格 / 延迟参考（v1 不优化，仅记录）

一段 200 字中文原文，input ≈ 150 token、output ≈ 100 token（参考 §3 实测）：

| 模型 | 单次成本 | 1000 次成本 | 首 token 经验值 |
|---|---|---|---|
| `deepseek-chat`（默认） | ~¥0.0005 | **~¥0.5** | ~300~600ms |
| `qwen-plus` | ~¥0.005 | ~¥5 | ~400~800ms |
| `qwen-max` | ~¥0.05 | ~¥50 | ~400~800ms |
| `claude-opus-4-7` | ~¥0.4 | ~¥400 | ~500~1000ms |

**黑客松 demo 调用次数估计 ~200 次，DeepSeek 总成本 < ¥0.5**。"随意调"不是夸张——本地开发跑全测试套件也烧不出一杯奶茶钱。

---

## 3. HTTP 调用样例（已实测）

### 3.1 实测记录（2026-05-24，本地 curl）

用 `.env` 里的 `DEEPSEEK_API_KEY` 直接打 DeepSeek 的 OpenAI 兼容端点，两个核心场景都跑通。

#### 场景 A：单段改写（clean profile）

请求：

```http
POST https://api.deepseek.com/v1/chat/completions
Authorization: Bearer $DEEPSEEK_API_KEY
Content-Type: application/json

{
  "model": "deepseek-chat",
  "messages": [
    {"role": "system", "content": "你是一个把口语转写改写成清晰文本的助手。去掉口头禅，补标点，保留原意。只输出改写后的文本本体，不要解释。"},
    {"role": "user", "content": "嗯我今天下午可能会晚到十分钟然后帮我跟老师说一下"}
  ],
  "temperature": 0.3,
  "max_tokens": 200
}
```

响应（截取关键字段）：

```json
{
  "model": "deepseek-v4-flash",
  "choices": [{
    "message": {
      "role": "assistant",
      "content": "我今天下午可能会晚到十分钟，帮我跟老师说一下。"
    },
    "finish_reason": "stop"
  }],
  "usage": {"prompt_tokens": 51, "completion_tokens": 12, "total_tokens": 63}
}
```

**观察**：
- 请求 `model: "deepseek-chat"` → 响应 `model: "deepseek-v4-flash"`。`deepseek-chat` 是稳定别名，DeepSeek 后端自动路由到当前最新版。配置里写 `deepseek-chat` 即可，**不要**写 `deepseek-v4-flash` 这种带版本号的名字。
- 改写质量符合 `clean` profile 预期：去口头禅、补标点、保留原意。
- 51 + 12 = 63 token，单次成本 < ¥0.001。

#### 场景 B：multi 档结构化输出（JSON mode）

加 `response_format: {"type": "json_object"}` 即可强制 JSON 输出：

```json
{
  "model": "deepseek-chat",
  "messages": [
    {"role": "system", "content": "你是一个把口语转写成多种成品文本的助手。请按以下 JSON 结构返回 4 个版本：{\"clean\":\"...\",\"polish\":\"...\",\"wechat\":\"...\",\"bullets\":\"...\"}。约束：保留所有人名、地名、英文专有名词、数字；不要编造原文没有的事实。只输出 JSON，不要任何其他内容。"},
    {"role": "user", "content": "嗯我今天下午可能因为地铁晚点会晚到十分钟，让老师不要等我"}
  ],
  "response_format": {"type": "json_object"},
  "temperature": 0.5,
  "max_tokens": 500
}
```

响应内容（已 `JSON.parse(choices[0].message.content)`）：

```json
{
  "clean":   "我今天下午可能因为地铁晚点会晚到十分钟，让老师不要等我。",
  "polish":  "我今天下午可能因地铁晚点而晚到十分钟，请老师不必等我。",
  "wechat":  "今天下午地铁可能晚点，我会晚到十分钟，让老师别等我了。",
  "bullets": "- 今天下午可能晚到十分钟\n- 原因是地铁晚点\n- 请老师不要等我"
}
```

**观察**：
- JSON 严格合法，可直接 `serde_json::from_str` 解析
- 4 字段风格区分清晰，数字"十分钟"全部保留，人称"老师"保留
- 153 + 102 = 255 token，单次成本 < ¥0.002

### 3.2 Rust 端调用流程（v1 设计）

代码侧基本就是把上面 curl 的事情用 reqwest 包一下。**整个 `LlmClient` v1 实现大约 80 行 Rust**：

```rust
// crates/voice-rewrite/src/llm/openai_compat.rs（v1 设计示意）

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct OpenAiCompatClient {
    http: Client,
    base_url: String,
    api_key: String,
    default_timeout: Duration,
}

impl OpenAiCompatClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            base_url,
            api_key,
            default_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,    // "system" | "user" | "assistant"
    content: &'a str,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,  // "json_object" | "text"
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    model: String,
    choices: Vec<Choice>,
    usage: Usage,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[async_trait::async_trait]
impl LlmClient for OpenAiCompatClient {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        let url = format!("{}/chat/completions", self.base_url);

        let body = ChatCompletionRequest {
            model: &req.model,
            messages: vec![
                ChatMessage { role: "system", content: &req.system },
                ChatMessage { role: "user",   content: &req.user },
            ],
            temperature: req.temperature.unwrap_or(0.3),
            max_tokens: req.max_tokens.unwrap_or(800),
            response_format: match req.response_format {
                ResponseFormatKind::JsonObject => Some(ResponseFormat { kind: "json_object" }),
                ResponseFormatKind::Text => None,
            },
        };

        let resp = tokio::time::timeout(req.timeout, async {
            self.http
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await?
                .error_for_status()?
                .json::<ChatCompletionResponse>()
                .await
        })
        .await
        .map_err(|_| LlmError::Timeout)?
        .map_err(LlmError::from_reqwest)?;

        let content = resp.choices.into_iter().next()
            .ok_or(LlmError::EmptyResponse)?
            .message.content;

        Ok(ChatResponse {
            content,
            tokens_in: resp.usage.prompt_tokens,
            tokens_out: resp.usage.completion_tokens,
            actual_model: resp.model,
        })
    }
}
```

### 3.3 三个 OpenAI 兼容 provider 的复用

DeepSeek / DashScope / OpenAI 三家直接复用同一个 `OpenAiCompatClient`，**只有构造参数不同**：

```rust
// DeepSeek（默认）
OpenAiCompatClient::new(
    "https://api.deepseek.com/v1".to_string(),
    env::var("DEEPSEEK_API_KEY")?,
)

// DashScope
OpenAiCompatClient::new(
    "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
    env::var("DASHSCOPE_API_KEY")?,
)

// OpenAI
OpenAiCompatClient::new(
    "https://api.openai.com/v1".to_string(),
    env::var("OPENAI_API_KEY")?,
)
```

### 3.4 Anthropic（独立实现，Day 1 / Day 2 不做）

Anthropic 原生 `/v1/messages` 的请求体 / 响应体 schema 不同：
- 用 `system` 字段（顶层），不放在 `messages` 数组
- header 是 `x-api-key` + `anthropic-version`，不是 `Authorization: Bearer`
- 没有 `response_format` 字段，JSON mode 靠 prompt 强约束（或用 tool use）

如果要支持，**单独写 80 行 `AnthropicClient`**，不复用 `OpenAiCompatClient`。Day 3 视进度再决定要不要做。

### 3.5 .env 加载机制

进程启动时（CLI 入口、桌面 main、tests），用 `dotenvy` crate 把 `.env` 内容加载到 `std::env`：

```rust
// crates/voice-cli/src/main.rs（入口示意）
fn main() {
    let _ = dotenvy::dotenv();  // 找不到 .env 不报错，由后续读取 env 的代码兜底
    // ...
}
```

读取顺序见 [.env.example](./.env.example) 顶部注释：keyring > 进程 env > .env 文件。

### 3.6 模型档位与 thinking mode 的选择

**默认 `deepseek-chat`（= v4-flash），不开 thinking。** 这是有意识的选择，记录理由如下。

#### 为什么 flash 够用

改写任务的本质是**模式转换 + 风格迁移**，不是推理 / 数学 / 因果分析。模型不需要"思考再回答"，flash 在它的舒适区里。各 Profile 的实际负担：

| Profile | 模型在做什么 | flash 够用？ |
|---|---|---|
| `clean` | 模式删除（嗯/啊）+ 标点补全 | ✅ 杀鸡用牛刀 |
| `wechat` / `commit` | 风格转换 + 长度约束 | ✅ |
| `polish` | 风格转换 + 词汇升级 | ✅ |
| `email` | 格式扩展（称呼 + 正文 + 结尾） | ✅ |
| `bullets` | 信息抽取 + 重组 | ✅ 偶尔会"过度合并" |
| `prompt` | 把含糊需求翻译成结构化 prompt | ⚠️ 临界 — 简单够用，复杂需求 V3 更好 |
| `multi` | 一次出 4 版本 + 风格区分 | ⚠️ 临界 — 偶尔风格区分度不够 |

§3.1 的实测已经验证 clean / multi 在 flash 上的输出质量符合预期。

#### 为什么**不开** thinking

1. **延迟爆表**：thinking 让首 token 从 ~400ms 拉到 ~3~10s。§7.3 的延迟 budget 是 "ASR + 1~2s"，开 thinking 直接破表
2. **改写没有推理路径**：模型不需要"先想再写"，直接给答案就行
3. **成本翻倍**：thinking token 通常按输出 token 计费，对高频调用场景不划算

#### 真正可能需要升档的两个边界

1. **prompt 档遇到含糊需求** → 升 `deepseek-v3-1` 或 Claude Opus，能更好推断隐含意图
2. **multi 档 4 个版本风格区分不够** → 升 `deepseek-v3-1`，V3 系列生成更发散

这两个场景**都不会在黑客松 demo 出现**——demo 用的是清晰简短的口语，flash 完全应付得了。

#### 留好的扩展点：profile_overrides

§5.4 的配置 schema 里**预留** `profile_overrides`，允许某个 Profile 单独覆盖 model（v1 不实现，留接口）：

```toml
[rewrite]
provider = "deepseek"
model = "deepseek-chat"        # 全局默认

# v2 扩展：profile 级覆盖
[rewrite.profile_overrides.multi]
model = "deepseek-v3-1"        # multi 档单独用更强的模型

[rewrite.profile_overrides.prompt]
model = "deepseek-v3-1"
```

代码侧 `Pipeline::resolve_model(profile)` 优先读 overrides，未命中 fall back 到全局 `model`。**v1 写一行 `unwrap_or(global_model)` 就够，将来扩展不破坏现有调用方**。

#### Day 3 前的验证动作

跑通 Day 1~2 后，用 demo 那段固定 WAV 做 A/B 对比：

```bash
voice-cli rewrite --profile=multi --model=deepseek-chat  < demo.txt
voice-cli rewrite --profile=multi --model=deepseek-v3-1  < demo.txt
```

肉眼对比 4 个版本的风格差异度。90% 概率 flash 已经够好，保持默认；如果明显差，给 multi / prompt 单独配 V3。

#### 结论

- **v1 默认**：`deepseek-chat`（flash 别名），全档共用，不开 thinking
- **扩展点已留**：`profile_overrides` schema 写进 §5.4，v1 不实现但代码侧 `resolve_model` 留一行
- **Day 3 视进度**：跑 A/B，决定 multi / prompt 是否单独升档

---

## 4. 数据流与模块边界

**核心原则：AI 改写是"文字→文字"，不接触音频。** 它和 ASR 是顺序流水线上的两个独立阶段，代码上严格分到不同 crate、不同 trait、不同测试集，互不知道对方的存在。

### 4.1 全链路数据流（按域分两段）

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

### 4.2 域 → crate 映射

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

### 4.3 trait 边界（代码骨架）

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

`RewriteTrace` 是后面 UI（§9 的 R3.4）和测试断言都要用的，建议从 R1.2 就埋好字段，后续 PR 只往里填东西。

### 4.4 voice-core 串联点

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

### 4.5 测试视角下的边界

| 测试集 | 跑在哪 | 依赖 |
|---|---|---|
| `voice-rewrite` 单测 / 集成 | Linux / Windows 都行 | 仅 mock LLM，不需要任何模型 / 音频文件 |
| `voice-rewrite` + 真 LLM（可选） | 本地手动 | 需要 `ANTHROPIC_API_KEY` 等环境变量，CI 默认 skip |
| `voice-core::push_to_talk` 改写集成 | Linux 也能跑 | 用 `FileCapture` + `MockAsrEngine` + `MockRewritePipeline` |
| 完整链路 | 仅 Windows | 真麦克风 + 真 ASR + 真 LLM |

测试样例细节见 §13。

---

## 5. 架构落点

### 5.1 新增 crate

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

### 5.2 voice-core 改动

- `RealtimeState` 新增 `Rewriting`（在 `Transcribing` 和 `Completed` 之间）
- `engine.rs` 不动；改写不是 ASR engine 的一部分，**挂在 ASR 之后**
- 新增 `text_pipeline.rs`：把 `transcript: String` + `RewriteSettings` 喂给 `RewritePipeline`，返回最终文本
- 现有 push_to_talk 流程里，在 ASR 出文本后、写剪贴板前插入一次 `pipeline.process()`

### 5.3 桌面壳改动

- 设置面板新增"AI 改写"分组（开关、Provider、Model、Profile、API key、custom prompt）
- 悬浮窗状态加 `Rewriting`（前端字串："改写中..."）
- 悬浮窗加"最近文本"多版本切换标签（multi 档启用时显示）

### 5.4 配置形态

`%APPDATA%\voice-flow\app.toml` 扩展：

```toml
[rewrite]
enabled = false                          # 默认关；用户自己显式开
default_profile = "clean"                # 默认档
provider = "deepseek"                    # deepseek | dashscope | openai | anthropic
model = "deepseek-chat"                  # 稳定别名，自动路由到当前最新版
timeout_ms = 4000                        # DeepSeek 单段 ~1s，4s 兜底足够；multi 偶尔到 3s
api_key_ref = "system-keyring"           # 桌面端 keyring；CLI / 测试 fallback 到 env / .env

# 不同 provider 的 key 单独存（设置面板每个 provider 一个输入框）
# keyring 里的 key 名称约定：voice-flow:deepseek、voice-flow:dashscope、voice-flow:openai、voice-flow:anthropic
# 对应 env 变量名：    DEEPSEEK_API_KEY、DASHSCOPE_API_KEY、OPENAI_API_KEY、ANTHROPIC_API_KEY

[rewrite.user_dictionary]
# 用户专有名词 / 易错词替换。preprocess 在 LLM 之前 apply
# 左 = ASR 可能出的错，右 = 正确写法
"克劳德" = "Claude"
"我推" = "Vue"
"赛恩" = "Shen"

[rewrite.profiles.custom]
system_prompt = ""                       # 高级用户自己写

# === v2 扩展点（v1 不实现，但 config schema 预留键名以免破坏向后兼容） ===
# 某个 Profile 单独覆盖 model，用于 multi / prompt 在质量要求高时升档。
# 设计依据见 §3.6。
# [rewrite.profile_overrides.multi]
# model = "deepseek-v3-1"
```

桌面端 API key 默认走 keyring（Windows Credential Manager）；CLI / 单元测试可直接读 env / `.env`。

---

## 6. 改写档（Profile）清单

下面是 v1 落地的档位。`off` 之外的每档都对应一个 system prompt（§8 给出）。

**v1 阶段所有档共用配置里的同一个 `model`**（默认 `deepseek-chat`），不分快档/慢档——DeepSeek 一档便宜得不需要分流。后续如果切到更贵的 provider，再加 `fast_model` / `quality_model` 路由。

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

## 7. 编排管道（创新点）

### 7.1 完整流程

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

### 7.2 三个值得喊出口号的编排细节

| 编排点 | 为什么是创新 |
|---|---|
| **本地预处理 + LLM 两段** | LLM 输入更短更干净，省 token、降延迟、保留专有名词。等价于"零训练个性化" |
| **语音命令在 ASR 流里识别，不靠 UI** | 用户说"改正式一点，写邮件"就能切档，不用切换 Profile 下拉 |
| **多版本一次 LLM 调用** | 用 JSON 结构化输出，1 次调用拿 4 个版本，比 4 次并行调用便宜 + 快 |

### 7.3 延迟 budget

目标（中文 200 字原文，默认 `deepseek-chat`）：

| 阶段 | 预算 |
|---|---|
| ASR（已实现） | 本地 200~500ms |
| 预处理 | <10ms |
| LLM 首 token | 300~600ms |
| LLM 完整生成 | 800~2000ms |
| 后处理 | <5ms |
| 粘贴 | <50ms |
| **改写档关闭** | 与现在持平 |
| **clean / polish / wechat 等单段** | ASR + 1~2s |
| **multi（JSON 4 字段）** | ASR + 2~3s |

`timeout_ms` 默认 4000，超时全部兜底原文。切到 Anthropic Opus 等更慢的 provider 时，pipeline 自动用配置里的更大 `timeout_ms`。

---

## 8. Prompt 设计

### 8.1 通用规则

- 每个 Profile 一份 system prompt，**编译进二进制**（`include_str!`），不读运行时文件
- 用户原文作为单独 user message，不和 system 拼接（防 prompt injection）
- system prompt 末尾固定加："只输出改写后的文本本体。不要解释、不要 markdown 包裹、不要前缀。如果你无法改写，原样输出输入。"
- 不传任何对话历史 / 上下文，**每次都是 zero-shot**

### 8.2 示例：clean profile system prompt

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

### 8.3 示例：multi profile（结构化）

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

### 8.4 语音命令映射

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

## 9. 三天 PR 拆分

风格对齐 [plan.md](./plan.md) §6（单一职责、≈300 行、Conventional Commit）。

### 9.0 拆分粒度说明（重要）

**下面表格里每一行是一个"功能单位"，不是一个 PR 上限。**

实际开发中遵循 [plan.md](./plan.md) §6.6 的"能拆就拆"原则——表格里大多数行**还可以再拆成 2~4 个更小的 PR**。下面给几个例子：

| 表格里的一行 | 实际可以拆成的小 PR |
|---|---|
| R1.1 `chore(rewrite): scaffold voice-rewrite crate` | (a) `chore(rewrite): create empty crate + workspace registry` <br> (b) `chore(rewrite): add cargo dependencies` <br> (c) `feat(rewrite): define error types` |
| R1.6 `feat(rewrite): openai-compat http client` | (a) `feat(rewrite): define LlmClient trait + ChatRequest/Response types` <br> (b) `feat(rewrite): implement reqwest-based OpenAiCompatClient (no retry)` <br> (c) `feat(rewrite): add timeout handling to OpenAiCompatClient` <br> (d) `feat(rewrite): add 1-shot retry on 5xx` |
| R1.8 `feat(rewrite): llm pipeline with clean profile` | (a) `feat(rewrite): add clean profile system prompt constant` <br> (b) `feat(rewrite): implement LlmPipeline::process happy path` <br> (c) `feat(rewrite): add fallback-to-original on llm error` <br> (d) `test(rewrite): integration tests with MockLlmClient (10 cases)` |
| R2.12 `feat(desktop): rewrite settings web ui` | (a) `feat(desktop): rewrite settings mock view` <br> (b) `feat(desktop): rewrite profile controls` <br> (c) `feat(desktop): provider and model controls` <br> (d) `feat(desktop): api key form shell` |

**取舍原则**：
- 一个 PR 一个动词，能用 "add X" / "implement Y" / "wire Z" 一句话讲清就行
- diff ≤ 300 行（不含 lock 文件和生成代码），超出的优先拆
- 拆出来的 PR 仍然要"主分支可运行"——空 trait 加个 `unimplemented!()` 也算可运行，下一 PR 再补实现
- 测试和实现可以同 PR，也可以拆开（实现 PR 留 `TODO: tests in follow-up`，紧接着一个 test PR）。前者适合简单单测，后者适合 multi-case 测试集
- **PR 编号是给计划用的，git 里的实际 commit / PR 不必严格 R1.6.a/b/c 这样编**——动词清楚 + 单一职责就够了

下面表格里如果某一行的"单一职责"列写了 "X + Y" 或多动词，那基本就是"还可以再拆"的信号。

### 9.0.1 GUI 边界

GUI 不另起独立 browser app。当前桌面端已经是 Tauri，前端就是 Web 技术栈。

本文件只记录 AI 改写引擎、profile、provider、fallback 和文本域测试。GUI / Windows 桌面体验的详细路线，包括 Tauri Web UI、keyring、托盘、trace 面板、打包和实机验收，统一记录在 [`desktop_gui_plan.md`](./desktop_gui_plan.md)。

### 9.1 Day 1：跑通最小链路（rewrite=clean，DeepSeek）

| 功能单位 | 标题 | 单一职责 |
|---|---|---|
| R1.1 | `chore(rewrite): scaffold voice-rewrite crate` | 空 crate + workspace 注册 + 错误类型；**显式禁止**依赖 `voice-asr-*` / `cpal` / `sherpa-onnx` |
| R1.2 | `feat(rewrite): add RewritePipeline trait + types` | `RewritePipeline` / `RewriteContext` / `RewriteResult` / `RewriteTrace`（trait + 数据结构定义，无实现） |
| R1.3 | `feat(rewrite): identity pipeline + mock llm` | 直通实现 + `MockLlmClient`（test-only），覆盖 P7 / I1 / I4 几个最简单测试用例 |
| R1.4 | `feat(rewrite): add filler removal preprocess` | 正则去口头禅 + 单测（F1~F10） |
| R1.5 | `feat(rewrite): add user dictionary substitution` | 词典替换 + 单测（D1~D6） |
| R1.6 | `feat(rewrite): openai-compat http client` | `OpenAiCompatClient`：base_url / api_key / headers 参数化 + timeout + 1-shot retry（**建议拆成 4 个小 PR，见 §9.0**） |
| R1.7 | `feat(rewrite): deepseek provider impl` | DeepSeek base_url + 默认 model `deepseek-chat` + `dotenvy` 加载 + 从 env 读 `DEEPSEEK_API_KEY` |
| R1.8 | `feat(rewrite): llm pipeline with clean profile` | clean system prompt + 超时兜底 + 集成测试（mock LLM，覆盖 P1~P3、P9） |
| R1.9 | `feat(core): add Rewriting realtime state` | 状态枚举 + 事件 |
| R1.10 | `feat(core): wire rewrite into push-to-talk` | ASR 后插入 pipeline，默认 off；改写为 None 时零开销，覆盖 I2 / I3 / I5 |
| R1.11 | `feat(cli): rewrite subcommand (text→text)` | `voice-cli rewrite --profile=clean < input.txt`，纯文字到文字，调试改写最快 |
| R1.12 | `feat(cli): transcribe --rewrite=clean flag` | `voice-cli transcribe x.wav --rewrite=clean`，audio→ASR→改写一条龙 |

**Day 1 验收**：
- `voice-cli rewrite --profile=clean < input.txt` 接收 stdin，输出 DeepSeek 改写结果（§3 实测已通）
- `voice-cli transcribe x.wav --rewrite=clean` 跑通 audio→text→rewrite 全链路
- Linux 上 mock LLM 单测全过（**不需要联网，不需要真 key**）
- 桌面端**不接 UI**，但状态机已经能切到 Rewriting

**实际进展（2026-05-24）**：

- R1.1~R1.12 已完成：`voice-rewrite` crate、预处理、用户词典、OpenAI-compatible HTTP client、DeepSeek 默认 provider、LLM pipeline、`Rewriting` 状态、`voice-cli rewrite` 和 `voice-cli transcribe --rewrite` 都已落地。
- R1.10 的非 GUI 实时路径已接到 `voice-cli push-to-talk-transcribe --rewrite <profile>`；桌面 UI 接线按当前策略后置。
- 验证命令：`cargo test -p voice-rewrite`、`cargo test -p voice-core`、`cargo test -p voice-cli`。

### 9.2 Day 2：做差异化（多 Profile + 命令 + 多 Provider + 桌面 UI）

| 功能单位 | 标题 | 单一职责 |
|---|---|---|
| R2.1 | `feat(rewrite): add polish profile` | 中度润色 prompt + 测试 |
| R2.2 | `feat(rewrite): add email profile` | 邮件 prompt + 测试 |
| R2.3 | `feat(rewrite): add wechat profile` | 微信 prompt + 测试 |
| R2.4 | `feat(rewrite): add commit profile` | Conventional Commit prompt + 测试 |
| R2.5 | `feat(rewrite): add bullets profile` | 要点 prompt + 测试 |
| R2.6 | `feat(rewrite): add prompt profile` | AI prompt 格式 + 测试 |
| R2.7 | `feat(rewrite): voice command detection in preprocess` | 7 个命令正则 + 命令移除 + 单测 C1~C8（**建议拆成"加 regex"+"加 command→profile 映射"+"加测试"三个 PR**） |
| R2.8 | `feat(rewrite): multi-version profile with json output` | `response_format=json_object` + 4 字段 schema 解析（覆盖 P4~P6） |
| R2.9 | `feat(rewrite): dashscope provider impl` | 复用 R1.6 client，base_url 切 `compatible-mode/v1`，默认 model `qwen-plus` |
| R2.10 | `feat(rewrite): openai provider impl` | 复用 R1.6 client，OpenAI 官方 base_url + 默认 model（待定，Day 2 时按当前发布版填） |
| R2.11 | `feat(core): config schema for rewrite section` | `app.toml [rewrite]` 读写 + 单测 |
| R2.12 | `feat(desktop): rewrite settings web ui` | G1：在 Tauri 前端用 mock 数据做开关、Profile、Provider、Model、API key form shell |
| R2.13 | `feat(desktop): wire rewrite settings commands` | G2：Tauri command 读写 `app.toml [rewrite]`，替换 mock 数据 |
| R2.14 | `feat(desktop): secure rewrite keys with keyring` | G3：Windows Credential Manager 存储每个 provider 的 key |
| R2.15 | `feat(desktop): show current rewrite profile` | G1/G2：悬浮窗显示当前档，先 mock，后接 runtime/config |
| R2.16 | `feat(desktop): render rewriting state` | G1/G2：渲染 "改写中..." 状态，先 mock state，后接 `RealtimeState::Rewriting` event |
| R2.17 | `feat(desktop): multi-version variant tabs` | G1/G2：multi 档显示 4 个标签，先 mock variants，后接 `RewriteResult::variants` |

**Day 2 验收**：
- Windows 上：选 email 档说话 → 松开 → 粘贴出邮件正文
- 说"改正式一点，下午晚到十分钟" → 自动切到 polish 档输出正式句
- 选 multi 档 → 悬浮窗 4 个标签都填好，点击任一可复制对应版本
- 设置面板切换 DeepSeek / DashScope 至少各成功跑一次（OpenAI 可选）

**实际进展（2026-05-24）**：

- R2.1~R2.8 已完成：`polish` / `email` / `wechat` / `commit` / `bullets` / `prompt` / `multi` profiles、语音命令识别、multi JSON 解析和 mock 覆盖都已落地。
- R2.9~R2.10 已完成到 provider 层：DeepSeek / DashScope / OpenAI 共用 `OpenAiCompatClient`，CLI 暴露 `--provider` / `--rewrite-provider`。
- R2.11 已完成：`app.toml [rewrite]` schema、默认关闭策略、用户词典、provider/model/profile/timeout 读写，以及 `voice-core::text_pipeline` 非 GUI glue。
- R2.12~R2.13 已完成：Tauri 内 Web UI 已支持 rewrite 开关、profile/provider/model/timeout 设置，并通过 command 读写 `app.toml [rewrite]`。
- R2.15~R2.17 已完成到 G2：悬浮窗显示当前 rewrite profile，`RealtimeState::Rewriting` 已由 desktop runtime 真实发出，multi variants 已从 mock 改为监听 `rewrite-result` 事件。
- R2.14 已完成到桌面端：Settings 可保存 / 清除 provider API key，Tauri 后端通过 Windows Credential Manager keyring 持久化，desktop runtime 构建 rewrite engine 时优先读取 keyring；CLI / 测试仍可走环境变量 / `.env`。
- Day 2 剩余验证：Windows 实机用真实录音 + 真实 LLM 跑 email / voice command / multi 三条验收路径，具体 checklist 见 [`desktop_gui_plan.md`](./desktop_gui_plan.md) G5。

### 9.3 Day 3：包装、demo、收尾

| 功能单位 | 标题 | 单一职责 |
|---|---|---|
| R3.1 | `feat(rewrite): post-process length sanity check` | 异常压缩兜底原文（覆盖 L1~L4） |
| R3.2 | `feat(rewrite): preserve numbers and proper nouns guard` | 数字 / 英文专有名词丢失检测（覆盖 N1~N4） |
| R3.3 | `feat(desktop): latency breakdown web ui` | G1/G2：先用 mock trace 显示 ASR / 改写 / 粘贴耗时，再接 `RewriteTrace` |
| R3.4 | `feat(desktop): rewrite trace overlay` | G1/G2：先用 mock 流程显示"预处理→命令→改写"，再接真实 trace |
| R3.5 | `feat(core): hot reload rewrite settings` | G2/G3：设置改完不用重启就生效（复用现有 restart_requested） |
| R3.6 | `feat(desktop): custom profile prompt editor` | G1/G2：设置里多行文本框，先前端编辑态，再接持久化 |
| R3.7 | `docs: ai rewrite usage guide` | README 加章节 + 截图 |
| R3.8 | `docs: demo script for ai rewrite` | `docs/demo-rewrite.md` 演示步骤 |
| R3.9 | `chore: fixtures for ai rewrite demo` | 一段固定 WAV + 期望输出，让评委可复现 |

**Day 3 验收**：
- Demo 脚本走通（§11）
- 改设置不重启桌面即生效
- README 和 demo 文档完整

**实际进展（2026-05-24）**：

- R3.1~R3.2 已完成：过短输出、数字丢失、英文专有名词丢失都会触发原文兜底。
- R3.3 已完成基础版：桌面文稿模式展示 ASR / rewrite / paste 粗略耗时；尚未做完整 trace overlay。
- R3.7~R3.8 已完成到非 GUI 文档层：README 增加 AI 改写使用指南，`docs/demo-rewrite.md` 提供纯文本、真实 LLM smoke 和 WAV-to-rewrite 命令。
- R3.9 已完成：`docs/fixtures/demo-rewrite.wav` 和 `docs/fixtures/demo-rewrite.expected.txt` 已提交，用于固定 demo 路径。
- R3.4~R3.6 仍后置：rewrite trace overlay、设置热更新的精细化和 custom profile prompt editor 仍未做，具体执行顺序见 [`desktop_gui_plan.md`](./desktop_gui_plan.md)。

### 9.4 节奏与删减

如果 Day 1 跑不完到 R1.12，**优先保证 R1.6~R1.10 落地**（拿到 clean 档基础链路 + 串到 push_to_talk），R1.3~R1.5 可以放到 Day 2 头一两个 PR，R1.11 / R1.12 谁也行都能砍。

---

## 10. 配置 / 密钥 / 隐私

### 10.1 API key 存储分层

| 来源 | 何时用 | 何时写 |
|---|---|---|
| 系统 keyring（Day 2 引入） | 桌面端运行时优先读 | 设置面板保存时 |
| 进程环境变量 | CLI / 测试 / keyring 缺失时兜底 | 用户手动 `export` 或 `source .env` |
| `.env` 文件（开发期） | CLI 启动时 `dotenvy::dotenv()` 加载到 env | 用户编辑 `.env` |

读取优先级：**keyring > env > .env**。任何场景下都**不写 TOML 明文**。

keyring 里**每个 provider 单独一个 entry**：
- `voice-flow:deepseek`（默认）
- `voice-flow:dashscope`
- `voice-flow:openai`
- `voice-flow:anthropic`

对应 env 变量名：`DEEPSEEK_API_KEY` / `DASHSCOPE_API_KEY` / `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`（与 [.env.example](./.env.example) 一致）。

设置面板：选中 provider 后显示对应 key 输入框；保存后立即写 keyring，输入框显示 `********`。删除 = "清除"按钮 → keyring delete 对应 entry。

### 10.2 数据出境提示

- **改写默认关**。开启时设置面板必须有一行说明（按当前 provider 动态替换）：
  > 开启 AI 改写后，识别文本会发送到 {DeepSeek | DashScope | OpenAI | Anthropic} 服务器进行处理。不要在涉密场景下开启。
- 关闭后，链路完全不联网（除非 ASR engine 选了 cloud）

### 10.3 日志

- LLM 调用入参 / 出参**不**写日志（仅 trace 级开关打开时写）
- 错误日志只记录错误类型 + 状态码 + token 用量，不记录 prompt / 转写文本

---

## 11. Demo 脚本

固定 WAV 文件 `docs/fixtures/demo-rewrite.wav`，内容：

> 嗯，我要写个邮件，就是跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我，那个语气正式一点。

### 演示顺序

1. **基础对比**：rewrite=off → 粘贴原始转写
2. **clean 档**：粘贴去口头禅 + 补标点的版本
3. **email 档**：粘贴正式邮件
4. **multi 档**：悬浮窗显示 4 个版本，逐个点击展示
5. **语音命令**：用户口语里"语气正式一点"被识别，自动切到 polish
6. **Provider 切换（可选）**：同一句话用 DeepSeek vs DashScope Qwen 对比延迟和风格

### 关键展示话术

> 我们没有训练任何模型，也没用 RAG。我们做的是 AI 编排层：
>
> 一段口语先经过本地预处理——去口头禅、应用用户词典——这是零训练的"个性化"；
> 然后正则在原文里识别用户的语音命令，自动选择改写档；
> 最后一次 LLM 调用通过结构化 JSON 输出拿到多个版本，比并发调用便宜也更快。
>
> ASR 负责听清楚，LLM 负责改写，编排层把它们串成一个延迟稳定的产品。

---

## 12. 删减线（进度落后按序砍）

| 序号 | 砍掉 | 影响 |
|---|---|---|
| 1 | R3.6 custom profile 编辑器 | 编辑 TOML 即可 |
| 2 | R3.4 trace overlay / R3.3 延迟显示 | demo 解说时口播 |
| 3 | R3.2 数字 / 专有名词保护 | 不致命，但 demo 时小心选词 |
| 4 | R2.17 multi-version 多标签 UI | 退化成 multi 档输出 JSON 到悬浮窗"最近文本"区，能看就行 |
| 5 | R2.14 keyring | 退化成只用 env / `.env`（已 gitignore，开发够用） |
| 6 | R2.9 / R2.10 DashScope / OpenAI provider | 退化成只有 DeepSeek 一家 |
| 7 | R2.7 语音命令识别 | 退化成仅靠 Profile 下拉 |
| 8 | R2.8 multi 档 | 退化成只有 clean / polish / email 三档 |
| 9 | **底线**：Day 1（R1.1~R1.12）必须完成 | clean 档单条链路跑通 = 创新点已立住，剩下都是加分 |

---

## 13. 测试策略与测试样例

### 13.1 分层

| 层 | 跑在哪 | 覆盖 | 工具 |
|---|---|---|---|
| L1 单测：preprocess | Linux / Windows / CI | filler / 词典 / 命令检测 | `cargo test -p voice-rewrite` |
| L2 单测：postprocess | 同上 | 长度 / 数字 / 专有名词检查 | 同上 |
| L3 集成：pipeline + MockLlmClient | 同上 | 完整文本域流程，不联网 | `cargo test -p voice-rewrite` |
| L4 集成：voice-core 串联 | 同上 | `MockAsrEngine` + `MockRewritePipeline` 串 push_to_talk | `cargo test -p voice-core` |
| L5 真 LLM 烟雾测试 | 本地手动 | 单档 + multi 档真实调用一次 | `cargo test -p voice-rewrite --test live_deepseek_examples -- --ignored` |
| L6 端到端 | 仅 Windows | 麦克风→ASR→改写→粘贴 | 人工 |

L1~L4 **必须**在 CI 跑（一旦加 CI）。L5 默认 ignore，避免 CI 调用真实 API 烧钱。L6 是 demo 验收。

### 13.2 Mock LLM 设计

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

### 13.3 具体测试样例（按 PR 对应）

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

需要 `DEEPSEEK_API_KEY` 环境变量或 `.env`；CI 不跑：

| # | profile | 原文 | 验证 |
|---|---|---|---|
| S1 | clean | `嗯我今天下午可能会晚到十分钟` | 输出非空、不含"嗯"、含"十分钟" |
| S2 | multi | 同上 | 解析 JSON 成功、4 字段都非空 |
| S3 | clean，timeout=1ms | 同上 | fallback=true |
| S4 | clean，key 错误 | 任意 | LlmError::Auth，fallback=true |

#### L5 扩展：真实输出评审（先记录，暂不执行）

真实 DeepSeek / DashScope 的改写输出**不做逐字相等断言**。即使 temperature 很低，LLM 也可能在同义表达、标点、称呼顺序上有小幅波动；真实调用测试只检查“能调用、非空、关键事实保留、不触发 fallback”等稳定性质。

为了评估“润色效果好不好”，后续可以加两类 `#[ignore]` 测试 / eval 脚本，默认不进入 CI：

1. **人类 judge（优先）**
   - 文件建议：`crates/voice-rewrite/tests/live_deepseek_examples.rs`
   - 做法：对固定输入批量跑 `clean / polish / email / wechat / bullets / multi`，用 `--nocapture` 打印真实输出。
   - 断言：只做最低限度断言（输出非空、关键事实如“老师 / 地铁 / 十分钟”仍存在、`fallback=false`）。
   - 用途：给开发者人工看 profile 风格、prompt 质量和 demo 可用性，适合早期快速调 prompt。
   - 示例命令：
     ```bash
     cargo test -p voice-rewrite --test live_deepseek_examples -- \
         --ignored --nocapture --test-threads=1
     ```

   期望打印形态：
   ```text
   === case: teacher_late ===
   input:
   嗯，跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我

   [clean]
   ...

   [polish]
   ...

   [email]
   ...
   ```

2. **LLM judge（可选，后置）**
   - 文件建议：`crates/voice-rewrite/tests/live_deepseek_judge.rs`
   - 做法：第一次调用生成 rewrite 输出；第二次调用 LLM 作为 judge，按固定 rubric 返回 JSON。
   - JSON schema 建议：
     ```json
     {
       "score": 0,
       "kept_facts": true,
       "removed_fillers": true,
       "style_matches_profile": true,
       "no_hallucination": true,
       "reason": "..."
     }
     ```
   - 断言：JSON 可解析，`score >= 8`，`kept_facts=true`，`no_hallucination=true`。
   - 风险：LLM judge 本身也有波动，且每个样例至少两次 API 调用；因此只作为本地质量参考，不作为 merge 阻塞项。

这两类测试都属于**真实质量评估**，和 L1~L4 的确定性 mock 测试职责不同：mock 测试保障编排逻辑稳定，live eval 帮助判断 prompt 和真实模型输出是否值得用于 demo。

### 13.4 测试 fixtures

- `crates/voice-rewrite/tests/fixtures/preprocess/`：每个样例一个 `.in.txt` + `.expected.txt`
- `crates/voice-rewrite/tests/fixtures/multi_json/`：几个 LLM 返回的 JSON 样本（正确 / 缺字段 / 非法）
- `docs/fixtures/demo-rewrite.wav`：demo 用的固定音频（Day 3 引入）

### 13.5 跑测命令一览

```bash
# Linux / Windows 都能跑
cargo test -p voice-rewrite                 # 全部本地测试（mock LLM）
cargo test -p voice-rewrite --test live_deepseek_examples -- \
    --ignored --test-threads=1              # 真 LLM 烟雾测试
cargo test -p voice-core text_pipeline      # voice-core 串联集成

# 一键全部（不含真 LLM）
cargo test --workspace
```

---

## 14. 决策点（已定稿）

启动 Day 1 前的决策都已定，列在这里作记录：

| # | 决策 | 结论 |
|---|---|---|
| 1 | 默认 provider | **DeepSeek**（`deepseek-chat`）。已在 2026-05-24 用 [.env](.env) 里的真实 key 通过 curl 实测两个调用模式（§3.1） |
| 2 | API key 注入方式 | Day 1：env / `.env` 文件 + `dotenvy` 加载。Day 2：桌面端引入 keyring 持久化，CLI / 测试仍 fallback 到 env |
| 3 | 模型版本绑定 | 配置写 `deepseek-chat` 这个稳定别名，**不**写 `deepseek-v4-flash` 等具体版本号。后端自动路由到最新 |
| 4 | OpenAI 兼容 client 复用 | DeepSeek / DashScope / OpenAI 三家共用一份 `OpenAiCompatClient`。Anthropic 单独写（Day 3 视进度，可砍） |
| 5 | `voice-cli rewrite` 子命令 | 做。R1.11 顺手做了，纯文字→文字，调试和 demo 都用得上 |
| 6 | 是否要求 ASR engine ≠ off 才能开改写 | 不要求。改写是独立文本管道，可独立测试 |
| 7 | 改写失败处理 | 静默兜底原文 + trace 标 `fallback=true`，UI 小红点提示（不弹窗）。Demo 时演示"断网也能用" |
| 8 | Day 1 是否接 SSE 流式 | 不接。粘贴是一次性动作，流式无收益 |
| 9 | 是否引入 `async-openai` 高层 SDK | 不引入。直接 reqwest + serde 共用 ~80 行（§3.2 给了完整骨架） |
| 10 | DashScope client 是否复用 `voice-asr-cloud::DashScopeClient` | 不复用。Step 8 是 WebSocket 协议，本计划是 HTTP OpenAI 兼容；client 类型完全分开，API key 可同一个账户 |

---

## 15. 与现有 plan.md 的衔接

完成 Day 1 时：
- plan.md §五 Step 10 的"占位 PR 表"用本文件 Day 1 的真实 PR 列表替换
- plan.md §十三 §13.7 待用户决定项逐项回写答案：
  - 改写档清单 → 见 §6
  - 是否热切档 → 录音前在设置面板切；说话中通过语音命令切；UI 切换的"重新生成"留到 Day 3 视进度
  - LLM 提供商 → 默认 DeepSeek `deepseek-chat`，可切 DashScope / OpenAI / Anthropic
  - 端侧改写 → 不做，留 `voice-rewrite-local` crate 占位
  - 自定义 Profile UI → 设置面板多行 textarea，不做语法高亮

完成 Day 3 时：
- plan.md §五 Step 10 加"实际进展（YYYY-MM-DD）"小节，按 Step 3 / Step 5 / Step 8 现有格式回写
