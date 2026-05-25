const labels = {
  idle: "待机",
  recording: "录音中",
  transcribing: "转写中",
  rewriting: "改写中",
  completed: "完成",
  error: "出错",
};

const rewriteDefaults = {
  enabled: false,
  provider: "deepseek",
  model: "deepseek-chat",
  default_profile: "clean",
  timeout_ms: 4000,
  user_dictionary: {},
};

const providerDefaults = {
  deepseek: {
    model: "deepseek-chat",
    env: "DEEPSEEK_API_KEY",
  },
  dashscope: {
    model: "qwen-plus",
    env: "DASHSCOPE_API_KEY",
  },
  openai: {
    model: "",
    env: "OPENAI_API_KEY",
  },
};

const configDefaults = {
  model_dir: null,
  asr: {
    engine: "local",
  },
  hotkey: {
    ctrl: false,
    alt: true,
    shift: false,
    logo: false,
    key: "Space",
  },
  rewrite: rewriteDefaults,
};

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

const dot = document.querySelector("#document-status-dot");
const stateLabel = document.querySelector("#document-state-label");
const documentEditor = document.querySelector("#document-editor");
const documentEditorStatus = document.querySelector("#document-editor-status");
const documentEditorModeHint = document.querySelector("#document-editor-mode-hint");
const documentApplyModeButtons = document.querySelectorAll("[data-document-apply-mode]");
const copyEditor = document.querySelector("#document-copy-editor");
const clearEditor = document.querySelector("#document-clear-editor");
const micButton = document.querySelector("#floating-mic-button");
const settingsClose = document.querySelector("#settings-close");
const runtimeError = document.querySelector("#document-runtime-error");
const runtimeErrorText = document.querySelector("#document-runtime-error-text");
const pauseToggle = document.querySelector("#document-pause-toggle");
const settingsToggle = document.querySelector("#document-settings-toggle");
const documentCloseApp = document.querySelector("#document-close-app");
const settingsMessage = document.querySelector("#settings-message");
const modeTabs = document.querySelectorAll("[data-mode-tab]");
const modePanes = document.querySelectorAll("[data-mode-pane]");
const asrCloudFields = document.querySelector("#settings-asr-cloud-fields");
const asrApiKey = document.querySelector("#settings-asr-api-key");
const clearAsrKey = document.querySelector("#settings-clear-asr-key");
const asrKeyStatus = document.querySelector("#settings-asr-key-status");
const hotkeyCombo = document.querySelector("#settings-hotkey-combo");
const saveSettings = document.querySelector("#settings-save");
const settingsTabs = document.querySelectorAll("[data-settings-tab]");
const settingsPanes = document.querySelectorAll("[data-settings-pane]");
const rewriteEnabled = document.querySelector("#settings-rewrite-enabled");
const rewriteEnabledLabel = document.querySelector("#settings-rewrite-enabled-label");
const rewriteProfile = document.querySelector("#settings-rewrite-profile");
const rewriteProfileSummary = document.querySelector("#settings-rewrite-profile-summary");
const rewriteKeySummary = document.querySelector("#settings-rewrite-key-summary");

const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;
const NativeMenu = window.__TAURI__?.menu?.Menu;
const NativeMenuItem = window.__TAURI__?.menu?.MenuItem;
const appWindow = window.__TAURI__?.window?.getCurrentWindow?.();
const LogicalSize = window.__TAURI__?.dpi?.LogicalSize;
/* 悬浮窗页面/文稿页面/设置页面的尺寸 */
const windowSizes = {
  floating: { width: 110, height: 110 },
  "voice-pad": { width: 720, height: 720 },
  settings: { width: 520, height: 400 },
};
const contentModes = new Set(["floating", "voice-pad"]);
const store = {
  config: normalizeConfig(configDefaults),
  settingsTab: "input",
  asrKeySaved: false,
  paused: false,
  mode: "floating",
  previousContentMode: "floating",
  outputMode: "floating_input",
  currentState: "idle",
  manualRecording: false,
  documentEditorText: "",
  documentApplyMode: "insert",
  runtimeError: "",
  editorStatus: "",
  settingsMessage: "",
};
let activeContextMenu = null;

