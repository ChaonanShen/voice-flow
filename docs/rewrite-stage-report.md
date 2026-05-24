# AI 改写阶段记录（2026-05-24）

本文记录当前阶段已经完成的语音识别 + AI 改写能力、运行方式、测试方式和剩余事项。更详细的长期计划仍以 [`ai_rewrite_plan.md`](../ai_rewrite_plan.md) 为准。

## 当前结论

当前已经跑通：

```text
按住快捷键录音 -> 本地 ASR -> 可选 AI rewrite -> 剪贴板 -> 粘贴
```

默认不开 rewrite 时，桌面端仍粘贴 ASR 原文；打开 rewrite 后，desktop runtime 会进入 `Rewriting` 状态，调用当前 rewrite 配置对应的引擎，把最终文本粘贴到当前输入框。

## 已完成能力

### 1. Rewrite 引擎

- 新增 `crates/voice-rewrite`，保持纯文本域：输入 `&str`，输出 `RewriteResult`，不依赖音频采集、ASR、剪贴板或 GUI。
- 已支持 profile：`off`、`clean`、`polish`、`email`、`wechat`、`bullets`、`commit`、`prompt`、`multi`。
- 已支持 provider：DeepSeek、DashScope、OpenAI，统一走 OpenAI-compatible HTTP client。
- 已实现本地预处理：
  - 去中文口头禅。
  - 用户词典替换。
  - 语音命令识别，例如“写成邮件”“改正式一点”等会覆盖默认 profile。
- 已实现 multi profile：
  - LLM 返回 JSON。
  - 解析 `clean`、`polish`、`wechat`、`bullets` variants。
  - `main` 使用 `clean`。
- 已实现兜底策略：
  - API key 缺失、LLM 错误、超时、非法 JSON、后处理校验失败时，回退到预处理后的文本。
  - 后处理会保护数字和英文专有名词，避免改写时丢掉关键信息。

### 2. Core / CLI 接入

- `voice-core::config::AppConfig` 增加 `[rewrite]` 配置段。
- `voice-core::state::RealtimeState` 增加 `Rewriting`。
- 新增 `voice_core::text_pipeline::rewrite_settings_from_config`，把 core config 转成 rewrite engine settings。
- CLI 已支持：
  - `voice-cli rewrite`：纯文本输入到 rewrite 输出，调试最快。
  - `voice-cli transcribe --rewrite <profile>`：WAV -> ASR -> rewrite。
  - `voice-cli push-to-talk-transcribe --rewrite <profile>`：按住说话实时链路接入 rewrite。

### 3. Desktop GUI / Runtime 接入

- Desktop 使用 Tauri 内 Web UI，不另做独立浏览器版 GUI。
- `apps/desktop/ui` 已有 rewrite 设置面板：
  - rewrite 开关。
  - profile 选择。
  - provider 选择。
  - model 输入。
  - timeout 输入。
  - API key 输入框目前是 UI shell，真实 key 暂时走环境变量或 `.env`。
- Tauri command 已接入：
  - `get_rewrite_config`
  - `save_rewrite_config`
  - 读写 `app.toml [rewrite]`
- Desktop runtime 已接入真实 rewrite pipeline：
  - 未启用 rewrite：`ASR -> paste raw transcript`
  - 启用 rewrite：`ASR -> Rewriting -> rewrite engine -> paste final text`
- GUI 已显示：
  - 当前 rewrite profile chip。
  - `Rewriting` 状态。
  - multi profile 的真实 variants tab，不再使用 mock variants。
- 后端新增 `rewrite-result` 事件，前端用该事件展示真实 `RewriteResult::variants`。

## 关键提交

近期与本阶段直接相关的提交：

```text
744739a docs(rewrite): update desktop gui progress
ac3d5cf feat(desktop): surface rewrite results
843ed61 feat(desktop): wire rewrite into runtime
ae45a02 feat(desktop): add mock rewrite variant tabs
be9e02d feat(desktop): show rewrite profile status
c70c668 feat(desktop): wire rewrite settings commands
8eccd91 feat(desktop): add rewrite settings web ui
698fb99 feat(cli): add rewrite flags to push-to-talk
750d0c4 feat(core): add rewrite text pipeline glue
13051ad feat(core): add rewrite config schema
```

## 运行方式

### PowerShell 启动 GUI

