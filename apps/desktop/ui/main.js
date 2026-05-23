const labels = {
  idle: "待机",
  recording: "录音中",
  transcribing: "转写中",
  completed: "完成",
};

const dot = document.querySelector(".status-dot");
const stateLabel = document.querySelector("#state-label");
const lastTranscript = document.querySelector("#last-transcript");

function applyState(event) {
  const state = event?.state ?? "idle";
  dot.dataset.state = state;
  stateLabel.textContent = labels[state] ?? labels.idle;

  if (event?.transcript) {
    lastTranscript.textContent = event.transcript;
  }
}

applyState({ state: "idle" });

window.__XENGINEER_APPLY_STATE__ = applyState;
