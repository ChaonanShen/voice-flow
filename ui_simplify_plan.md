# UI 精简计划 (ui-simplify 分支)

目标：在不改后端核心逻辑的前提下，把桌面 UI 大幅瘦身。仅前端文件 + 极少量 Tauri 命令调整。

## 1. 文稿模式 (voice-pad) — 只保留文稿编辑区

### 1.1 HTML 删除（[apps/desktop/ui/index.html](apps/desktop/ui/index.html)）

`#document-pane` 下整段精简，删除：

- `<section class="document-card document-card-transcript">` 整块（"最近转写"卡片）
  - 标签 `<p class="label">最近转写</p>`
  - chip 行：`#document-output-chip`、`#document-rewrite-chip`
  - 文本：`#document-last-text`
  - 结果元信息：`#document-result-meta`、`#document-fallback-reason`、`#document-latency-summary`
  - trace 按钮：`#document-trace-toggle`
- `<section class="document-card document-card-diff">` 整块（"转写与输出"卡片）
  - `#document-profile-chip`、`#document-diff-raw`、`#document-diff-final`
- `<div class="document-variant-panel">` 整块（改写候选面板，入口在被删的卡片里，连带去掉）
  - `#document-variant-panel`、`.document-variant-tab`、`#document-variant-text`、`#document-copy-variant`、`#document-paste-variant`、`#document-variant-action-status`
- `<section id="document-trace-overlay">` 整块（trace 浮层，入口已被删）
  - `#document-trace-close`、`#document-trace-profile`、`#document-trace-preprocess`、`#document-trace-llm`、`#document-trace-fallback`、`#document-trace-error`

保留：

- 顶部 `.document-status-row`（状态点 + 状态文字 + 暂停 / 设置 / 关闭按钮）
- `#document-startup-notice`
- `.document-card.document-card-editor`（文稿编辑区）— **核心**
- `.document-runtime-error`（运行错误提示）

文稿编辑区内部行为也调整：

- 当前三个 apply mode 按钮（插入光标 / 替换全文 / 追加末尾）配合"语音结果写入"的语义。删除"最近转写"后语音流仍会触发 `apply_text_to_document_editor`，因此 **apply-mode 按钮要保留**，文稿模式下"按住说话松开" → 文本按当前 apply mode 写入 textarea。
- 工具栏中删除 `#document-copy-final`（最终输出不再独立暴露），保留 `#document-copy-editor`、`#document-clear-editor`。

### 1.2 JS 调整（[apps/desktop/ui/main.js](apps/desktop/ui/main.js)）

删除以下 DOM 句柄声明与所有引用：

```
lastTranscript, rewriteChip, outputChip, profileChip,
resultMeta, fallbackReason, latencySummary,
traceToggle, diffRaw, diffFinal,
variantPanel, variantText, variantTabs, copyVariant, pasteVariant, variantActionStatus,
traceOverlay, traceClose, traceProfile, tracePreprocess, traceLlm, traceFallback, traceError,
copyFinal
```

随之删除/精简这些函数：

- `renderTraceOverlay`、`applyRewriteTrace`、`updateResultMeta`、`updateVariantPanel`、`switchVariantTab`、`writeSelectedVariant` 全部删除
- `renderDocumentMode` 只保留：状态点、状态文字、startup notice、runtime error、暂停按钮、textarea 内容/状态/apply-mode 提示
- `applyRewriteResult`：仍要更新 `store.documentText` / `store.finalText` 以驱动文稿写入逻辑，但去掉 `updateResultMeta` / `updateVariantPanel` 调用
- `applyDesktopOutputResult`：保留写入文稿编辑器的逻辑 (`applyTextToDocumentEditor`)，去掉 chip/variants/trace/result-meta 更新
- 移除 `await listen("rewrite-trace", ...)` 监听器（trace 不再展示）
- `boot()` 中删除 traceToggle/traceClose 事件绑定