在仓库根目录：

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe run --manifest-path apps/desktop/src-tauri/Cargo.toml
```

带 DeepSeek key：

```powershell
$env:DEEPSEEK_API_KEY="你的key"
& C:\Users\16867\.cargo\bin\cargo.exe run --manifest-path apps/desktop/src-tauri/Cargo.toml
```

### Git Bash 启动 GUI

在仓库根目录：

```bash
/c/Users/16867/.cargo/bin/cargo.exe run --manifest-path apps/desktop/src-tauri/Cargo.toml
```

带 DeepSeek key：

```bash
export DEEPSEEK_API_KEY="你的key"
/c/Users/16867/.cargo/bin/cargo.exe run --manifest-path apps/desktop/src-tauri/Cargo.toml
```

如果 `cargo` 已在 PATH，也可以直接：

```bash
cargo run --manifest-path apps/desktop/src-tauri/Cargo.toml
```

GUI 默认快捷键：

```text
Alt + Space
```

按住说话，松开后转写、可选改写并粘贴到当前焦点输入框。

### CLI rewrite 调试

```bash
echo "嗯，跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我，那个语气正式一点。" \
  | cargo run -p voice-cli -- rewrite --profile clean --provider deepseek
```

multi profile：

```bash
echo "嗯我今天下午可能因为地铁晚点会晚到十分钟，让老师不要等我" \
  | cargo run -p voice-cli -- rewrite --profile multi --provider deepseek
```

WAV -> ASR -> rewrite：

```bash
cargo run -p voice-cli -- transcribe docs/fixtures/demo-rewrite.wav --rewrite clean --rewrite-provider deepseek
```

## 已执行测试记录

### 本地单元 / 集成测试

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe test -p voice-core
```

结果：

```text
40 passed; 0 failed
```

覆盖重点：

- config 读写。
- `RealtimeState` 序列化。
- push-to-talk 状态机。
- `voice_core::text_pipeline` glue。
- rewrite config -> rewrite settings。

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe test -p voice-rewrite
```

结果：

```text
40 unit tests passed; 0 failed
3 live DeepSeek tests ignored by default
3 text rewrite case tests passed
```

覆盖重点：

- profile parse / label。
- filler removal。
- user dictionary。
- voice command detection。
- OpenAI-compatible client request behavior。
- 5xx retry。
- clean / polish / email / wechat / bullets / commit / prompt / multi pipeline。
- multi JSON parse。
- LLM error fallback。
- invalid JSON fallback。
- 数字和专有名词保护。

### Desktop 编译检查

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe check --manifest-path apps/desktop/src-tauri/Cargo.toml
```

结果：

```text
Finished dev profile
```

覆盖重点：

- Tauri command 编译。
- desktop runtime 调用 rewrite engine。
- `rewrite-result` event payload 类型。
- desktop Cargo 依赖和 lockfile。

### Desktop 前端语法检查

```powershell
node --check apps/desktop/ui/main.js
```

结果：无语法错误。

覆盖重点：

- rewrite 设置 UI 脚本语法。
- `realtime-state` / `rewrite-result` / `runtime-error` 事件处理脚本语法。
- multi variants 状态更新脚本语法。

### Live LLM 测试

已有 ignored live tests，默认不跑，避免 CI 或普通本地测试误调用真实 API。

手动运行：

```powershell
$env:DEEPSEEK_API_KEY="你的key"
& C:\Users\16867\.cargo\bin\cargo.exe test -p voice-rewrite --test live_deepseek_examples -- --ignored --nocapture --test-threads=1
```

预期：

- `live_deepseek_clean_smoke`：真实 clean 改写成功且不 fallback。
- `live_deepseek_multi_smoke`：真实 multi JSON 返回 4 个 variants。
- `live_deepseek_timeout_falls_back`：超时时走 fallback。

## 当前限制

- GUI 的 API key 输入框暂未持久化到 Windows Credential Manager；当前真实 LLM key 通过环境变量或 `.env` 读取。
- Desktop 设置保存后当前通过 restart runtime session 生效，还不是更细粒度的热更新。
- multi variants 已能展示真实结果，但点击某个 variant 后复制 / 替换粘贴文本的交互还没有做完整。
- 尚未完成 Windows 实机 checklist 记录：真实麦克风、全局快捷键、剪贴板粘贴、真实 LLM、不同 app 输入框的完整组合测试。
- GUI 还没有展示 rewrite trace / latency breakdown。

## 下一步建议

GUI / Windows 桌面体验的后续提升已独立整理到 [`desktop_gui_plan.md`](../desktop_gui_plan.md)。近期建议按该文档的 G2 -> G3 -> G4 顺序推进：

1. 做 Windows Credential Manager keyring 接入，解决 GUI API key 输入框只是 shell 的问题。
2. 完成 multi variants 的用户操作闭环：点击候选后复制或替换当前结果。
3. 加 rewrite trace / latency UI，展示 ASR、rewrite、fallback 原因和耗时。
4. 补 desktop 实机验收清单，并记录一次真实运行结果。
5. 做托盘、暂停、日志诊断和 Tauri 打包。
