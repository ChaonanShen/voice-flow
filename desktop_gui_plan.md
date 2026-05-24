# Desktop GUI 提升计划（Windows-first）

> 本文专门记录 voice-flow 的 Windows 桌面 GUI、Tauri runtime、系统集成、打包和人工验收计划。
>
> 文档边界：
> - [`plan.md`](./plan.md)：基础语音转文本主线，重点是录音、ASR、快捷键、剪贴板、粘贴。
> - [`ai_rewrite_plan.md`](./ai_rewrite_plan.md)：AI 改写引擎主线，重点是文本 rewrite、profile、provider、fallback、trace。
> - **本文**：桌面 GUI / Windows 产品体验主线，重点是 Tauri Web UI、设置、keyring、托盘、窗口行为、打包、诊断和实机验收。

---

## 1. 目标

把当前可试用的 Tauri MVP 打磨成一个 Windows 上可长期常驻使用的桌面应用。

当前主链路已经跑通：

```text
按住快捷键 -> 录音 -> 本地 ASR -> 可选 AI rewrite -> 剪贴板 -> 粘贴
```

GUI 提升的目标不是重写核心引擎，而是补齐普通 Windows 用户会自然期待的桌面体验：

- 设置可视化、持久化、安全保存。
- 状态清楚：待机、录音、转写、改写、完成、失败。
- 错误可理解：缺模型、缺 key、麦克风失败、快捷键冲突、LLM fallback。
- 常驻可控：托盘、暂停、退出、窗口行为合理。
- 可交付：打包、日志、诊断、人工验收清单。

## 2. 不做什么

| 不做 | 原因 |
|---|---|
| 独立浏览器 Web app | 当前桌面端已经是 Tauri，前端就是 Web 技术栈；独立浏览器版会产生丢弃成本 |
| 重写 ASR / rewrite core | GUI 只接现有 core API 和事件，不复制业务逻辑 |
| 复杂主题系统 | 先保证 Windows 工具体验，视觉 polish 后置 |
| 读取当前 App 上下文 | 需要 UIA/accessibility 权限和适配，超出当前阶段 |
| 流式 token 到光标 | 当前产品交互是松开后一次性粘贴，流式会改变基本 UX |

## 3. 当前状态（2026-05-24）

已完成：

- Tauri v2 桌面壳已存在，前端位于 `apps/desktop/ui`。
- 桌面后端常驻运行：
  - 注册全局快捷键。
  - 调用麦克风录音。
  - 调用本地 Streaming Zipformer ASR。
  - 写剪贴板并模拟粘贴。
- 前端已显示：
  - 待机 / 录音 / 转写 / 改写 / 完成 / 出错状态。
  - 最近一次识别文本。
  - rewrite profile chip。
  - runtime error 文案。
- 设置面板已支持：
  - 模型目录。
  - 快捷键组合。
  - rewrite 开关。
  - rewrite profile/provider/model/timeout。
- Tauri command 已支持：
  - `get_config`
  - `save_config`
  - `get_rewrite_config`
  - `save_rewrite_config`
  - `start_runtime`
- Desktop runtime 已接入 rewrite：
  - rewrite disabled：ASR 原文直接粘贴。
  - rewrite enabled：ASR -> `Rewriting` -> rewrite engine -> 粘贴最终文本。
- `rewrite-result` event 已接入 GUI，multi variants 使用真实结果，不再使用 mock variants。

当前限制：

- API key 输入框还只是 shell，真实 key 仍走环境变量或 `.env`。
- rewrite / engine key 还没保存到 Windows Credential Manager。
- multi variants 只展示，点击复制 / 替换 / 重新粘贴闭环还没完成。
- fallback 原因和 trace 还没有专门 UI，只能通过 settings message / 日志间接看。
- 还没有托盘图标、暂停开关和退出菜单。
- 还没有正式打包 installer。
- 还缺 Windows 实机验收清单和记录。

## 4. GUI 架构原则

### 4.1 分层

```text
apps/desktop/ui
  只负责界面状态、设置表单、事件监听、轻量交互

apps/desktop/src-tauri
  负责 Tauri command/event、runtime 线程、系统能力接入

crates/voice-core
  负责录音、状态、配置、剪贴板、粘贴、ASR/rewrite glue

crates/voice-rewrite
  负责纯文本 AI 改写，不知道 GUI 存在
```

GUI 不直接实现 ASR 或 rewrite 规则。前端只接 command 和 event。

### 4.2 配置与密钥

- 普通配置写 `%APPDATA%\voice-flow\app.toml`。
- API key 不写 TOML 明文。
- 桌面端优先读 Windows Credential Manager。
- CLI / 测试仍可读 env / `.env`。