`store` 字段裁剪：删 `rewriteVariants`、`activeVariant`、`rawTranscript`、`finalText`（如果仅用于已删 UI）、`resultProfile`、`resultMeta`、`trace`、`traceVisible`。保留 `documentText`（用于 startup/error 时态可观察）只在必要时保留——审查后删 `documentText`、`rawTranscript`，仅留 `documentEditorText` + `editorStatus` + apply-mode。

### 1.3 CSS 清理（[apps/desktop/ui/styles.css](apps/desktop/ui/styles.css)）

删除与上述被移除 DOM 相关的样式块（共 39 处命中，按类名搜索逐个清理）：

- `.document-card-transcript`、`.document-transcript-meta`、`.document-chip-row`、`#document-last-text` 相关
- `.document-card-diff`、`.document-diff-header`、`.document-diff-grid`、`.document-diff-block`、`.document-diff-label`、`.document-diff-content`
- `.document-result-meta`、`#document-fallback-reason`、`#document-latency-summary`
- `.document-trace-actions`、`.document-trace-overlay`、`.document-trace-card`、`.document-trace-header`、`.document-trace-grid`
- `.document-variant-panel`、`.document-variant-tabs`、`.document-variant-tab`、`.document-variant-actions`、`#document-variant-text`
- 保留：`.document-card-editor`、`.document-editor-header`、`.document-editor-hint`、`.document-editor-actions`、`.document-editor-mode`、`.document-editor`、`.document-editor-toolbar`、`#document-editor-status`

---

## 2. 设置 — 输入页

### 2.1 删除"模型目录"

[apps/desktop/ui/index.html](apps/desktop/ui/index.html) 中删除：

```html
<label class="field">
  <span>模型目录</span>
  <input id="settings-model-dir" type="text" spellcheck="false" />
</label>
```

[apps/desktop/ui/main.js](apps/desktop/ui/main.js) 中：

- 删 `modelDirInput` 句柄
- `applyConfig`：删 `modelDirInput.value = store.config.model_dir ?? "";`
- `readConfig`：`model_dir` 字段固定回传 `store.config.model_dir`（即从后端读到什么就写回什么），让后端 `default_model_dir()`/`with_default_model_dir` 继续生效，UI 不暴露这一项。

后端 [main.rs](apps/desktop/src-tauri/src/main.rs) 与 [config.rs](crates/voice-core/src/config.rs) **不动** —— `model_dir` 字段仍存在 app.toml，仍由 `with_default_model_dir` 注入默认值。

### 2.2 快捷键 UI 改为单输入框

HTML 把 `.hotkey-grid` 整块替换为：

```html
<label class="field">
  <span>快捷键</span>
  <input id="settings-hotkey-combo" type="text" spellcheck="false" placeholder="例如：alt+space" />
</label>
<p class="pane-note">支持 ctrl/alt/shift/win 组合，最后一段是主键。例：alt+space、ctrl+shift+r。</p>
```

JS 重写解析层（保持后端 `HotkeyConfig` 结构体不变）：

```js
const HOTKEY_MOD_MAP = {
  ctrl: "ctrl",
  control: "ctrl",
  alt: "alt",
  option: "alt",
  shift: "shift",
  win: "logo",
  super: "logo",
  meta: "logo",
  cmd: "logo",
  command: "logo",
};

function parseHotkeyCombo(combo) {
  const parts = combo.split("+").map((s) => s.trim()).filter(Boolean);
  if (parts.length === 0) return { error: "快捷键不能为空" };
  const flags = { ctrl: false, alt: false, shift: false, logo: false };
  const keyToken = parts[parts.length - 1];
  for (const token of parts.slice(0, -1)) {
    const slot = HOTKEY_MOD_MAP[token.toLowerCase()];
    if (!slot) return { error: `未知修饰键: ${token}` };
    flags[slot] = true;
  }
  if (!flags.ctrl && !flags.alt && !flags.shift && !flags.logo) {
    return { error: "至少需要一个修饰键" };
  }
  if (!keyToken) return { error: "缺少主键" };
  return { config: { ...flags, key: normalizeHotkeyKey(keyToken) } };
}

function formatHotkeyCombo(cfg) {
  const parts = [];
  if (cfg.ctrl) parts.push("ctrl");
  if (cfg.alt) parts.push("alt");
  if (cfg.shift) parts.push("shift");
  if (cfg.logo) parts.push("win");
  parts.push((cfg.key ?? "space").toLowerCase());
  return parts.join("+");
}

function normalizeHotkeyKey(token) {
  // 单字母转大写，"space"→"Space"（首字大写），其余保持原样。后端 to_label 走 global-hotkey 解析。
  if (/^[a-zA-Z]$/.test(token)) return token.toUpperCase();
  return token.charAt(0).toUpperCase() + token.slice(1).toLowerCase();
}
```