function renderFloatingMode() {
  const state = store.manualRecording ? "recording" : store.currentState;
  micButton.dataset.state = store.paused ? "idle" : state;
  micButton.dataset.paused = String(store.paused);
  micButton.dataset.manualRecording = String(store.manualRecording);
  micButton.title = store.paused
    ? "监听已暂停"
    : store.manualRecording
      ? "点击结束录音"
      : "点击开始/结束；Alt+Space 按住说话";
}

function renderDocumentMode() {
  dot.dataset.state = store.paused ? "idle" : store.currentState;
  stateLabel.textContent = store.paused
    ? "已暂停"
    : labels[store.currentState] ?? labels.idle;
  runtimeError.hidden = !store.runtimeError;
  runtimeErrorText.textContent = store.runtimeError;
  pauseToggle.dataset.active = String(store.paused);
  pauseToggle.title = store.paused ? "恢复监听" : "暂停监听";
  pauseToggle.querySelector("span").textContent = store.paused ? ">" : "||";
  documentEditor.value = store.documentEditorText;
  documentEditorStatus.textContent = store.editorStatus ?? "";
  documentApplyModeButtons.forEach((button) => {
    const active = button.dataset.documentApplyMode === store.documentApplyMode;
    button.classList.toggle("is-active", active);
  });
  documentEditorModeHint.textContent = documentApplyModeHint(store.documentApplyMode);
}

function renderSettingsView() {
  settingsMessage.textContent = store.settingsMessage;
}

function renderRuntimeViews() {
  renderFloatingMode();
  renderDocumentMode();
  renderSettingsView();
}

function renderModeVisibility() {
  modeTabs.forEach((button) => {
    const active = button.dataset.modeTab === store.mode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", String(active));
  });
  modePanes.forEach((pane) => {
    pane.hidden = pane.dataset.modePane !== store.mode;
  });
}

function applyState(event) {
  const state = event?.state ?? "idle";
  store.currentState = state;
  if (state !== "recording" && store.manualRecording) {
    setManualRecording(false);
  } else {
    renderFloatingMode();
  }

  if (state !== "error") {
    store.runtimeError = "";
  }

  if (event?.error) {
    const message = String(event.error);
    store.runtimeError = message;
    store.settingsMessage = message;
  }
  renderRuntimeViews();
}

function applyPauseState(event) {
  store.paused = Boolean(event?.paused);
  if (store.paused) {
    setManualRecording(false);
  }
  renderRuntimeViews();
}

function setManualRecording(active) {
  store.manualRecording = Boolean(active);
  renderFloatingMode();
}

function applyRewriteResult(result) {
  if (result?.fallback && result.error) {
    store.settingsMessage = result.error;
  }
  renderRuntimeViews();
}

function applyDesktopOutputResult(result) {
  if (!result) {
    return;
  }

  store.outputMode = result.output_mode ?? store.outputMode;
  if (result.output_mode === "voice_pad" && result.final_text) {
    applyTextToDocumentEditor(result.final_text);
  }
  if (result.output_mode === "floating_input") {
    store.editorStatus = result.pasted_to_external
      ? "已输出到外部应用"
      : "未写入外部应用";
  }
  renderRuntimeViews();
}

function normalizeConfig(config) {
  return {
    ...configDefaults,
    ...(config ?? {}),
    asr: {
      ...configDefaults.asr,
      ...(config?.asr ?? {}),
    },
    hotkey: {
      ...configDefaults.hotkey,
      ...(config?.hotkey ?? {}),
    },
    rewrite: {
      ...rewriteDefaults,
      ...(config?.rewrite ?? {}),
    },
  };
}

function applyConfig(config) {
  store.config = normalizeConfig(config);
  setAsrEngine(store.config.asr.engine ?? "local");
  hotkeyCombo.value = formatHotkeyCombo(store.config.hotkey);
  applyRewriteConfig(store.config.rewrite);
  updateAsrSummary();
}

function readConfig() {
  const parsed = parseHotkeyCombo(hotkeyCombo.value.trim());
  return {
    model_dir: store.config.model_dir ?? null,
    asr: {
      engine: currentAsrEngine(),
    },
    hotkey: parsed.config ?? store.config.hotkey,
    rewrite: store.config.rewrite,
  };
}

