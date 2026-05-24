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

const dot = document.querySelector(".status-dot");
const stateLabel = document.querySelector("#state-label");
const lastTranscript = document.querySelector("#last-transcript");
const rewriteChip = document.querySelector("#rewrite-chip");
const resultMeta = document.querySelector("#result-meta");
const fallbackReason = document.querySelector("#fallback-reason");
const latencySummary = document.querySelector("#latency-summary");
const variantPanel = document.querySelector("#variant-panel");
const variantText = document.querySelector("#variant-text");
const variantTabs = document.querySelectorAll("[data-variant-tab]");
const copyVariant = document.querySelector("#copy-variant");
const pasteVariant = document.querySelector("#paste-variant");
const variantActionStatus = document.querySelector("#variant-action-status");
const runtimeError = document.querySelector("#runtime-error");
const runtimeErrorText = document.querySelector("#runtime-error-text");
const pauseToggle = document.querySelector("#pause-toggle");
const settingsToggle = document.querySelector("#settings-toggle");
const settingsPanel = document.querySelector("#settings-panel");
const settingsMessage = document.querySelector("#settings-message");
const modeTabs = document.querySelectorAll("[data-mode-tab]");
const modePanes = document.querySelectorAll("[data-mode-pane]");
const modelDirInput = document.querySelector("#model-dir");
const hotkeyCtrl = document.querySelector("#hotkey-ctrl");
const hotkeyAlt = document.querySelector("#hotkey-alt");
const hotkeyShift = document.querySelector("#hotkey-shift");
const hotkeyLogo = document.querySelector("#hotkey-logo");
const hotkeyKey = document.querySelector("#hotkey-key");
const saveSettings = document.querySelector("#save-settings");
const settingsTabs = document.querySelectorAll("[data-settings-tab]");
const settingsPanes = document.querySelectorAll("[data-settings-pane]");
const refreshDiagnostics = document.querySelector("#refresh-diagnostics");
const diagConfigPath = document.querySelector("#diag-config-path");
const diagLogPath = document.querySelector("#diag-log-path");
const diagModelDir = document.querySelector("#diag-model-dir");
const diagRuntime = document.querySelector("#diag-runtime");
const diagRewriteKey = document.querySelector("#diag-rewrite-key");
const rewriteEnabled = document.querySelector("#rewrite-enabled");
const rewriteEnabledLabel = document.querySelector("#rewrite-enabled-label");
const rewriteProfile = document.querySelector("#rewrite-profile");
const rewriteModel = document.querySelector("#rewrite-model");
const rewriteTimeout = document.querySelector("#rewrite-timeout");
const rewriteApiKey = document.querySelector("#rewrite-api-key");
const clearRewriteKey = document.querySelector("#clear-rewrite-key");
const rewriteKeyStatus = document.querySelector("#rewrite-key-status");
const rewriteProviderSummary = document.querySelector("#rewrite-provider-summary");
const rewriteProfileSummary = document.querySelector("#rewrite-profile-summary");
const rewriteKeySummary = document.querySelector("#rewrite-key-summary");

const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;
let configSnapshot = normalizeConfig(configDefaults);
let activeSettingsTab = "input";
let activeVariant = "clean";
let rewriteVariants = {};
let rewriteKeySaved = false;
let paused = false;
let activeMode = "floating";

function applyState(event) {
  const state = event?.state ?? "idle";
  dot.dataset.state = state;
  stateLabel.textContent = paused ? "已暂停" : labels[state] ?? labels.idle;

  if (state !== "error") {
    runtimeError.hidden = true;
    runtimeErrorText.textContent = "";
  }

  if (["recording", "transcribing", "rewriting"].includes(state)) {
    rewriteVariants = {};
    updateVariantPanel();
  }

  if (event?.transcript) {
    lastTranscript.textContent = event.transcript;
  }

  if (event?.error) {
    const message = String(event.error);
    runtimeError.hidden = false;
    runtimeErrorText.textContent = message;
    settingsMessage.textContent = message;
  }
}

