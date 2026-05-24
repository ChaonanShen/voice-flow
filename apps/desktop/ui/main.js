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
  hotkey: {
    ctrl: false,
    alt: true,
    shift: false,
    logo: false,
    key: "Space",
  },
  rewrite: rewriteDefaults,
};

const dot = document.querySelector("#document-status-dot");
const stateLabel = document.querySelector("#document-state-label");
const lastTranscript = document.querySelector("#document-last-text");
const rewriteChip = document.querySelector("#document-rewrite-chip");
const resultMeta = document.querySelector("#document-result-meta");
const fallbackReason = document.querySelector("#document-fallback-reason");
const latencySummary = document.querySelector("#document-latency-summary");
const variantPanel = document.querySelector("#document-variant-panel");
const variantText = document.querySelector("#document-variant-text");
const variantTabs = document.querySelectorAll(".document-variant-tab");
const copyVariant = document.querySelector("#document-copy-variant");
const pasteVariant = document.querySelector("#document-paste-variant");
const variantActionStatus = document.querySelector("#document-variant-action-status");
const micButton = document.querySelector("#floating-mic-button");
const floatingSettings = document.querySelector("#floating-settings");
const settingsClose = document.querySelector("#settings-close");
const runtimeError = document.querySelector("#document-runtime-error");
const runtimeErrorText = document.querySelector("#document-runtime-error-text");
const pauseToggle = document.querySelector("#document-pause-toggle");
const settingsToggle = document.querySelector("#document-settings-toggle");
const settingsPanel = document.querySelector("#settings-panel");
const settingsMessage = document.querySelector("#settings-message");
const modeTabs = document.querySelectorAll("[data-mode-tab]");
const modePanes = document.querySelectorAll("[data-mode-pane]");
const modelDirInput = document.querySelector("#settings-model-dir");
const hotkeyCtrl = document.querySelector("#settings-hotkey-ctrl");
const hotkeyAlt = document.querySelector("#settings-hotkey-alt");
const hotkeyShift = document.querySelector("#settings-hotkey-shift");
const hotkeyLogo = document.querySelector("#settings-hotkey-logo");
const hotkeyKey = document.querySelector("#settings-hotkey-key");
const saveSettings = document.querySelector("#settings-save");
const settingsTabs = document.querySelectorAll("[data-settings-tab]");
const settingsPanes = document.querySelectorAll("[data-settings-pane]");
const refreshDiagnostics = document.querySelector("#settings-refresh-diagnostics");
const diagConfigPath = document.querySelector("#settings-diag-config-path");
const diagLogPath = document.querySelector("#settings-diag-log-path");
const diagModelDir = document.querySelector("#settings-diag-model-dir");
const diagRuntime = document.querySelector("#settings-diag-runtime");
const diagRewriteKey = document.querySelector("#settings-diag-rewrite-key");
const rewriteEnabled = document.querySelector("#settings-rewrite-enabled");
const rewriteEnabledLabel = document.querySelector("#settings-rewrite-enabled-label");
const rewriteProfile = document.querySelector("#settings-rewrite-profile");
const rewriteModel = document.querySelector("#settings-rewrite-model");
const rewriteTimeout = document.querySelector("#settings-rewrite-timeout");
const rewriteApiKey = document.querySelector("#settings-rewrite-api-key");
const clearRewriteKey = document.querySelector("#settings-clear-rewrite-key");
const rewriteKeyStatus = document.querySelector("#settings-rewrite-key-status");
const rewriteProviderSummary = document.querySelector("#settings-rewrite-provider-summary");
const rewriteProfileSummary = document.querySelector("#settings-rewrite-profile-summary");
const rewriteKeySummary = document.querySelector("#settings-rewrite-key-summary");