function parseHotkeyCombo(combo) {
  const parts = combo
    .split("+")
    .map((s) => s.trim())
    .filter(Boolean);
  if (parts.length === 0) {
    return { error: "快捷键不能为空" };
  }
  const flags = { ctrl: false, alt: false, shift: false, logo: false };
  const keyToken = parts[parts.length - 1];
  for (const token of parts.slice(0, -1)) {
    const slot = HOTKEY_MOD_MAP[token.toLowerCase()];
    if (!slot) {
      return { error: `未知修饰键: ${token}` };
    }
    flags[slot] = true;
  }
  if (!flags.ctrl && !flags.alt && !flags.shift && !flags.logo) {
    return { error: "至少需要一个修饰键" };
  }
  if (!keyToken) {
    return { error: "缺少主键" };
  }
  if (HOTKEY_MOD_MAP[keyToken.toLowerCase()]) {
    return { error: "主键不能是修饰键" };
  }
  return { config: { ...flags, key: normalizeHotkeyKey(keyToken) } };
}

function formatHotkeyCombo(config) {
  const parts = [];
  if (config.ctrl) parts.push("ctrl");
  if (config.alt) parts.push("alt");
  if (config.shift) parts.push("shift");
  if (config.logo) parts.push("win");
  parts.push((config.key ?? "Space").toLowerCase());
  return parts.join("+");
}

function normalizeHotkeyKey(token) {
  if (/^[a-zA-Z]$/.test(token)) {
    return token.toUpperCase();
  }
  return token.charAt(0).toUpperCase() + token.slice(1).toLowerCase();
}

function validateHotkeyForm() {
  const parsed = parseHotkeyCombo(hotkeyCombo.value.trim());
  return parsed.error ?? null;
}

function currentRewriteProvider() {
  return store.config.rewrite.provider ?? rewriteDefaults.provider;
}

function currentAsrEngine() {
  return document.querySelector('input[name="settings-asr-engine"]:checked')?.value ?? "local";
}

function setAsrEngine(engine) {
  document.querySelectorAll('input[name="settings-asr-engine"]').forEach((input) => {
    input.checked = input.value === engine;
  });
}

function applyRewriteConfig(rewrite) {
  const config = { ...rewriteDefaults, ...(rewrite ?? {}) };
  store.config = {
    ...store.config,
    rewrite: config,
  };
  rewriteEnabled.checked = Boolean(config.enabled);
  rewriteProfile.value = config.default_profile ?? rewriteDefaults.default_profile;
  updateRewriteSummary();
}

function readRewriteConfig() {
  return {
    enabled: rewriteEnabled.checked,
    provider: currentRewriteProvider(),
    model: null,
    default_profile: rewriteProfile.value,
    timeout_ms: store.config.rewrite.timeout_ms ?? rewriteDefaults.timeout_ms,
    user_dictionary: {},
  };
}

function updateRewriteSummary() {
  const provider = currentRewriteProvider();
  const profile = rewriteProfile.value;
  const meta = providerDefaults[provider] ?? providerDefaults.deepseek;
  rewriteEnabledLabel.textContent = rewriteEnabled.checked ? "开启" : "关闭";
  rewriteProfileSummary.textContent = profile;
  rewriteKeySummary.textContent = meta.env;
}

function updateAsrSummary() {
  const engine = currentAsrEngine();
  const cloud = engine === "cloud";
  asrCloudFields.hidden = !cloud;
  asrKeyStatus.textContent = cloud
    ? store.asrKeySaved
      ? "DashScope key 已保存"
      : `未保存，将回退到 ${providerDefaults.dashscope.env}`
    : "Local 模式不需要 key";
}

function documentApplyModeHint(mode) {
  switch (mode) {
    case "replace":
      return "新结果会替换当前全文稿";
    case "append":
      return "新结果会追加到文稿末尾";
    default:
      return "新结果将插入到当前光标位置";
  }
}