function applyPauseState(event) {
  paused = Boolean(event?.paused);
  pauseToggle.dataset.active = String(paused);
  pauseToggle.title = paused ? "恢复监听" : "暂停监听";
  pauseToggle.querySelector("span").textContent = paused ? ">" : "||";
  if (paused) {
    dot.dataset.state = "idle";
    stateLabel.textContent = "已暂停";
  } else if (stateLabel.textContent === "已暂停") {
    stateLabel.textContent = labels.idle;
  }
  if (activeSettingsTab === "diagnostics") {
    refreshDiagnosticsPanel();
  }
}

function applyRewriteResult(result) {
  rewriteVariants = {
    clean: result?.text ?? "",
    ...(result?.variants ?? {}),
  };
  activeVariant = rewriteVariants[activeVariant] ? activeVariant : "clean";
  variantActionStatus.textContent = "";
  if (rewriteVariants.clean) {
    lastTranscript.textContent = rewriteVariants.clean;
  }
  if (result?.fallback && result.error) {
    settingsMessage.textContent = result.error;
  }
  updateResultMeta(result);
  updateVariantPanel();
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
  fallbackReason.textContent = reason ? `fallback: ${reason}` : "";
  fallbackReason.dataset.active = String(Boolean(reason));
  latencySummary.textContent = parts.join(" / ");
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
  configSnapshot = normalizeConfig(config);
  modelDirInput.value = configSnapshot.model_dir ?? "";
  hotkeyCtrl.checked = Boolean(configSnapshot.hotkey.ctrl);
  hotkeyAlt.checked = Boolean(configSnapshot.hotkey.alt);
  hotkeyShift.checked = Boolean(configSnapshot.hotkey.shift);
  hotkeyLogo.checked = Boolean(configSnapshot.hotkey.logo);
  hotkeyKey.value = configSnapshot.hotkey.key ?? "Space";
  applyRewriteConfig(configSnapshot.rewrite);
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
    rewrite: configSnapshot.rewrite,
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
    document.querySelector('input[name="rewrite-provider"]:checked')?.value ??
    rewriteDefaults.provider
  );
}

