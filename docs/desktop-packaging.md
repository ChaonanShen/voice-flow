# Desktop Packaging Guide

本文记录 `voice-flow` Windows 桌面端的本地打包步骤和最小 smoke 流程。

## 前置条件

- Windows 原生环境
- Rust stable + MSVC toolchain
- Visual Studio 2022 Build Tools（含 C++ workload）
- 已准备 sherpa-onnx Windows 静态库 archive
- 已准备模型目录

## 打包命令

在仓库根目录执行：

```powershell
$env:SHERPA_ONNX_ARCHIVE_DIR = "$env:USERPROFILE\.cache\voice-flow\sherpa-onnx"
& C:\Users\16867\.cargo\bin\cargo.exe tauri build --manifest-path apps/desktop/src-tauri/Cargo.toml
```

如果本机没有安装 `cargo-tauri`，先安装：

```powershell
& C:\Users\16867\.cargo\bin\cargo.exe install tauri-cli --version "^2"
```

## 产物位置

默认产物位于：

```text
apps\desktop\src-tauri\target\release\bundle\
```

具体子目录取决于 Tauri 当前启用的 bundle target。

## 首次安装后 smoke checklist

1. 能正常启动应用。
2. 悬浮窗模式默认显示小圆形麦克风。
3. 文稿模式可切换，编辑区可见。
4. 设置页能看到模型目录、ASR engine、rewrite 设置和 diagnostics。
5. Local ASR 下按住 `Alt+Space` 或点击麦克风可以完成一次转写。
6. 文稿模式下结果写入内部编辑区，不自动粘贴外部应用。
7. 悬浮窗模式下结果会自动粘贴到当前输入框。
8. 如果切到 cloud ASR 或启用 rewrite，缺 key 时应给出可理解错误，不静默 fallback。

## 已知限制

- 首启模型目录缺失时，目前仍依赖 diagnostics / settings 提示，尚未做专门的 first-run wizard。
- 实际 installer 兼容结果仍需补 Windows 实机记录。