function applyTextToDocumentEditor(text) {
  const current = store.documentEditorText ?? "";
  switch (store.documentApplyMode) {
    case "replace":
      store.documentEditorText = text;
      store.editorStatus = "已替换全文稿";
      break;
    case "append":
      store.documentEditorText = current ? `${current}\n${text}` : text;
      store.editorStatus = "已追加到文稿末尾";
      break;
    default:
      store.documentEditorText = insertAtSelection(current, text);
      store.editorStatus = "已插入到当前光标位置";
      break;
  }
}

function insertAtSelection(current, text) {
  const start = documentEditor.selectionStart ?? current.length;
  const end = documentEditor.selectionEnd ?? current.length;
  return `${current.slice(0, start)}${text}${current.slice(end)}`;
}

async function refreshAsrKeyStatus() {
  const engine = currentAsrEngine();
  if (engine !== "cloud") {
    store.asrKeySaved = false;
    updateAsrSummary();
    return;
  }
  if (!invoke) {
    store.asrKeySaved = false;
    updateAsrSummary();
    return;
  }

  try {
    const status = await invoke("get_asr_key_status");
    store.asrKeySaved = Boolean(status?.saved);
  } catch (error) {
    store.asrKeySaved = false;
    store.settingsMessage = String(error);
    renderSettingsView();
  }
  updateAsrSummary();
}

function switchSettingsTab(tab) {
  store.settingsTab = tab;
  settingsTabs.forEach((button) => {
    const active = button.dataset.settingsTab === tab;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", String(active));
  });
  settingsPanes.forEach((pane) => {
    pane.hidden = pane.dataset.settingsPane !== tab;
  });
}

function setMode(mode) {
  if (!document.querySelector(`[data-mode-pane="${mode}"]`)) {
    return;
  }

  if (contentModes.has(mode)) {
    store.previousContentMode = mode;
  }
  store.mode = mode;
  renderModeVisibility();
  renderRuntimeViews();
  void applyWindowChrome(mode);
  void syncOutputMode(mode);
}

function openSettings() {
  setMode("settings");
}

function closeSettings() {
  setMode(store.previousContentMode);
}

function contextMenuItemsForActiveMode() {
  const items = [];
  const hasVoicePad = Boolean(document.querySelector('[data-mode-pane="voice-pad"]'));

  if (store.mode !== "floating") {
    items.push({
      label: "切换到悬浮窗模式",
      action: () => setMode("floating"),
    });
  }
  if (hasVoicePad && store.mode !== "voice-pad") {
    items.push({
      label: "切换到文稿模式",
      action: () => setMode("voice-pad"),
    });
  }
  if (store.mode !== "settings") {
    items.push({
      label: "打开设置",
      action: () => setMode("settings"),
    });
  }
  items.push({
    label: "关闭",
    action: closeApp,
  });

  return items;
}

function shouldShowModeContextMenu(event) {
  const pane = event.target.closest("[data-mode-pane]");
  if (!pane || pane.hidden) {
    return false;
  }

  return ["floating", "voice-pad"].includes(pane.dataset.modePane);
}

async function showModeContextMenu(event) {
  if (!shouldShowModeContextMenu(event)) {
    return;
  }

  event.preventDefault();
  const items = contextMenuItemsForActiveMode();
  if (items.length === 0) {
    return;
  }

  if (!NativeMenu || !NativeMenuItem) {
    items[0].action();
    return;
  }

  const menuItems = await Promise.all(
    items.map((item) =>
      NativeMenuItem.new({
        text: item.label,
        action: item.action,
      }),
    ),
  );
  activeContextMenu = await NativeMenu.new({ items: menuItems });
  await activeContextMenu.popup(undefined, appWindow);
}

async function closeApp() {
  if (!invoke) {
    return;
  }
  try {
    await invoke("exit_app");
  } catch (error) {
    store.settingsMessage = String(error);
    renderSettingsView();
  }
}

async function applyWindowChrome(mode) {
  if (!appWindow || !LogicalSize) {
    return;
  }

  const size = windowSizes[mode] ?? windowSizes.floating;
  try {
    await appWindow.setFocusable(mode !== "floating");
    await appWindow.setSize(new LogicalSize(size.width, size.height));
  } catch (error) {
    store.settingsMessage = String(error);
    renderSettingsView();
  }
}

