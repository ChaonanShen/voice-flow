# 已知未解决问题

记录已发现但暂未根治的 bug / 不优雅的行为。每条按"现象 / 复现 / 已尝试 / 推测原因 / 后续方向"格式。

---

## 1. 首次启动 DeepSeek key 弹窗：PowerShell 进程后台残留

**现象**

- 通过 `voice-flow_0.1.0_x64-setup.exe` 安装并启动应用后，任务管理器里会出现一个 `powershell.exe` 子进程，并且**持续常驻**，即便没有可见控制台窗口（`MainWindowTitle` 为空）。
- 该进程不会随用户在 InputBox 上点击"确定/取消"而立即退出，也观察不到明显的进程退出时机。

**复现**

1. 用 NSIS installer 装上 v0.1.0
2. 删除 `%APPDATA%\voice-flow\.skip-deepseek-prompt`，并清掉 keyring 里的 `voice-flow:DeepSeek` 条目（保证首次启动逻辑会触发）
3. 启动 voice-flow（开始菜单图标或安装最后的"启动"勾选）
4. `Get-Process powershell*` 能看到一个新的 PID，时间戳与启动同步

**已尝试**

- `commit 310eabc`：给 `Command::new("powershell")` 加 `CREATE_NO_WINDOW (0x08000000)` flag。
  - 效果：黑窗确实不显示了（之前会有控制台一闪而过 / 短暂显示）。
  - 没解决的：进程本身仍然驻留，没退出。

**推测原因**

可能性 A — **InputBox modal 卡住**
- `[Microsoft.VisualBasic.Interaction]::InputBox(...)` 是同步 modal 调用；如果对话框没有真正显示（被遮挡 / 失焦 / 在隐藏的窗口栈底），用户根本看不到，自然也无法点关闭，于是 PowerShell 一直阻塞在这一行。
- 但任务管理器里若同时有一个非 voice-flow 主窗口的对话框，理论上 Alt+Tab 能看到——之前一次 debug 里没找到这个对话框窗口。

可能性 B — **PowerShell exec 路径异常**
- `powershell` 可执行文件被某些 PATH 解析重定向到一个 wrapper，wrapper 没退出。
- 但 git bash 里 `Get-Process` 看到的可执行名确实是 `powershell`，时间戳也对得上。

可能性 C — **`[Console]::Out.Write($key)` 没刷新**
- 脚本末尾 `[Console]::Out.Write($key)`，然后预期 PowerShell 进程自然退出。如果 stdout pipe 因为父进程读取顺序不对而阻塞，PowerShell 可能卡在 IO flush 阶段。
- `Command::output()` 在 Rust 这边会读完 stdout 才返回，但若 PowerShell 自己 hang 在 Write，子进程也不会结束。

**后续方向**（按推荐度排）

1. **改用 Win32 原生 API 弹窗**（最稳）
   - 不依赖 PowerShell；用 Rust crate 如 [`native-dialog`](https://crates.io/crates/native-dialog) 或自己调 `MessageBoxW` / `DialogBoxParamW`。
   - 缺点：MessageBoxW 没有原生"输入框"，要么用第三方 input dialog crate，要么自绘窗口（复杂）。
2. **改用 `tauri-plugin-dialog`** + JS 端 `prompt()`
   - Tauri 2 自带 `dialog` plugin 没有 input prompt，但可以在 WebView 里用 `window.prompt()`——前提是你愿意改 UI。
   - 之前用户明确说"不要改 UI"，所以这条要看用户是否松口。
3. **InputBox 改用 `Out-Default` 或 `Write-Host`**
   - 把 stdout 写改为别的方式，避免 Write/flush 阻塞。
4. **在 PowerShell 脚本末尾显式 `[Environment]::Exit($exitcode)`**
   - 强制进程退出，绕过自然清理路径。

**临时缓解**

- 用户可以在任务管理器手动结束那个 `powershell.exe` 进程，不影响 voice-flow 主程序运行。
- 首次启动后写了 `.skip-deepseek-prompt` 标记，**下次启动不再弹** → 这个常驻进程只在首次启动期间出现一次，长期使用没影响。

**优先级**

中。功能上不影响使用，只是任务管理器多一行/吃几 MB 内存。release 演示前可以接受。