const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;
const NativeMenu = window.__TAURI__?.menu?.Menu;
const NativeMenuItem = window.__TAURI__?.menu?.MenuItem;
const appWindow = window.__TAURI__?.window?.getCurrentWindow?.();
const LogicalSize = window.__TAURI__?.dpi?.LogicalSize;
const windowSizes = {
  floating: { width: 96, height: 106 },
  "voice-pad": { width: 560, height: 420 },
  settings: { width: 460, height: 360 },
};
const store = {
  config: normalizeConfig(configDefaults),
  settingsTab: "input",
  activeVariant: "clean",
  rewriteVariants: {},
  rewriteKeySaved: false,
  paused: false,
  mode: "floating",
  previousContentMode: "floating",
  currentState: "idle",
  manualRecording: false,
  documentText: "",
  runtimeError: "",
  resultMeta: {
    fallbackReason: "",
    latencySummary: "",
  },
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
  lastTranscript.textContent = store.documentText || "尚无识别结果";
  runtimeError.hidden = !store.runtimeError;
  runtimeErrorText.textContent = store.runtimeError;
  pauseToggle.dataset.active = String(store.paused);
  pauseToggle.title = store.paused ? "恢复监听" : "暂停监听";
  pauseToggle.querySelector("span").textContent = store.paused ? ">" : "||";
}

function renderSettingsView() {
  settingsMessage.textContent = store.settingsMessage;
}