async function syncOutputMode(mode) {
  if (!contentModes.has(mode)) {
    return;
  }

  const next = mode === "voice-pad" ? "voice_pad" : "floating_input";
  store.outputMode = next;
  renderRuntimeViews();
  if (!invoke) {
    return;
  }

  try {
    store.outputMode = await invoke("set_output_mode", { outputMode: next });
    renderRuntimeViews();
  } catch (error) {
    store.settingsMessage = String(error);
    renderSettingsView();
  }
}

async function boot() {
  renderModeVisibility();
  void applyWindowChrome(store.mode);
  applyState({ state: "idle" });

  if (!invoke || !listen) {
    applyConfig(configDefaults);
    store.settingsMessage = "预览模式";
    renderSettingsView();
    return;
  }

  await listen("realtime-state", (event) => applyState(event.payload));
  await listen("pause-state", (event) => applyPauseState(event.payload));
  await listen("rewrite-result", (event) => applyRewriteResult(event.payload));
  await listen("desktop-output-result", (event) => applyDesktopOutputResult(event.payload));
  await listen("output-mode-updated", (event) => {
    store.outputMode = event.payload ?? "floating_input";
    renderRuntimeViews();
  });
  await listen("runtime-error", (event) => {
    const payload = event.payload;
    applyState({ state: "error", error: payload?.error ?? "运行时错误" });
  });
  await listen("paste-failure", (event) => {
    const payload = event.payload;
    store.runtimeError = `文本已生成，但自动粘贴失败：${payload?.error ?? ""}`;
    renderRuntimeViews();
  });

  applyConfig(await invoke("get_config"));
  applyRewriteConfig(await invoke("get_rewrite_config"));
  applyPauseState(await invoke("get_pause_state"));
  store.outputMode = await invoke("get_output_mode");
  if (store.outputMode === "voice_pad") {
    store.mode = "voice-pad";
    store.previousContentMode = "voice-pad";
    renderModeVisibility();
    void applyWindowChrome("voice-pad");
  }
  await refreshAsrKeyStatus();
  store.settingsMessage = "运行中";
  renderSettingsView();
  await invoke("start_runtime");
}

settingsToggle.addEventListener("click", () => {
  if (store.mode === "settings") {
    closeSettings();
  } else {
    openSettings();
  }
});

settingsClose.addEventListener("click", closeSettings);
documentCloseApp?.addEventListener("click", closeApp);

modeTabs.forEach((button) => {
  button.addEventListener("click", () => setMode(button.dataset.modeTab));
});

document.addEventListener("contextmenu", showModeContextMenu);
pauseToggle.addEventListener("click", togglePauseState);
micButton.addEventListener("click", toggleManualRecording);

async function togglePauseState() {
  const next = !store.paused;
  applyPauseState({ paused: next });
  if (!invoke) {
    store.settingsMessage = "预览模式";
    renderSettingsView();
    return;
  }
  try {
    const state = await invoke("set_pause_state", { paused: next });
    applyPauseState(state);
  } catch (error) {
    applyPauseState({ paused: !next });
    store.settingsMessage = String(error);
    renderSettingsView();
  }
}

async function toggleManualRecording() {
  if (store.paused) {
    store.settingsMessage = "监听已暂停";
    renderSettingsView();
    return;
  }

  if (store.manualRecording) {
    await endManualRecording();
  } else {
    await beginManualRecording();
  }
}

async function beginManualRecording() {
  if (!invoke) {
    setManualRecording(true);
    applyState({ state: "recording" });
    store.settingsMessage = "预览模式";
    renderSettingsView();
    return;
  }

  try {
    await invoke("begin_manual_recording");
    setManualRecording(true);
  } catch (error) {
    setManualRecording(false);
    store.settingsMessage = String(error);
    renderSettingsView();
  }
}

async function endManualRecording() {
  if (!invoke) {
    setManualRecording(false);
    applyState({ state: "idle" });
    store.settingsMessage = "预览模式";
    renderSettingsView();
    return;
  }

  try {
    await invoke("end_manual_recording");
    setManualRecording(false);
  } catch (error) {
    store.settingsMessage = String(error);
    renderSettingsView();
  }
}

