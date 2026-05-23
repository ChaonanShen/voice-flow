const labels = {
  idle: "待机",
  recording: "录音中",
  transcribing: "转写中",
  completed: "完成",
  error: "出错",
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

const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;

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

function applyConfig(config) {
  modelDirInput.value = config.model_dir ?? "";
  hotkeyCtrl.checked = Boolean(config.hotkey?.ctrl);
  hotkeyAlt.checked = Boolean(config.hotkey?.alt);
  hotkeyShift.checked = Boolean(config.hotkey?.shift);
  hotkeyLogo.checked = Boolean(config.hotkey?.logo);
  hotkeyKey.value = config.hotkey?.key ?? "Space";
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
  };
}

async function boot() {
  applyState({ state: "idle" });

  if (!invoke || !listen) {
    settingsMessage.textContent = "Tauri API 未加载";
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

saveSettings.addEventListener("click", async () => {
  settingsMessage.textContent = "保存中...";
  try {
    const config = await invoke("save_config", { config: readConfig() });
    applyConfig(config);
    settingsMessage.textContent = "已保存并重新加载";
  } catch (error) {
    settingsMessage.textContent = String(error);
  }
});

window.__XENGINEER_APPLY_STATE__ = applyState;

boot().catch((error) => {
  applyState({ state: "error", error: String(error) });
});