function renderRuntimeViews() {
  renderFloatingMode();
  renderDocumentMode();
  renderSettingsView();
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

  if (["recording", "transcribing", "rewriting"].includes(state)) {
    store.rewriteVariants = {};
    updateVariantPanel();
  }

  if (event?.transcript) {
    store.documentText = event.transcript;
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
  if (store.settingsTab === "diagnostics") {
    refreshDiagnosticsPanel();
  }
}

function setManualRecording(active) {
  store.manualRecording = Boolean(active);
  renderFloatingMode();
}

function applyRewriteResult(result) {
  store.rewriteVariants = {
    clean: result?.text ?? "",
    ...(result?.variants ?? {}),
  };
  store.activeVariant = store.rewriteVariants[store.activeVariant]
    ? store.activeVariant
    : "clean";
  variantActionStatus.textContent = "";
  if (store.rewriteVariants.clean) {
    store.documentText = store.rewriteVariants.clean;
  }
  if (result?.fallback && result.error) {
    store.settingsMessage = result.error;
  }
  updateResultMeta(result);
  updateVariantPanel();
  renderRuntimeViews();
}

function updateResultMeta(result) {
  const timings = result?.timings ?? {};
  const parts = [];
  if (Number.isFinite(Number(timings.asr_ms))) {
    parts.push(`ASR ${timings.asr_ms}ms`);
  }
  if (Number.isFinite(Number(timings.rewrite_ms))) {
    parts.push(`rewrite ${timings.rewrite_ms}ms`);
  }
  if (Number.isFinite(Number(timings.paste_ms))) {
    parts.push(`paste ${timings.paste_ms}ms`);
  }

  const reason = result?.fallback ? result?.error ?? "rewrite fallback" : "";
  store.resultMeta = {
    fallbackReason: reason ? `fallback: ${reason}` : "",
    latencySummary: parts.join(" / "),
  };
  fallbackReason.textContent = store.resultMeta.fallbackReason;
  fallbackReason.dataset.active = String(Boolean(reason));
  latencySummary.textContent = store.resultMeta.latencySummary;
  resultMeta.hidden = !reason && parts.length === 0;
}

function normalizeConfig(config) {
  return {
    ...configDefaults,
    ...(config ?? {}),
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
  modelDirInput.value = store.config.model_dir ?? "";
  hotkeyCtrl.checked = Boolean(store.config.hotkey.ctrl);
  hotkeyAlt.checked = Boolean(store.config.hotkey.alt);
  hotkeyShift.checked = Boolean(store.config.hotkey.shift);
  hotkeyLogo.checked = Boolean(store.config.hotkey.logo);
  hotkeyKey.value = store.config.hotkey.key ?? "Space";
  applyRewriteConfig(store.config.rewrite);
}

function readConfig() {
  const modelDir = modelDirInput.value.trim();
  return {
    model_dir: modelDir.length > 0 ? modelDir : null,
    hotkey: {
      ctrl: hotkeyCtrl.checked,
      alt: hotkeyAlt.checked,
      shift: hotkeyShift.checked,
      logo: hotkeyLogo.checked,
      key: hotkeyKey.value.trim() || "Space",
    },
    rewrite: store.config.rewrite,
  };
}

function validateHotkeyForm() {
  const key = hotkeyKey.value.trim();
  if (!key) {
    return "快捷键主键不能为空";
  }
  if (key.includes("+") || key.split(/\s+/).length > 1) {
    return "主键只填单个按键，修饰键使用复选框";
  }
  if (
    !hotkeyCtrl.checked &&
    !hotkeyAlt.checked &&
    !hotkeyShift.checked &&
    !hotkeyLogo.checked
  ) {
    return "快捷键至少需要一个修饰键";
  }
  return null;
}

function currentRewriteProvider() {
  return (
    document.querySelector('input[name="settings-rewrite-provider"]:checked')?.value ??
    rewriteDefaults.provider
  );
}

function applyRewriteConfig(rewrite) {
  const config = { ...rewriteDefaults, ...(rewrite ?? {}) };
  store.config = {
    ...store.config,
    rewrite: config,
  };
  rewriteEnabled.checked = Boolean(config.enabled);
  rewriteProfile.value = config.default_profile ?? rewriteDefaults.default_profile;
  setRewriteProvider(config.provider ?? rewriteDefaults.provider);
  rewriteModel.value =
    config.model ?? providerDefaults[currentRewriteProvider()]?.model ?? "";
  rewriteTimeout.value = String(config.timeout_ms ?? rewriteDefaults.timeout_ms);
  rewriteApiKey.value = "";
  store.rewriteVariants = {};
  updateRewriteSummary();
}

function readRewriteConfig() {
  const model = rewriteModel.value.trim();
  const timeout = Number.parseInt(rewriteTimeout.value, 10);
  return {
    enabled: rewriteEnabled.checked,
    provider: currentRewriteProvider(),
    model: model.length > 0 ? model : null,
    default_profile: rewriteProfile.value,
    timeout_ms: Number.isFinite(timeout) ? timeout : rewriteDefaults.timeout_ms,
    user_dictionary: {},
  };
}

function setRewriteProvider(provider) {
  document.querySelectorAll('input[name="settings-rewrite-provider"]').forEach((input) => {
    input.checked = input.value === provider;
  });
}

function updateRewriteSummary() {
  const provider = currentRewriteProvider();
  const profile = rewriteProfile.value;
  const meta = providerDefaults[provider] ?? providerDefaults.deepseek;
  rewriteEnabledLabel.textContent = rewriteEnabled.checked ? "开启" : "关闭";
  rewriteProviderSummary.textContent = provider;
  rewriteProfileSummary.textContent = profile;
  rewriteKeySummary.textContent = meta.env;
  rewriteKeyStatus.textContent = store.rewriteKeySaved
    ? `${provider} key 已保存`
    : `未保存，将回退到 ${meta.env}`;
  rewriteChip.textContent = rewriteEnabled.checked ? `改写 ${profile}` : "改写关闭";
  rewriteChip.dataset.enabled = String(rewriteEnabled.checked);
  updateVariantPanel();
}

async function refreshRewriteKeyStatus() {
  const provider = currentRewriteProvider();
  if (!invoke) {
    store.rewriteKeySaved = false;
    updateRewriteSummary();
    return;
  }

  try {
    const status = await invoke("get_rewrite_key_status", {
      request: { provider },
    });
    store.rewriteKeySaved = Boolean(status?.saved);
  } catch (error) {
    store.rewriteKeySaved = false;
    store.settingsMessage = String(error);
    renderSettingsView();
  }
  updateRewriteSummary();
}

async function saveRewriteKeyIfNeeded() {
  const apiKey = rewriteApiKey.value.trim();
  if (!apiKey) {
    return;
  }

  const status = await invoke("save_rewrite_key", {
    request: {
      provider: currentRewriteProvider(),
      api_key: apiKey,
    },
  });
  rewriteApiKey.value = "";
  store.rewriteKeySaved = Boolean(status?.saved);
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
  if (tab === "diagnostics") {
    refreshDiagnosticsPanel();
  }
}

function switchMode(mode) {
  if (!document.querySelector(`[data-mode-pane="${mode}"]`)) {
    return;
  }

  if (mode !== "settings") {
    store.previousContentMode = mode;
  }
  store.mode = mode;
  void applyWindowMode(mode);
  modeTabs.forEach((button) => {
    const active = button.dataset.modeTab === mode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", String(active));
  });
  modePanes.forEach((pane) => {
    pane.hidden = pane.dataset.modePane !== mode;
  });
  if (mode === "settings" && store.settingsTab === "diagnostics") {
    refreshDiagnosticsPanel();
  }
}

function contextMenuItemsForActiveMode() {
  const items = [];
  const hasVoicePad = Boolean(document.querySelector('[data-mode-pane="voice-pad"]'));

  if (store.mode !== "floating") {
    items.push({
      label: "切换到悬浮窗模式",
      mode: "floating",
    });
  }
  if (hasVoicePad && store.mode !== "voice-pad") {
    items.push({
      label: "切换到文稿模式",
      mode: "voice-pad",
    });
  }
  if (store.mode !== "settings") {
    items.push({
      label: "打开设置",
      mode: "settings",
    });
  }

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
    switchMode(items[0].mode);
    return;
  }

  const menuItems = await Promise.all(
    items.map((item) =>
      NativeMenuItem.new({
        text: item.label,
        action: () => switchMode(item.mode),
      }),
    ),
  );
  activeContextMenu = await NativeMenu.new({ items: menuItems });
  await activeContextMenu.popup(undefined, appWindow);
}

async function applyWindowMode(mode) {
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

async function refreshDiagnosticsPanel() {
  if (!invoke) {
    diagConfigPath.textContent = "预览模式";
    diagLogPath.textContent = "预览模式";
    diagModelDir.textContent = "预览模式";
    diagRuntime.textContent = "预览模式";
    diagRewriteKey.textContent = "预览模式";
    return;
  }

  try {
    const diagnostics = await invoke("get_diagnostics");
    diagConfigPath.textContent = diagnostics.config_path ?? "-";
    diagLogPath.textContent = diagnostics.log_path ?? "-";
    diagModelDir.textContent = diagnostics.model_dir
      ? `${diagnostics.model_dir} (${diagnostics.model_dir_exists ? "存在" : "缺失"})`
      : "未配置";
    diagRuntime.textContent = diagnostics.runtime_running
      ? diagnostics.paused
        ? "已暂停"
        : diagnostics.restart_requested
          ? "运行中，等待重载"
          : "运行中"
      : "未运行";
    diagRewriteKey.textContent = diagnostics.rewrite_key_saved
      ? `${diagnostics.rewrite_provider} 已保存`
      : `${diagnostics.rewrite_provider} 未保存`;
  } catch (error) {
    store.settingsMessage = String(error);
    renderSettingsView();
  }
}

function switchVariantTab(variant) {
  store.activeVariant = variant;
  variantTabs.forEach((button) => {
    button.classList.toggle("is-active", button.dataset.variantTab === variant);
  });
  variantText.textContent = store.rewriteVariants[variant] ?? "";
}

function updateVariantPanel() {
  const show =
    rewriteEnabled.checked &&
    rewriteProfile.value === "multi" &&
    Object.keys(store.rewriteVariants).length > 1;
  variantPanel.hidden = !show;
  copyVariant.disabled = !show;
  pasteVariant.disabled = !show;
  if (show) {
    switchVariantTab(store.activeVariant);
  } else {
    variantActionStatus.textContent = "";
  }
}

async function boot() {
  void applyWindowMode(store.mode);
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
  await listen("runtime-error", (event) => {
    const payload = event.payload;
    applyState({ state: "error", error: payload?.error ?? "运行时错误" });
  });
  await listen("paste-failure", (event) => {
    const payload = event.payload;
    if (payload?.text) {
      store.documentText = payload.text;
    }
    store.runtimeError = `文本已生成，但自动粘贴失败：${payload?.error ?? ""}`;
    renderRuntimeViews();
  });

  applyConfig(await invoke("get_config"));
  applyRewriteConfig(await invoke("get_rewrite_config"));
  applyPauseState(await invoke("get_pause_state"));
  await refreshRewriteKeyStatus();
  store.settingsMessage = "运行中";
  renderSettingsView();
  await invoke("start_runtime");
}

settingsToggle.addEventListener("click", () => {
  switchMode(store.mode === "settings" ? store.previousContentMode : "settings");
});

floatingSettings?.addEventListener("click", () => switchMode("settings"));
settingsClose.addEventListener("click", () => switchMode(store.previousContentMode));

modeTabs.forEach((button) => {
  button.addEventListener("click", () => switchMode(button.dataset.modeTab));
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
refreshDiagnostics.addEventListener("click", refreshDiagnosticsPanel);

rewriteEnabled.addEventListener("change", updateRewriteSummary);
rewriteProfile.addEventListener("change", updateRewriteSummary);
variantTabs.forEach((button) => {
  button.addEventListener("click", () => switchVariantTab(button.dataset.variantTab));
});
copyVariant.addEventListener("click", async () => {
  await writeSelectedVariant("copy_text", "复制中...", "已复制");
});
pasteVariant.addEventListener("click", async () => {
  await writeSelectedVariant("paste_text", "粘贴中...", "已粘贴");
});

async function writeSelectedVariant(command, pendingLabel, doneLabel) {
  const text = store.rewriteVariants[store.activeVariant] ?? "";
  if (!text.trim()) {
    variantActionStatus.textContent = "当前版本为空";
    return;
  }

  variantActionStatus.textContent = pendingLabel;
  try {
    if (!invoke) {
      await navigator.clipboard.writeText(text);
    } else {
      await invoke(command, { text });
    }
    variantActionStatus.textContent = doneLabel;
  } catch (error) {
    variantActionStatus.textContent = String(error);
  }
}
document.querySelectorAll('input[name="settings-rewrite-provider"]').forEach((input) => {
  input.addEventListener("change", async () => {
    if (!rewriteModel.value.trim()) {
      rewriteModel.value = providerDefaults[currentRewriteProvider()]?.model ?? "";
    }
    rewriteApiKey.value = "";
    await refreshRewriteKeyStatus();
  });
});

clearRewriteKey.addEventListener("click", async () => {
  store.settingsMessage = "清除中...";
  renderSettingsView();
  try {
    if (!invoke) {
      rewriteApiKey.value = "";
      store.rewriteKeySaved = false;
      updateRewriteSummary();
      store.settingsMessage = "预览模式";
      renderSettingsView();
      return;
    }

    const status = await invoke("delete_rewrite_key", {
      request: { provider: currentRewriteProvider() },
    });
    rewriteApiKey.value = "";
    store.rewriteKeySaved = Boolean(status?.saved);
    updateRewriteSummary();
    store.settingsMessage = "API key 已清除";
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
      await saveRewriteKeyIfNeeded();
      applyRewriteConfig(rewrite);
      await refreshRewriteKeyStatus();
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

    const config = await invoke("save_config", { config: readConfig() });
    applyConfig(config);
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
