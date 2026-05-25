# Windows 测试指南

本文用于在 Windows 原生环境中验证当前语音转文本框架。建议先用 CLI 跑通端侧 ASR，再验证麦克风、全局快捷键、剪贴板和自动粘贴链路。

不要在 WSL 里做完整链路测试，因为全局快捷键、麦克风、剪贴板和模拟粘贴都需要 Windows 原生 API。

## Windows 使用注意

Codex / PowerShell 会话里的 `PATH` 可能没有继承用户级 Rust 路径。若直接运行 `cargo` 报 `The term 'cargo' is not recognized`，优先使用绝对路径：

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe --version
& C:\Users\16867\.cargo\bin\cargo.exe build -p voice-cli
```

读取中文 Markdown 时固定使用 UTF-8，避免 PowerShell 控制台把文档显示成乱码：

```powershell
$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
Get-Content -Raw -Encoding UTF8 .\windows_testing.md
```

如果仍有显示问题，使用 .NET API 直接按 UTF-8 读文件：

```powershell
[System.IO.File]::ReadAllText("windows_testing.md", [System.Text.Encoding]::UTF8)
```

## 1. 安装环境

安装以下组件：

- Git
- Rust stable，MSVC toolchain
- Visual Studio 2022 Build Tools，并勾选 `Desktop development with C++`

打开 `Developer PowerShell for VS 2022`：

```powershell
rustup default stable
rustup target add x86_64-pc-windows-msvc
```

## 2. 获取代码

```powershell
git clone <你的仓库地址> voice-flow
cd voice-flow
```

如果已经有本仓库，直接进入仓库目录即可。

## 3. 准备 Windows 版 sherpa 静态库和模型

当前 `scripts/download-models.sh` 默认下载 Linux 静态库，Windows 测试不要直接使用它。需要准备下面两个文件：

- `sherpa-onnx-v1.13.2-win-x64-static-MT-Release-lib.tar.bz2`
- `sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2`

其中静态库 archive 属于 `v1.13.2` release；模型 archive 属于 `asr-models` release。

如果测试机不能访问 GitHub，请先在可联网机器下载这两个 archive，再复制到 Windows 测试机：

```powershell
$cache = "$env:USERPROFILE\.cache\voice-flow\sherpa-onnx"
New-Item -ItemType Directory -Force $cache | Out-Null
```

复制后，`$cache` 目录里应能看到这两个 `.tar.bz2` 文件。

如果测试机可以直接访问 GitHub，可用下面的 PowerShell 命令下载：

```powershell
$version = "1.13.2"
$cache = "$env:USERPROFILE\.cache\voice-flow\sherpa-onnx"
$modelRoot = "models"
$libBase = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v$version"
$modelBase = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models"

New-Item -ItemType Directory -Force $cache, $modelRoot | Out-Null

$lib = "sherpa-onnx-v$version-win-x64-static-MT-Release-lib.tar.bz2"
$model = "sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2"

Invoke-WebRequest "$libBase/$lib" -OutFile "$cache\$lib"
Invoke-WebRequest "$modelBase/$model" -OutFile "$cache\$model"

tar -xjf "$cache\$model" -C $modelRoot
```

如果文件已经由其他机器复制到 `$cache`，执行下面的解压命令：

```powershell
$cache = "$env:USERPROFILE\.cache\voice-flow\sherpa-onnx"
$modelRoot = "models"
$model = "sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2"

New-Item -ItemType Directory -Force $modelRoot | Out-Null
tar -xjf "$cache\$model" -C $modelRoot
```

## 4. 编译 CLI

```powershell
$env:SHERPA_ONNX_ARCHIVE_DIR = "$env:USERPROFILE\.cache\voice-flow\sherpa-onnx"

cargo build -p voice-cli
cargo test -p voice-core
```

后续命令建议继续使用同一个 `Developer PowerShell for VS 2022` 窗口；如果新开窗口，需要重新设置 `SHERPA_ONNX_ARCHIVE_DIR` 和 `$modelDir`。

如需 release 构建：

```powershell
cargo build -p voice-cli --release
```

## 5. 验证 WAV 转写

先跑模型自带的测试音频。这一步只验证 sherpa 静态库、模型目录、Rust 绑定和 `voice-cli transcribe` 是否正常。

```powershell
$modelDir = "$PWD\models\sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20"
$env:VOICE_FLOW_SHERPA_ZIPFORMER_MODEL_DIR = $modelDir

