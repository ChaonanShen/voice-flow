# AI rewrite demo script

This demo verifies the text rewrite engine without the desktop GUI.

## Prerequisites

- Rust toolchain is installed.
- Optional live LLM calls require `DEEPSEEK_API_KEY` in the environment or `.env`.
- Default model: `deepseek-chat`.

## Text-only rewrite

Use a fixed oral-style sample:

```powershell
$sample = "嗯，跟老师说一下，今天下午可能因为地铁晚点要晚到十分钟左右，让他不要等我，那个语气正式一点。"
```

Baseline, no LLM call:

```powershell
$sample | cargo run -p voice-cli -- rewrite --profile off
```

Expected:

- Output is the preprocessed transcript.
- Filler words such as `嗯` and `那个` are removed.
- No network key is required.

Clean rewrite with DeepSeek:

```powershell
$sample | cargo run -p voice-cli -- rewrite --profile clean --provider deepseek
```

Expected:

- Output is non-empty.
- Meaning is preserved.
- Filler words are removed and punctuation is improved.

Email rewrite:

```powershell
$sample | cargo run -p voice-cli -- rewrite --profile email --provider deepseek
```

Expected:

- Output is a short, polite Chinese email body.
- Facts such as `老师`, `地铁`, and `十分钟` remain.

Multi-version rewrite:

```powershell
$sample | cargo run -p voice-cli -- rewrite --profile multi --provider deepseek
```

Expected:

- The main output is the `clean` variant.
- The underlying engine asks the provider for JSON variants: `clean`, `polish`, `wechat`, `bullets`.

## Live smoke tests

Run ignored live tests manually:

```powershell
cargo test -p voice-rewrite --test live_deepseek_examples -- --ignored --nocapture --test-threads=1
```

Expected:

- `live_deepseek_clean_smoke` succeeds without fallback.
- `live_deepseek_multi_smoke` prints all 4 variants.
- `live_deepseek_timeout_falls_back` confirms timeout fallback to preprocessed text.

## WAV-to-rewrite path

The committed fixture is `docs/fixtures/demo-rewrite.wav`.

It is a 16 kHz mono PCM WAV generated from the demo sentence with the
Windows `Microsoft Huihui Desktop` zh-CN voice. The local Zipformer model
currently transcribes it as:

```text
我要写个邮件就是跟老师说一下今天下午可能因为地铁晚点要晚到十分钟左右让他不要等我那个语气正是一点
```

Run ASR only:

```powershell
cargo run -p voice-cli -- transcribe docs/fixtures/demo-rewrite.wav
```

Run ASR plus clean rewrite:

```powershell
cargo run -p voice-cli -- transcribe docs/fixtures/demo-rewrite.wav --rewrite clean --rewrite-provider deepseek
```

Expected:

- ASR runs first.
- CLI logs `state: rewriting`.
- Final stdout is the rewritten text.
