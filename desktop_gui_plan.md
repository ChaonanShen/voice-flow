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

后续桌面 UI 明确成同一个语音写作引擎的两个出口，而不是两个产品：

```text
Audio Capture
-> ASR
-> Preprocess
-> Profile / Voice Command Resolver
-> AI Rewrite
-> Result
-> Output Adapter
```

两个 UI 模式共享同一条 pipeline。用户可见命名使用中文，括号内保留内部 / 旧文档名方便对应代码：

| 模式 | 定位 | Output Adapter |
|---|---|---|
| 文稿模式（Voice Pad） | 在 voice-flow 里写长文本、草稿、多版本对比 | Result -> 插入 / 替换文稿编辑区内容 |
| 悬浮窗模式（Floating Input） | 向当前外部应用输入，像系统级语音输入法 | Result -> clipboard -> simulated paste 到当前应用 |

GUI 提升的目标不是重写核心引擎，而是补齐普通 Windows 用户会自然期待的桌面体验：

- 设置可视化、持久化、安全保存。
- 状态清楚：待机、录音、转写、改写、完成、失败。
- 错误可理解：缺模型、缺 key、麦克风失败、快捷键冲突、LLM fallback。
- 双模式清楚：文稿模式是内部写作工作台，悬浮窗模式是外部输入状态面板。
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
  - rewrite provider key 的保存、清除和保存状态显示。
- Tauri command 已支持：
  - `get_config`
  - `save_config`
  - `get_rewrite_config`
  - `save_rewrite_config`
  - `start_runtime`
  - `get_rewrite_key_status`
  - `save_rewrite_key`
  - `delete_rewrite_key`
  - `get_diagnostics`
  - `get_pause_state`
  - `set_pause_state`
- Desktop runtime 已接入 rewrite：
  - rewrite disabled：ASR 原文直接粘贴。
  - rewrite enabled：ASR -> `Rewriting` -> rewrite engine -> 粘贴最终文本。
- `rewrite-result` event 已接入 GUI，multi variants 使用真实结果，不再使用 mock variants。
- GUI 已支持：
  - multi variants 切换、复制和重新粘贴。
  - fallback 原因和 ASR / rewrite / paste 粗略耗时展示。
  - Settings 里的 diagnostics 面板。
  - 托盘打开窗口、暂停 / 恢复、退出。
  - 悬浮窗模式小圆形麦克风悬浮态。
  - 文稿模式最小编辑区入口。
  - 悬浮窗模式 / 文稿模式右键模式切换菜单。

当前限制：

- 文稿模式目前只是最小 `textarea` 编辑区，还没有接入 `Result -> 文稿编辑区` output adapter。
- 文稿模式还没有完整展示“Last transcript -> Final text”差异、variants 切换和复制最终结果的工作台形态。
- runtime output adapter 仍以悬浮窗模式自动粘贴链路为主，尚未按模式切换外部粘贴 / 内部插入。
- rewrite trace 还没有独立事件或完整 trace 面板，目前只展示 fallback 和耗时摘要。
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

### 4.1.1 两种 UI 模式

桌面主窗口需要有清晰的模式切换：

```text
文稿模式 | 悬浮窗模式 | 设置
```

最小改动方案优先：继续使用原生 HTML / CSS / JS，不引入大型前端框架。模式切换只改变 UI 组织和 output adapter，不复制 ASR、rewrite、clipboard、paste 等核心逻辑。

#### 文稿模式（Voice Pad）

定位：软件本身像一个带 AI 语音输入能力的记事本。

适合：

- 长文本草稿。
- 邮件草稿。
- 会议纪要。
- prompt 草稿。
- 想法整理。
- 多版本对比后再复制或使用。

UI 要素：

- 一个主要文本编辑区，显示最终可用文本。
- 最近一次原始 ASR 文本。
- 当前 rewrite profile。
- rewrite variants：clean / polish / email / wechat / bullets / prompt 等。
- 用户能看到“原始转写 -> AI 改写后结果”的关系。
- 用户可以复制最终结果。
- 如果当前有 multi variants，可以切换查看不同版本。

暂时不要求复杂富文本，`textarea` 或 `contenteditable` 都可以。第一版重点是信息架构清晰。

#### 悬浮窗模式（Floating Input）

定位：系统级输入法感觉。用户焦点在 VS Code、邮箱、微信、飞书、浏览器输入框等外部软件里，按住快捷键说话，松开后自动转写、可选 rewrite，并粘贴到当前光标位置。

UI 要素：

- 紧凑悬浮窗。
- 当前状态：Idle / Recording / Transcribing / Rewriting / Completed / Error。
- 当前 profile chip。
- 最近一次输出文本摘要。
- rewrite fallback 的轻量提示，不打断用户。
- variants 可以有入口或简化 tabs，但悬浮窗不能变成重编辑器。

现有自动粘贴链路必须保持。新增文稿模式时，不能破坏悬浮窗模式的 `clipboard -> simulated paste` 行为。

### 4.1.2 Output Adapter 策略

当前 desktop runtime 默认在完成 ASR / rewrite 后自动调用 `paste_transcript`。要支持文稿模式，需要在 desktop 层新增一个很薄的 output mode 状态：

| output mode | 行为 |
|---|---|
| `floating_input` | 保持现状：Result -> clipboard -> simulated paste；继续发 `rewrite-result` / `realtime-state` 事件给 UI |
| `voice_pad` | 不自动粘贴到外部应用；发结果事件给 UI，由前端把最终文本插入 / 替换文稿编辑区 |