cargo run -p voice-cli -- transcribe "$modelDir\test_wavs\0.wav"
```

能输出一段中英混合文本，就说明端侧 ASR 主链路已经跑通。

## 6. 验证麦克风录音

```powershell
cargo run -p voice-cli -- record out.wav --duration 5s
cargo run -p voice-cli -- transcribe out.wav --model-dir "$modelDir"
```

执行 `record` 后请对麦克风说一小段话；如果 `transcribe out.wav` 没有输出有效文本，先确认 `out.wav` 是否能正常播放。

如果录不到声音，检查 Windows 麦克风权限：

`设置 -> 隐私和安全性 -> 麦克风 -> 允许桌面应用访问麦克风`

## 7. 验证完整链路

先测试全局快捷键：

```powershell
cargo run -p voice-cli -- listen-hotkey
```

按 `Ctrl+Alt+Space`，终端应打印 pressed / released 事件。

然后测试完整链路：

```powershell
cargo run -p voice-cli -- push-to-talk-transcribe --model-dir "$modelDir"
```

启动后切到记事本或任意输入框，按住 `Ctrl+Alt+Space` 说话，松开后应该自动完成：

1. 停止录音
2. 端侧 ASR 转写
3. 写入剪贴板
4. 粘贴到当前光标位置

当前 CLI 在完成一次按住说话识别后会退出；这是 Step 5 手测的正常行为。后续桌面外壳会再处理常驻状态。

## 8. 验证桌面应用

桌面应用是 Step 6 / Step 7 的 Windows-first 验证入口。它会默认尝试加载仓库内的模型目录，也可以在设置面板里手动填写模型目录。

```powershell
cd apps\desktop\src-tauri
$env:SHERPA_ONNX_ARCHIVE_DIR = "$env:USERPROFILE\.cache\voice-flow\sherpa-onnx"
cargo run
```

`cargo run` 是开发时最方便的启动方式。只要已经构建过，也可以直接启动生成的桌面程序：

```powershell
cd apps\desktop\src-tauri
.\target\debug\voice-flow-desktop.exe
```

直接运行 exe 时，如果当前机器没有默认配置文件，请先确认 `%APPDATA%\voice-flow\app.toml` 里已经写入正确模型目录，或启动后在设置面板中填写模型目录并保存。

如果在 Git Bash 中运行，使用 Bash 语法设置环境变量：

```bash
cd ~/dev/voice-flow/apps/desktop/src-tauri
export SHERPA_ONNX_ARCHIVE_DIR="$(cygpath -w "$HOME/.cache/voice-flow/sherpa-onnx")"
cargo run
```

启动后应出现一个置顶悬浮窗。切到记事本、VS Code 或任意输入框，按住 `Ctrl+Alt+Space` 说话，松开后应自动完成转写、写入剪贴板并粘贴到当前光标位置；悬浮窗应显示录音、转写、完成状态和最近一次识别文本。

设置面板可以修改：

- 模型目录
- `Ctrl` / `Alt` / `Shift` / `Win` 修饰键
- 主按键，例如 `Space`

点击保存后，桌面后端会重新加载配置并重新注册快捷键。配置文件写入 `%APPDATA%\voice-flow\app.toml`。

## 常见问题

- `link.exe not found`：使用 `Developer PowerShell for VS 2022`，或补装 Visual Studio Build Tools 的 C++ workload。
- 找不到 `win-x64-static-MT-Release-lib` archive：确认下载的是 Windows archive，不是 Linux archive。
- `SHERPA_ONNX_ARCHIVE_DIR does not contain expected archive`：确认 `$env:SHERPA_ONNX_ARCHIVE_DIR` 指向包含 `sherpa-onnx-v1.13.2-win-x64-static-MT-Release-lib.tar.bz2` 的目录。
- 模型目录加载失败：确认 `$modelDir` 指向解压后的目录，且里面包含 `encoder-epoch-99-avg-1.int8.onnx`、`decoder-epoch-99-avg-1.onnx`、`joiner-epoch-99-avg-1.int8.onnx` 和 `tokens.txt`。
- 快捷键注册失败：`Ctrl+Alt+Space` 可能被其他软件占用。
- `event.listen not allowed`：确认 `apps/desktop/src-tauri/capabilities/default.json` 存在，且重新 `cargo build` / `cargo run`。
- `no input config matches channels=1`：说明默认输入设备不暴露单声道采集。当前 `CpalCapture` 会回退到设备支持的通道数；如果仍出现该错误，先确认运行的是最新构建。
- 粘贴失败：如果目标程序以管理员权限运行，`voice-cli` 或桌面应用也需要以管理员权限运行。

## 9. 记录实测结果

## 10. 桌面 GUI QA checklist

建议每次桌面端较大改动后至少覆盖一次下面矩阵：

- 悬浮窗模式：
  - `Ctrl+Alt+Space` 录音 -> local ASR -> 自动粘贴到 Notepad
  - 点击麦克风录音 -> 自动粘贴到浏览器输入框
- 文稿模式：
  - 录音结果写入内部编辑区，不自动粘贴到外部应用
  - `插入光标` / `替换全文` / `追加末尾` 三种写入模式都可用
  - multi variants 可切换，并可“写入文稿”
- rewrite：
  - clean profile 可用
  - email profile 可用
  - voice command 可覆盖默认 profile
  - trace overlay 可打开，能看到 preprocess / LLM / fallback 信息
- ASR engine：
  - local engine 可用
  - cloud engine 在保存 DashScope key 后可用
  - cloud engine 缺 key 时给出清晰报错
- 常驻与设置：
  - 托盘可暂停 / 恢复 / 退出
  - 关闭窗口后进入托盘
  - 保存设置后 runtime 自动重载
  - diagnostics 可刷新
  - “打开日志目录”按钮可用

## 11. App compatibility checklist

至少记录一次下面目标应用的结果：

- Notepad
- VS Code
- 浏览器输入框
- Outlook / Web Mail
- 微信 / 企业微信

对每个目标应用记录：

- 悬浮窗模式是否能自动粘贴
- 文稿模式是否保持只写内部编辑区
- 是否需要管理员权限
- 是否有焦点/快捷键冲突/粘贴失败问题

### 2026-05-23 首轮桌面实测

- 环境：Windows 原生桌面，Git Bash 中运行 `apps/desktop/src-tauri` 的 `cargo run`，Rust / Cargo 可正常使用，MSVC 构建环境可完成桌面构建。
- sherpa 静态库：`%USERPROFILE%\.cache\voice-flow\sherpa-onnx\sherpa-onnx-v1.13.2-win-x64-static-MT-Release-lib.tar.bz2`。
- 模型目录：仓库内 `models\sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20`。
- 结果：桌面悬浮窗可启动；按住 `Ctrl+Alt+Space` 能调用麦克风；松开后完成端侧 ASR 转写，悬浮窗显示最近文本，并自动写入剪贴板 / 粘贴到当前光标位置。
- 已修复问题：Tauri v2 缺 capability 导致 `event.listen not allowed`；Windows 默认麦克风不支持请求的 `channels=1` 导致录音启动失败。

补充：桌面应用不只可以通过 `cargo run` 启动。完成一次 `cargo build` / `cargo run` 后，也可以直接运行 `apps\desktop\src-tauri\target\debug\voice-flow-desktop.exe` 进行手测。

后续完成更多 Windows 原生环境测试后，在对应 PR 描述或 `plan.md` Step 5 继续记录：

- Windows 版本和 Rust toolchain
- 使用的 sherpa archive 文件名和模型目录
- `transcribe` / `record` / `listen-hotkey` / `push-to-talk-transcribe` / 桌面应用的实际结果
- 测试目标程序，例如记事本或 VS Code
- 失败项的完整报错和复现步骤