`applyConfig` 中：删除 `hotkeyCtrl/Alt/Shift/Logo/Key` 句柄使用，改成 `hotkeyCombo.value = formatHotkeyCombo(store.config.hotkey)`。

`readConfig` 中：调用 `parseHotkeyCombo(hotkeyCombo.value.trim())`，失败时让 `validateHotkeyForm` 接管返回错误信息，成功时把解析出的字段塞入 `hotkey: {...}`。

`validateHotkeyForm` 改为：调用 `parseHotkeyCombo`，错误时返回错误字符串；正确时返回 null。

后端 `validate_hotkey_config`（Rust 侧）保持不变，仍校验"至少一个修饰键 + 主键非空"，作为双保险。

### 2.3 ASR 引擎单选保留

`ASR 引擎 Local / Cloud` 保留。Cloud 仍可填 DashScope key（`#settings-asr-cloud-fields`）。

---

## 3. 设置 — 改写页

HTML 删除：

- "Provider" `.field`（含三个 `<input name="settings-rewrite-provider">`）
- "模型" `<label class="field">` 输入框（`#settings-rewrite-model`）
- `.rewrite-summary` 中的 `#settings-rewrite-provider-summary` span（保留 profile + key summary）

"默认档" → 改名为 **"改写风格"**（建议命名理由：与"悬浮窗模式/文稿模式"的"模式"语义区分；"档"对用户不直观；"风格"明确传达这是改写文本的语气/格式选项）。

JS 调整：

- 删句柄 `rewriteModel`、`rewriteProviderSummary`、和所有 `currentRewriteProvider`、`setRewriteProvider` 内 DOM 写入；保留 `currentRewriteProvider` 返回 `store.config.rewrite.provider`（沿用后端读到的 provider）
- `readRewriteConfig`：`provider` 仍从 `store.config.rewrite.provider` 取（不让用户改）；`model: null`（让后端走 provider 默认模型 `DEFAULT_REWRITE_MODEL`）
- `applyRewriteConfig`：不再写 model 输入框；`updateRewriteSummary` 删除 provider 显示，但 key status / key env 文案保留（用户仍需知道当前会回退到哪个环境变量）
- `refreshRewriteKeyStatus`：传 `store.config.rewrite.provider` 即可

后端 [main.rs](apps/desktop/src-tauri/src/main.rs)、[config.rs](crates/voice-core/src/config.rs) **不动** —— provider/model 字段仍在 app.toml 持久化，仅 UI 隐藏。如果将来想换 provider，可直接编辑 app.toml。

API key 输入框、清除按钮、key 保存提示 **保留**。

`profile` 下拉保留（multi 选项可保留，但因 variants 面板被删，multi 实际效果等于 clean 主结果；这是接受的取舍）。

---

## 4. 设置 — 诊断页

直接删除：

### 4.1 HTML