settingsTabs.forEach((button) => {
  button.addEventListener("click", () => switchSettingsTab(button.dataset.settingsTab));
});

rewriteEnabled.addEventListener("change", updateRewriteSummary);
rewriteProfile.addEventListener("change", updateRewriteSummary);

documentEditor.addEventListener("input", () => {
  store.documentEditorText = documentEditor.value;
});
documentApplyModeButtons.forEach((button) => {
  button.addEventListener("click", () => {
    store.documentApplyMode = button.dataset.documentApplyMode;
    renderRuntimeViews();
  });
});
copyEditor.addEventListener("click", async () => {
  if (!store.documentEditorText.trim()) {
    store.editorStatus = "文稿为空";
    renderRuntimeViews();
    return;
  }
  try {
    if (!invoke) {
      await navigator.clipboard.writeText(store.documentEditorText);
    } else {
      await invoke("copy_text", { text: store.documentEditorText });
    }
    store.editorStatus = "已复制全文稿";
    renderRuntimeViews();
  } catch (error) {
    store.editorStatus = String(error);
    renderRuntimeViews();
  }
});
clearEditor.addEventListener("click", () => {
  store.documentEditorText = "";
  store.editorStatus = "已清空文稿";
  renderRuntimeViews();
});
document.querySelectorAll('input[name="settings-asr-engine"]').forEach((input) => {
  input.addEventListener("change", async () => {
    asrApiKey.value = "";
    await refreshAsrKeyStatus();
  });
});

clearAsrKey.addEventListener("click", async () => {
  store.settingsMessage = "清除中...";
  renderSettingsView();
  try {
    if (!invoke) {
      asrApiKey.value = "";
      store.asrKeySaved = false;
      updateAsrSummary();
      store.settingsMessage = "预览模式";
      renderSettingsView();
      return;
    }

    const status = await invoke("delete_asr_key");
    asrApiKey.value = "";
    store.asrKeySaved = Boolean(status?.saved);
    updateAsrSummary();
    store.settingsMessage = "ASR API key 已清除";
    renderSettingsView();
  } catch (error) {
    store.settingsMessage = String(error);
    renderSettingsView();
  }
});

saveSettings.addEventListener("click", async () => {
  if (store.settingsTab === "rewrite") {
    store.settingsMessage = "保存中...";
    renderSettingsView();
    try {
      if (!invoke) {
        applyRewriteConfig(readRewriteConfig());
        store.settingsMessage = "预览模式";
        renderSettingsView();
        return;
      }

      const rewrite = await invoke("save_rewrite_config", {
        rewrite: readRewriteConfig(),
      });
      applyRewriteConfig(rewrite);
      store.settingsMessage = "改写设置已保存";
      renderSettingsView();
    } catch (error) {
      store.settingsMessage = String(error);
      renderSettingsView();
    }
    return;
  }

  store.settingsMessage = "保存中...";
  renderSettingsView();
  try {
    const hotkeyError = validateHotkeyForm();
    if (hotkeyError) {
      store.settingsMessage = hotkeyError;
      renderSettingsView();
      return;
    }

    if (!invoke) {
      applyConfig(readConfig());
      store.settingsMessage = "预览模式";
      renderSettingsView();
      return;
    }

    if (currentAsrEngine() === "cloud" && asrApiKey.value.trim()) {
      const status = await invoke("save_asr_key", {
        request: { api_key: asrApiKey.value.trim() },
      });
      store.asrKeySaved = Boolean(status?.saved);
      asrApiKey.value = "";
      updateAsrSummary();
    }

    const config = await invoke("save_config", { config: readConfig() });
    applyConfig(config);
    await refreshAsrKeyStatus();
    store.settingsMessage = "已保存并重新加载";
    renderSettingsView();
  } catch (error) {
    store.settingsMessage = String(error);
    renderSettingsView();
  }
});

window.__VOICE_FLOW_APPLY_STATE__ = applyState;

boot().catch((error) => {
  applyState({ state: "error", error: String(error) });
});