读取优先级：

```text
Windows Credential Manager -> process env -> .env
```

### 4.3 事件

GUI 应主要监听事件，而不是轮询 runtime：

- `realtime-state`：录音 / 转写 / 改写 / 完成 / 错误。
- `rewrite-result`：rewrite profile、main text、variants、fallback、error。
- 后续新增：
  - `rewrite-trace`
  - `runtime-diagnostics`
  - `settings-updated`

## 5. 里程碑

### G1：MVP GUI 闭环（已基本完成）

目标：让 GUI 可以操作当前主链路。

已完成：

- 状态显示。
- 最近文本显示。
- 模型目录 / 快捷键配置。
- rewrite 设置面板。
- Tauri command 读写配置。
- desktop runtime 接入 rewrite。
- multi variants 真实事件展示。

剩余小项：

- 设置保存后的提示文案更精确。
- 禁用状态和加载状态补齐。
- profile/provider/model 的非法组合提示。

### G2：安全密钥和 rewrite 结果操作

目标：让用户不需要命令行环境变量也能使用真实 LLM，并把 multi 结果操作闭环。

建议 PR：

| PR | 标题 | 单一职责 |
|---|---|---|
| G2.1 | `feat(desktop): add rewrite keyring commands` | Tauri 后端增加 provider key 的 get/save/delete command |
| G2.2 | `feat(desktop): store rewrite keys in credential manager` | Windows Credential Manager 持久化 DeepSeek/DashScope/OpenAI key |
| G2.3 | `feat(desktop): load rewrite keys for runtime` | runtime 构建 rewrite engine 时优先读 keyring |
| G2.4 | `feat(desktop): show rewrite key status` | GUI 显示 key 是否已保存，不回显明文 |
| G2.5 | `feat(desktop): copy rewrite variants` | multi tab 支持复制选中版本到剪贴板 |
| G2.6 | `feat(desktop): paste selected rewrite variant` | multi tab 支持重新粘贴选中版本 |

验收：

- 不设置环境变量，只在 GUI 保存 DeepSeek key，重启后 clean rewrite 可用。
- 清除 key 后，rewrite fallback，不影响 ASR 原文粘贴。
- multi 档返回 variants 后，点击任一 tab 可以复制或粘贴该版本。

### G3：错误、fallback、trace 和 latency

目标：用户知道系统这次做了什么、为什么 fallback、慢在哪里。

建议 PR：

| PR | 标题 | 单一职责 |
|---|---|---|
| G3.1 | `feat(desktop): emit rewrite trace events` | 后端把 `RewriteTrace` 转为 GUI event |
| G3.2 | `feat(desktop): show rewrite fallback reason` | GUI 显示缺 key、超时、JSON 解析失败、后处理拒绝等原因 |
| G3.3 | `feat(desktop): show latency breakdown` | 显示 ASR / rewrite / paste 粗略耗时 |
| G3.4 | `feat(desktop): add diagnostics panel` | 设置里增加诊断面板：模型目录、key 状态、runtime 状态 |
| G3.5 | `chore(desktop): write runtime logs` | 写本地日志文件，方便定位用户机器问题 |

验收：

- 断网或错误 key 时，GUI 明确显示 rewrite fallback，但仍粘贴可用文本。
- multi JSON 非法时，GUI 显示 fallback 原因。
- 一次录音完成后能看到 ASR 和 rewrite 耗时。
- 可从 GUI 打开日志目录。

### G4：Windows 常驻体验

目标：让应用像 Windows 常驻输入工具，而不是一个开发窗口。

建议 PR：

| PR | 标题 | 单一职责 |
|---|---|---|
| G4.1 | `feat(desktop): add tray icon` | 增加系统托盘图标 |
| G4.2 | `feat(desktop): add tray menu` | 托盘菜单：打开窗口、暂停/启用、退出 |
| G4.3 | `feat(desktop): support pause toggle` | 暂停时注销快捷键或忽略录音事件 |
| G4.4 | `feat(desktop): refine window focus behavior` | 减少悬浮窗抢焦点，保持 always-on-top 合理 |
| G4.5 | `feat(desktop): add compact status mode` | 常驻小窗只显示状态、profile 和最近文本摘要 |

验收：

- 关闭窗口不等于退出，应用进入托盘。
- 托盘可暂停/恢复。
- 暂停后快捷键不会触发录音。
- 从托盘退出能释放快捷键和音频资源。

### G5：快捷键、粘贴和 Windows 实机兼容

目标：把输入工具最容易出问题的 Windows 系统交互测扎实。