- 设置 tabs 中第三个 `<button data-settings-tab="diagnostics">`
- `<div class="settings-pane" data-settings-pane="diagnostics">` 整块
- 关联 ID：`#settings-diagnostics-grid`、`#settings-diag-config-path`、`#settings-diag-log-path`、`#settings-diag-model-dir`、`#settings-diag-runtime`、`#settings-diag-asr`、`#settings-diag-rewrite-key`、`#settings-refresh-diagnostics`、`#settings-open-logs`

### 4.2 JS

- 删句柄 `refreshDiagnostics`、`openLogs`、`diagConfigPath`、`diagLogPath`、`diagModelDir`、`diagRuntime`、`diagAsr`、`diagRewriteKey`
- 删函数 `refreshDiagnosticsPanel`
- `switchSettingsTab`：删 `if (tab === "diagnostics") refreshDiagnosticsPanel();`
- `setMode`：删同上分支
- `applyPauseState`：删 `if (store.settingsTab === "diagnostics") refreshDiagnosticsPanel();`
- 启动通知 `store.startupNotice` 当前依赖 `get_diagnostics` 中 `model_dir_exists` 计算。由于诊断面板删了，要么：
  - **(a) 删除启动 notice 机制**（最简，建议）：删 `#document-startup-notice` 相关 DOM 与 JS。
  - (b) 仍保留启动 notice，但改为 `boot()` 中单次调用 `get_diagnostics` 取 `model_dir_exists`。

建议选 (a)，因为 `with_default_model_dir` 已经会在首次启动写入默认路径，model_dir 缺失这一兜底现实中只发生在用户手改 toml 错配的情况，可以让 ASR engine 启动失败时通过 runtime-error 暴露。

### 4.3 后端

[apps/desktop/src-tauri/src/main.rs](apps/desktop/src-tauri/src/main.rs) 中保留 `get_diagnostics` 和 `open_log_directory` tauri 命令本身（**不必删**，无副作用，未来日志/诊断 CLI 可能要用）。也可同步删除以减少代码量——可选项。

**优先简单**：先只删 UI 入口，命令保留。

### 4.4 CSS

删除 `.diagnostics-grid`、`.settings-diagnostics-actions` 样式块。

---

## 5. 文档/记忆

- 不动 [CLAUDE.md](CLAUDE.md)（约束仍然有效）
- 不动 [plan.md](plan.md)（产品路线图，与本次裁剪不冲突）
- 不动 voice-core / voice-cli / voice-rewrite / voice-asr-* 任何 Rust 代码（仅 desktop 壳 main.rs 不动也通过）

## 6. 验收（必须在 Windows 真机跑）

1. `cargo test --workspace` 仍通过（不应有任何变化，因为 Rust 代码完全没碰）
2. `cargo tauri dev` (apps/desktop) 启动后：
   - 悬浮窗模式：麦克风按钮、Alt+Space 按住说话 → 松开自动粘贴到外部应用 ✓
   - 文稿模式：按 Alt+Space → 文本按当前 apply mode 写入文稿编辑器；可手动打字编辑；复制/清空按钮工作 ✓
   - 设置 → 输入：快捷键输入框 `alt+space` 保存成功，重启或重载后仍生效；尝试 `ctrl+shift+r` 也能解析 ✓
   - 设置 → 输入：填错（如 `alt`、`alt+`、`xxx+space`）有错误提示 ✓
   - 设置 → 改写：开启改写后，clean profile 走默认 provider/model（DeepSeek/deepseek-chat），API key 状态显示正确 ✓
   - 设置 → 没有"诊断"tab ✓

## 7. 实施顺序

1. **改后端解析** —— 无需，方案 A 后端零改动
2. HTML 删除三块卡片 + 诊断 tab + provider/model 字段
3. HTML 替换 hotkey-grid 为单输入框
4. JS 删句柄与未使用函数
5. JS 加入 `parseHotkeyCombo` / `formatHotkeyCombo`，改写 `applyConfig` / `readConfig` / `validateHotkeyForm`
6. CSS 清理孤儿样式
7. Windows 真机走一遍 §6 验收
