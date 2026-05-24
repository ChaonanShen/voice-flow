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
    ctrl: true,
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
const runtimeError = document.querySelector("#runtime-error");
const runtimeErrorText = document.querySelector("#runtime-error-text");
const settingsToggle = document.querySelector("#settings-toggle");
const settingsPanel = document.querySelector("#settings-panel");
const settingsMessage = document.querySelector("#settings-message");
const modelDirInput = document.querySelector("#model-dir");
const hotkeyCtrl = document.querySelector("#hotkey-ctrl");
const hotkeyAlt = document.querySelector("#hotkey-alt");
const hotkeyShift = document.querySelector("#hotkey-shift");
const hotkeyLogo = document.querySelector("#hotkey-logo");
const hotkeyKey = document.querySelector("#hotkey-key");
const saveSettings = document.querySelector("#save-settings");
const settingsTabs = document.querySelectorAll("[data-settings-tab]");
const settingsPanes = document.querySelectorAll("[data-settings-pane]");
const rewriteEnabled = document.querySelector("#rewrite-enabled");
const rewriteEnabledLabel = document.querySelector("#rewrite-enabled-label");
const rewriteProfile = document.querySelector("#rewrite-profile");
const rewriteModel = document.querySelector("#rewrite-model");
const rewriteTimeout = document.querySelector("#rewrite-timeout");
const rewriteProviderSummary = document.querySelector("#rewrite-provider-summary");
const rewriteProfileSummary = document.querySelector("#rewrite-profile-summary");
const rewriteKeySummary = document.querySelector("#rewrite-key-summary");

const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;
let configSnapshot = normalizeConfig(configDefaults);
let activeSettingsTab = "input";

function applyState(event) {
  const state = event?.state ?? "idle";
  dot.dataset.state = state;
  stateLabel.textContent = labels[state] ?? labels.idle;

  if (state !== "error") {
    runtimeError.hidden = true;
    runtimeErrorText.textContent = "";
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

function currentRewriteProvider() {
  return (
    document.querySelector('input[name="rewrite-provider"]:checked')?.value ??
    rewriteDefaults.provider
  );
}

function applyRewriteConfig(rewrite) {
  const config = { ...rewriteDefaults, ...(rewrite ?? {}) };
  rewriteEnabled.checked = Boolean(config.enabled);
  rewriteProfile.value = config.default_profile ?? rewriteDefaults.default_profile;
  setRewriteProvider(config.provider ?? rewriteDefaults.provider);
  rewriteModel.value =
    config.model ?? providerDefaults[currentRewriteProvider()]?.model ?? "";
  rewriteTimeout.value = String(config.timeout_ms ?? rewriteDefaults.timeout_ms);
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
}

async function boot() {
  applyState({ state: "idle" });

  if (!invoke || !listen) {
    applyConfig(configDefaults);
    settingsMessage.textContent = "预览模式";
    return;
  }

  await listen("realtime-state", (event) => applyState(event.payload));
  await listen("runtime-error", (event) => {
    const payload = event.payload;
    applyState({ state: "error", error: payload?.error ?? "运行时错误" });
  });

  applyConfig(await invoke("get_config"));
  settingsMessage.textContent = "运行中";
  await invoke("start_runtime");
}

settingsToggle.addEventListener("click", () => {
  settingsPanel.hidden = !settingsPanel.hidden;
});

settingsTabs.forEach((button) => {
  button.addEventListener("click", () => switchSettingsTab(button.dataset.settingsTab));
});

rewriteEnabled.addEventListener("change", updateRewriteSummary);
rewriteProfile.addEventListener("change", updateRewriteSummary);
document.querySelectorAll('input[name="rewrite-provider"]').forEach((input) => {
  input.addEventListener("change", () => {
    if (!rewriteModel.value.trim()) {
      rewriteModel.value = providerDefaults[currentRewriteProvider()]?.model ?? "";
    }
    updateRewriteSummary();
  });
});

saveSettings.addEventListener("click", async () => {
  if (activeSettingsTab === "rewrite") {
    updateRewriteSummary();
    settingsMessage.textContent = "改写设置预览中，保存接线在下一步";
    return;
  }

  settingsMessage.textContent = "保存中...";
  try {
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
