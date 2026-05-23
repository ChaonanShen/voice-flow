# Windows 测试指南

本文用于在 Windows 原生环境中验证当前语音转文本框架。建议先用 CLI 跑通端侧 ASR，再验证麦克风、全局快捷键、剪贴板和自动粘贴链路。

不要在 WSL 里做完整链路测试，因为全局快捷键、麦克风、剪贴板和模拟粘贴都需要 Windows 原生 API。

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
git clone <你的仓库地址> VoiceFlow
cd VoiceFlow
```

如果已经有本仓库，直接进入仓库目录即可。

## 3. 准备 Windows 版 sherpa 静态库和模型

当前 `scripts/download-models.sh` 默认下载 Linux 静态库，Windows 测试不要直接使用它。需要准备下面两个文件：

- `sherpa-onnx-v1.13.2-win-x64-static-MT-Release-lib.tar.bz2`
- `sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2`

如果测试机不能访问 GitHub，请先在可联网机器下载这两个 archive，再复制到：

```powershell
$env:USERPROFILE\.cache\xengineer\sherpa-onnx
```

如果测试机可以直接访问 GitHub，可用下面的 PowerShell 命令下载：

```powershell
$version = "1.13.2"
$cache = "$env:USERPROFILE\.cache\xengineer\sherpa-onnx"
$modelRoot = "models"
$base = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v$version"

New-Item -ItemType Directory -Force $cache, $modelRoot | Out-Null

$lib = "sherpa-onnx-v$version-win-x64-static-MT-Release-lib.tar.bz2"
$model = "sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2"

Invoke-WebRequest "$base/$lib" -OutFile "$cache\$lib"
Invoke-WebRequest "$base/$model" -OutFile "$cache\$model"

tar -xjf "$cache\$model" -C $modelRoot
```

如果文件已经由其他机器复制到 `$cache`，只需要执行最后一行解压模型。

## 4. 编译 CLI

```powershell
$env:SHERPA_ONNX_ARCHIVE_DIR = "$env:USERPROFILE\.cache\xengineer\sherpa-onnx"

cargo build -p voice-cli
```

如需 release 构建：

```powershell
cargo build -p voice-cli --release
```

## 5. 验证 WAV 转写

先跑模型自带的测试音频。这一步只验证 sherpa 静态库、模型目录、Rust 绑定和 `voice-cli transcribe` 是否正常。

```powershell
$modelDir = "$PWD\models\sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20"
$env:XENGINEER_SHERPA_ZIPFORMER_MODEL_DIR = $modelDir

cargo run -p voice-cli -- transcribe "$modelDir\test_wavs\0.wav"
```

能输出一段中英混合文本，就说明端侧 ASR 主链路已经跑通。

## 6. 验证麦克风录音

```powershell
cargo run -p voice-cli -- record out.wav --duration 5s
cargo run -p voice-cli -- transcribe out.wav --model-dir "$modelDir"
```

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

## 常见问题

- `link.exe not found`：使用 `Developer PowerShell for VS 2022`，或补装 Visual Studio Build Tools 的 C++ workload。
- 找不到 `win-x64-static-MT-Release-lib` archive：确认下载的是 Windows archive，不是 Linux archive。
- 快捷键注册失败：`Ctrl+Alt+Space` 可能被其他软件占用。
- 粘贴失败：如果目标程序以管理员权限运行，`voice-cli` 也需要以管理员权限运行。

## 8. 记录实测结果

完成 Windows 原生环境测试后，在对应 PR 描述或 `plan.md` Step 5 记录：

- Windows 版本和 Rust toolchain
- 使用的 sherpa archive 文件名和模型目录
- `transcribe` / `record` / `listen-hotkey` / `push-to-talk-transcribe` 的实际结果
- 测试目标程序，例如记事本或 VS Code
- 失败项的完整报错和复现步骤
