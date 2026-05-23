#!/usr/bin/env bash
set -euo pipefail

SHERPA_VERSION="${SHERPA_VERSION:-1.13.2}"
CACHE_DIR="${VOICE_FLOW_SHERPA_CACHE_DIR:-$HOME/.cache/voice-flow/sherpa-onnx}"
MODELS_DIR="${VOICE_FLOW_MODELS_DIR:-models}"
EXTRACT="${EXTRACT:-1}"

LIB_ARCHIVE="sherpa-onnx-v${SHERPA_VERSION}-linux-x64-static-lib.tar.bz2"
MODEL_ARCHIVE="sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2"
BASE_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v${SHERPA_VERSION}"
LIB_URL="${BASE_URL}/${LIB_ARCHIVE}"
MODEL_URL="${BASE_URL}/${MODEL_ARCHIVE}"
MODEL_DIR="${MODELS_DIR}/${MODEL_ARCHIVE%.tar.bz2}"

mkdir -p "$CACHE_DIR" "$MODELS_DIR"

fetch() {
  local url="$1"
  local out="$2"
  if [[ -s "$out" ]]; then
    echo "exists: $out"
    return
  fi

  echo "download: $url"
  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 --retry-delay 2 -o "$out" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$out" "$url"
  else
    echo "error: curl or wget is required" >&2
    exit 1
  fi
}

fetch "$LIB_URL" "$CACHE_DIR/$LIB_ARCHIVE"
fetch "$MODEL_URL" "$CACHE_DIR/$MODEL_ARCHIVE"

if [[ "$EXTRACT" == "1" ]]; then
  if [[ -d "$MODEL_DIR" ]]; then
    echo "exists: $MODEL_DIR"
  else
    echo "extract: $CACHE_DIR/$MODEL_ARCHIVE -> $MODELS_DIR"
    tar -xjf "$CACHE_DIR/$MODEL_ARCHIVE" -C "$MODELS_DIR"
  fi
fi

cat <<EOF

Done.

Build with local sherpa archive:
  export SHERPA_ONNX_ARCHIVE_DIR=$CACHE_DIR
  cargo build

Transcribe a bundled test WAV:
  export VOICE_FLOW_SHERPA_ZIPFORMER_MODEL_DIR=$MODEL_DIR
  cargo run -p voice-cli -- transcribe "$MODEL_DIR/test_wavs/0.wav"
EOF