function applyRewriteConfig(rewrite) {
  const config = { ...rewriteDefaults, ...(rewrite ?? {}) };
  configSnapshot = {
    ...configSnapshot,
    rewrite: config,
  };
  rewriteEnabled.checked = Boolean(config.enabled);
  rewriteProfile.value = config.default_profile ?? rewriteDefaults.default_profile;
  setRewriteProvider(config.provider ?? rewriteDefaults.provider);
  rewriteModel.value =
    config.model ?? providerDefaults[currentRewriteProvider()]?.model ?? "";
  rewriteTimeout.value = String(config.timeout_ms ?? rewriteDefaults.timeout_ms);
  rewriteApiKey.value = "";
  rewriteVariants = {};
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
  document.querySelectorAll('input[name="rewrite-provider"]').forEach((input) => {
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
  rewriteKeyStatus.textContent = rewriteKeySaved
    ? `${provider} key 已保存`
    : `未保存，将回退到 ${meta.env}`;
  rewriteChip.textContent = rewriteEnabled.checked ? `改写 ${profile}` : "改写关闭";
  rewriteChip.dataset.enabled = String(rewriteEnabled.checked);
  updateVariantPanel();
}

async function refreshRewriteKeyStatus() {
  const provider = currentRewriteProvider();
  if (!invoke) {
    rewriteKeySaved = false;
    updateRewriteSummary();
    return;
  }

  try {
    const status = await invoke("get_rewrite_key_status", {
      request: { provider },
    });
    rewriteKeySaved = Boolean(status?.saved);
  } catch (error) {
    rewriteKeySaved = false;
    settingsMessage.textContent = String(error);
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
  rewriteKeySaved = Boolean(status?.saved);
}

function switchSettingsTab(tab) {
  activeSettingsTab = tab;
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
  activeMode = mode;
  modeTabs.forEach((button) => {
    const active = button.dataset.modeTab === mode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", String(active));
  });
  modePanes.forEach((pane) => {
    pane.hidden = pane.dataset.modePane !== mode;
  });
  if (mode === "settings" && activeSettingsTab === "diagnostics") {
    refreshDiagnosticsPanel();
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
    settingsMessage.textContent = String(error);
  }
}

function switchVariantTab(variant) {
  activeVariant = variant;
  variantTabs.forEach((button) => {
    button.classList.toggle("is-active", button.dataset.variantTab === variant);
  });
  variantText.textContent = rewriteVariants[variant] ?? "";
}

function updateVariantPanel() {
  const show =
    rewriteEnabled.checked &&
    rewriteProfile.value === "multi" &&
    Object.keys(rewriteVariants).length > 1;
  variantPanel.hidden = !show;
  copyVariant.disabled = !show;
  pasteVariant.disabled = !show;
  if (show) {
    switchVariantTab(activeVariant);
  } else {
    variantActionStatus.textContent = "";
  }
}

async function boot() {
  applyState({ state: "idle" });

  if (!invoke || !listen) {
    applyConfig(configDefaults);
    settingsMessage.textContent = "预览模式";
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
      lastTranscript.textContent = payload.text;
    }
    runtimeError.hidden = false;
    runtimeErrorText.textContent = `文本已生成，但自动粘贴失败：${payload?.error ?? ""}`;
  });

  applyConfig(await invoke("get_config"));
  applyRewriteConfig(await invoke("get_rewrite_config"));
  applyPauseState(await invoke("get_pause_state"));
  await refreshRewriteKeyStatus();
  settingsMessage.textContent = "运行中";
  await invoke("start_runtime");
}

settingsToggle.addEventListener("click", () => {
  switchMode(activeMode === "settings" ? "floating" : "settings");
});

modeTabs.forEach((button) => {
  button.addEventListener("click", () => switchMode(button.dataset.modeTab));
});

pauseToggle.addEventListener("click", async () => {
  const next = !paused;
  applyPauseState({ paused: next });
  if (!invoke) {
    settingsMessage.textContent = "预览模式";
    return;
  }
  try {
    const state = await invoke("set_pause_state", { paused: next });
    applyPauseState(state);
  } catch (error) {
    applyPauseState({ paused: !next });
    settingsMessage.textContent = String(error);
  }
});

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
  const text = rewriteVariants[activeVariant] ?? "";
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
document.querySelectorAll('input[name="rewrite-provider"]').forEach((input) => {
  input.addEventListener("change", async () => {
    if (!rewriteModel.value.trim()) {
      rewriteModel.value = providerDefaults[currentRewriteProvider()]?.model ?? "";
    }
    rewriteApiKey.value = "";
    await refreshRewriteKeyStatus();
  });
});

clearRewriteKey.addEventListener("click", async () => {
  settingsMessage.textContent = "清除中...";
  try {
    if (!invoke) {
      rewriteApiKey.value = "";
      rewriteKeySaved = false;
      updateRewriteSummary();
      settingsMessage.textContent = "预览模式";
      return;
    }

    const status = await invoke("delete_rewrite_key", {
      request: { provider: currentRewriteProvider() },
    });
    rewriteApiKey.value = "";
    rewriteKeySaved = Boolean(status?.saved);
    updateRewriteSummary();
    settingsMessage.textContent = "API key 已清除";
  } catch (error) {
    settingsMessage.textContent = String(error);
  }
});

saveSettings.addEventListener("click", async () => {
  if (activeSettingsTab === "rewrite") {
    settingsMessage.textContent = "保存中...";
    try {
      if (!invoke) {
        applyRewriteConfig(readRewriteConfig());
        settingsMessage.textContent = "预览模式";
        return;
      }

      const rewrite = await invoke("save_rewrite_config", {
        rewrite: readRewriteConfig(),
      });
      await saveRewriteKeyIfNeeded();
      applyRewriteConfig(rewrite);
      await refreshRewriteKeyStatus();
      settingsMessage.textContent = "改写设置已保存";
    } catch (error) {
      settingsMessage.textContent = String(error);
    }
    return;
  }

  settingsMessage.textContent = "保存中...";
  try {
    const hotkeyError = validateHotkeyForm();
    if (hotkeyError) {
      settingsMessage.textContent = hotkeyError;
      return;
    }

    if (!invoke) {
      applyConfig(readConfig());
      settingsMessage.textContent = "预览模式";
      return;
    }

    const config = await invoke("save_config", { config: readConfig() });
    applyConfig(config);
    settingsMessage.textContent = "已保存并重新加载";
  } catch (error) {
    settingsMessage.textContent = String(error);
  }
});

window.__VOICE_FLOW_APPLY_STATE__ = applyState;

boot().catch((error) => {
  applyState({ state: "error", error: String(error) });
});