建议 PR：

| PR | 标题 | 单一职责 |
|---|---|---|
| G5.1 | `feat(desktop): validate hotkey settings` | 禁止空快捷键，提示非法组合 |
| G5.2 | `feat(desktop): surface hotkey registration errors` | 快捷键冲突时 GUI 明确提示 |
| G5.3 | `feat(desktop): improve paste failure handling` | 剪贴板或模拟粘贴失败时给出可恢复提示 |
| G5.4 | `docs(desktop): add Windows manual QA checklist` | 固化 Windows 手测清单 |
| G5.5 | `docs(desktop): record Windows app compatibility results` | 记录 Notepad/浏览器/微信/VS Code/Word 等测试结果 |

基础测试矩阵：

| 场景 | 应用 | 预期 |
|---|---|---|
| 纯 ASR | Notepad | 录音后自动粘贴 ASR 原文 |
| clean rewrite | 浏览器输入框 | 粘贴 clean 改写结果 |
| email rewrite | Outlook / Web Mail | 粘贴邮件正文 |
| wechat rewrite | 微信 / 企业微信 | 粘贴自然口吻文本 |
| prompt rewrite | VS Code / ChatGPT 页面 | 粘贴结构化 prompt |
| multi rewrite | Notepad / 浏览器 | 默认粘贴 clean，variants 可复制/粘贴 |
| 快捷键冲突 | 任意 | GUI 显示注册失败，不静默失效 |
| 断网 | 任意 | rewrite fallback，ASR 原文仍可粘贴 |

### G6：打包和发布

目标：从开发命令运行变成可安装、可复现的 Windows 应用。

建议 PR：

| PR | 标题 | 单一职责 |
|---|---|---|
| G6.1 | `chore(desktop): enable tauri bundling` | 打开 Tauri bundle 配置 |
| G6.2 | `chore(desktop): add app icons and metadata` | 补 product name、icon、version 信息 |
| G6.3 | `feat(desktop): first-run model directory check` | 首启检测模型目录并引导配置 |
| G6.4 | `docs(desktop): add packaging guide` | 写本地打包步骤 |
| G6.5 | `docs(desktop): add release smoke checklist` | installer 安装后 smoke test |

验收：

- 能构建 `.exe` 或 installer。
- 新机器安装后能打开 GUI。
- 模型目录缺失时有可理解提示。
- 安装后可完成一次 ASR-only 输入。
- 配置和 key 重启后保留。

## 6. 推荐执行顺序

当前最推荐从 G2 开始：

```text
G2.1 add rewrite keyring commands
G2.2 store rewrite keys in credential manager
G2.3 load rewrite keys for runtime
G2.4 show rewrite key status
G2.5 copy rewrite variants
G2.6 paste selected rewrite variant
G3.1 emit rewrite trace events
G3.2 show rewrite fallback reason
G3.3 show latency breakdown
G4.1 add tray icon
G4.2 add tray menu
G5.4 add Windows manual QA checklist
G6.1 enable tauri bundling
```

理由：

1. keyring 先做。否则 GUI 看起来能设置 rewrite，但真实 LLM 仍依赖命令行环境变量，桌面体验断裂。
2. variants 操作闭环紧跟。multi 是 GUI 最能体现差异化的地方，展示之后必须能复制/粘贴。
3. trace / fallback 再做。真实用户遇到缺 key、断网、超时时，需要明确知道系统仍然可用。
4. 托盘和打包后做。等主功能闭环稳定后再处理常驻和交付。

## 7. 验证命令

GUI 静态脚本检查：

```powershell
node --check apps/desktop/ui/main.js
```

Tauri 后端编译：

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe check --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Core / rewrite 回归：

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe test -p voice-core
& C:\Users\16867\.cargo\bin\cargo.exe test -p voice-rewrite
```

PowerShell 启动 GUI：

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe run --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Git Bash 启动 GUI：

```bash
/c/Users/16867/.cargo/bin/cargo.exe run --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## 8. 完成标准

达到下面标准时，可以认为 Windows 桌面 GUI 进入“可日常试用”状态：

- 不需要命令行环境变量即可在 GUI 保存和使用 LLM key。
- ASR-only 和 rewrite 两条链路都能在 Windows 常见输入框稳定粘贴。
- 用户能暂停、恢复、退出应用。
- 错误和 fallback 原因能在 GUI 中看懂。
- multi variants 能复制或重新粘贴。
- 配置、快捷键和 key 重启后保留。
- 有本地日志和基础诊断入口。
- 有明确的 Windows manual QA checklist 和至少一次实机记录。
- 能构建 Windows 可执行文件或 installer。
