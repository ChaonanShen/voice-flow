# voice-flow

按住快捷键说话、松开自动粘贴的语音输入法。端侧 ASR 默认，Windows-first。

详细路线见 [plan.md](./plan.md)。本文件只记录从代码本身看不出来的踩坑约束。

## 平台策略

- **Windows 是唯一桌面验收目标**。Linux 仅用于 `voice-core` / `voice-asr-local` / `voice-cli` 的纯逻辑与文件回放测试，**不安装也不调试 Tauri/WebKit/DBus 等桌面依赖**。
- 桌面真实链路（麦克风、全局快捷键、剪贴板、粘贴、Tauri 窗口）只在 Windows 原生环境跑，不要拿 Linux 结果推断 Windows 行为。

## Windows 文档读取

- 在 **Windows PowerShell** 下读取 `plan.md`、`README.md`、`desktop_gui_plan.md`、`ai_rewrite_plan.md` 等中文文档时，**必须显式指定 UTF-8**，不要依赖默认编码。
- 标准读法：
  - `Get-Content -Path plan.md -Encoding UTF8`
  - `Get-Content -Path ai_rewrite_plan.md -Encoding UTF8`
  - `Get-Content -Path desktop_gui_plan.md -Encoding UTF8`
  - `Get-Content -Path README.md -Encoding UTF8`
- 如果需要整文件读取，也保持显式编码：`Get-Content -Path plan.md -Encoding UTF8 -Raw`
- 未显式带 `-Encoding UTF8` 时，PowerShell 在当前机器上多次出现中文乱码；后续 Codex 会话默认按上面的命令读取。

## 不要合并的"重复"实现

- **CLI 和桌面壳故意用两套全局快捷键实现**：
  - CLI ([crates/voice-core/src/hotkey.rs](crates/voice-core/src/hotkey.rs))：`global-hotkey` crate
  - 桌面 ([apps/desktop/src-tauri/src/main.rs](apps/desktop/src-tauri/src/main.rs) 的 `DesktopHotkey`)：Tauri `global-shortcut` 插件
  - 原因：Windows 实测发现后台线程直接注册 Windows 热键不稳定。**不要"统一"它们**。

## 编译环境

- `voice-asr-local` / 桌面壳依赖 sherpa-onnx 静态库 archive，**必须**设置 `SHERPA_ONNX_ARCHIVE_DIR` 指向解压目录，`build.rs` 从本地读，不联网下载。运行测试或编译前确认 `$HOME/.cache/voice-flow/sherpa-onnx`（Linux）或对应 Windows 缓存目录已就绪。
- 真实模型加载测试通过 `VOICE_FLOW_SHERPA_ZIPFORMER_MODEL_DIR` 环境变量触发；未设置时自动 skip。

## 音频后端

- [cpal_backend.rs](crates/voice-core/src/cpal_backend.rs) **必须保留通道数回退**：优先单声道，失败时落到设备可用通道。Windows 默认麦克风经常只暴露双声道，移除这段回退会直接报 `no input config matches channels=1`。

## Tauri v2 capability

- 前端要监听任何事件都必须在 [capabilities/default.json](apps/desktop/src-tauri/capabilities/default.json) 显式声明权限。当前已包含 `global-shortcut:default`。改前端事件名时检查这里。

## 测试 / 验收

- `cargo test --workspace` 跑 voice-core / voice-asr-local 的纯逻辑测试，**不覆盖桌面链路**。
- 桌面端验收必须在 Windows 上人工跑：启动 `apps/desktop/src-tauri` → 按住 `Alt+Space` → 松开后看悬浮窗"最近文本"更新且粘贴生效。
- 主分支必须保持可运行：每个 PR 合并后评委可任意时间检出能跑（见 plan.md §6）。

## 配置

- 桌面配置文件：`%APPDATA%\voice-flow\app.toml`，由 `AppConfig::write_to` 写入（[config.rs](crates/voice-core/src/config.rs)）。
- 保存设置会触发 `restart_requested`，由 `runtime_loop` 重新加载模型 + 重注册热键，**无需关闭窗口**。改这套热重载逻辑时记得 drop 旧的 `DesktopHotkey` 才能释放 Tauri 注册。