这个状态只属于 desktop shell，不进入 `voice-core` / `voice-rewrite`。ASR、rewrite、clipboard、paste 的实现不重写。

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
  - `desktop-output-result` 或等价事件：包含 raw transcript、final text、profile、variants、fallback、timings，用于文稿模式写入内部编辑器
  - `output-mode-updated`：GUI 模式切换时同步当前 output adapter
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

### G4.5a：双模式桌面 UI（计划新增）

目标：把桌面 UI 明确分成文稿模式（Voice Pad）和悬浮窗模式（Floating Input）两种形态，同时共用现有 pipeline。

建议 PR：

| PR | 标题 | 单一职责 |
|---|---|---|
| G4.5a.1 | `docs(desktop): plan dual desktop modes` | 记录文稿模式 / 悬浮窗模式的 UI 目标、output adapter 和验收标准 |
| G4.5a.2 | `feat(desktop): add output mode state` | desktop shell 增加 `voice_pad` / `floating_input` 状态和 command/event，不改 core pipeline |
| G4.5a.3 | `feat(desktop): add desktop mode navigation` | 主窗口增加文稿模式 / 悬浮窗模式 / 设置模式切换 |
| G4.5a.4 | `feat(desktop): add voice pad editor` | 文稿模式添加编辑区、last transcript、final text、copy 和 variants 展示 |
| G4.5a.4a | `feat(desktop): add minimal voice pad pane` | 先提供可编辑 textarea 和可切换目标，后续再接 output adapter 和 variants |
| G4.5a.5 | `feat(desktop): refine floating input panel` | 悬浮窗模式保留自动粘贴链路，压缩为状态、profile、摘要、fallback 入口 |
| G4.5a.5a | `feat(desktop): compact floating mic window` | 悬浮窗模式默认显示为小圆形麦克风悬浮窗，设置页按需展开 |
| G4.5a.5b | `feat(desktop): record from floating mic` | 小麦克风点击开始 / 结束录音，仍复用现有 push-to-talk pipeline |
| G4.5a.5c | `feat(desktop): simplify floating mic control` | 悬浮态只显示顶部拖拽小横杠和圆形麦克风，状态通过颜色 / 动效反馈 |
| G4.5a.5d | `feat(desktop): add mode context menu` | 悬浮窗模式 / 文稿模式右键原生菜单切换悬浮窗模式 / 文稿模式 / 设置 |
| G4.5a.6 | `docs(desktop): document dual mode usage` | README 或 GUI 文档补充两种模式说明 |

执行约束：

- 优先改 `apps/desktop/ui`。
- 只在必要时小幅修改 `apps/desktop/src-tauri/src/main.rs`，用于 output mode 状态和结果事件。
- 不重写 ASR、rewrite、clipboard、paste。
- 不引入大型前端框架。
- UI 要支持当前 460x360 窄窗口，避免文本溢出和元素重叠。
- 页面必须是实际工具界面，不做 marketing landing page。

验收：

- 打开桌面端后，可以清楚看到文稿模式和悬浮窗模式两种模式。
- 悬浮窗模式的按住说话、转写、rewrite、自动粘贴能力不被破坏。
- 文稿模式能展示最近一次 ASR / rewrite 结果，并能复制最终文本。
- multi variants 如果后端返回，至少文稿模式能清楚展示和切换。
- UI 不像配置面板堆叠，而像一个真实的语音写作工具。

实际进展（2026-05-24）：

- 已完成 G4.5a.3 的第一步：主窗口增加悬浮窗模式 / 设置切换，Settings 不再只是临时展开面板。
- 已完成 G4.5a.4a：新增最小文稿模式 pane 和 `textarea` 编辑区，使文稿模式成为真实可切换目标；暂未接入 Result -> 文稿编辑区的 output adapter。
- 已完成 G4.5a.5 的早期状态面板版本：悬浮窗模式曾压缩为状态、profile、输出目标、最近输出、fallback / latency 和 variants 入口；该形态随后被更轻的麦克风悬浮态替代。
- 已完成 G4.5a.5a：悬浮窗模式默认窗口缩小为 96x106 的透明小圆形麦克风悬浮窗；进入文稿模式时展开到 560x420，进入设置时展开到 460x360，返回悬浮窗模式时缩回。
- 已完成悬浮窗拖拽能力：小窗顶部拖拽条使用 Tauri `data-tauri-drag-region`，麦克风本体只负责点击开始 / 结束录音。
- 已完成 G4.5a.5b：小麦克风点击开始 / 结束录音，后端只向 runtime 注入 `PushToTalkEvent::Pressed / Released`，ASR、rewrite、clipboard、paste 仍走原有 pipeline；`Alt+Space` 快捷键链路保持可用。
- 已完成 G4.5a.5c：悬浮态 UI 只保留顶部小横杠和圆形麦克风；录音时红色脉冲，转写 / 改写时蓝色反馈，完成时绿色反馈，错误时红色反馈，不再显示长状态面板。
- 已完成 G4.5a.5d：悬浮窗模式和文稿模式中右键弹出 Tauri 原生模式菜单，可以切换到悬浮窗模式、文稿模式或设置。
- 悬浮窗模式设置为 non-focusable，目标是减少点击小窗时抢走外部输入框焦点；设置模式切回 focusable，保证设置表单可编辑。
- 文稿模式 / 设置模式切回 focusable；当前自动粘贴链路仍保持悬浮窗模式现状，尚未实现按模式切换 output adapter。

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
