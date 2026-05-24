# Debugging Notes

本文档记录项目调试、问题反馈和排障办法。重点覆盖 Windows GUI 调试，因为桌面端问题很容易只剩一张截图，缺少足够上下文。

## Windows GUI 问题怎么反馈

截图有用，但最好和以下信息一起给，Codex 才能快速定位是前端 UI、Tauri 权限、Rust 后端、麦克风、热键还是剪贴板问题。

### 最小反馈包

每次 GUI 异常，尽量提供：

1. 当前执行目录和命令。

   ```bash
   pwd
   cargo run
   ```

2. 终端完整输出。

   包括按快捷键前后的新日志、panic、runtime error、Windows 权限错误。

3. 截图。

   建议至少两张：操作前一张、操作后一张。文件名可以用 `run-before.png` / `run-after.png` / `run-error.png`。

4. 触发步骤。

   例如：

   ```text
   1. 在 apps/desktop/src-tauri 运行 cargo run
   2. 窗口显示待机
   3. 在 VS Code 输入框聚焦
   4. 按住 Alt+Space 说话
   5. 松开快捷键
   6. 期望自动粘贴，实际只更新了最近文本
   ```

5. 当前配置。

   Windows 桌面配置文件默认在：

   ```text
   %APPDATA%\voice-flow\app.toml
   ```

   需要关注模型目录、快捷键配置。路径里如果有隐私信息，可以只保留关键部分。

6. 期望行为和实际行为。

   不要只写“出错了”，尽量写成：

   ```text
   期望：松开快捷键后文本粘贴到 VS Code。
   实际：悬浮窗最近文本更新了，但 VS Code 没有粘贴；终端没有新错误。
   ```

### 推荐反馈格式

可以直接按这个模板发：

````text
环境：
- Windows 版本：
- 运行目录：
- 命令：
- 模型目录：

操作：
1.
2.
3.

期望：

实际：

终端输出：
```text
粘贴 cargo run 输出
```

截图：
- run-before.png
- run-after.png

配置：
```toml
粘贴 %APPDATA%\voice-flow\app.toml 中相关项
```
````

## 截图之外的更好 GUI 调试方法

当前程序 GUI 不复杂，截图足够解决很多问题。但如果后续 UI、设置项、状态流变复杂，应补自动化调试能力。

### 1. 前端静态 UI 自动化

桌面前端在 `apps/desktop/ui/`，本质是 HTML / CSS / JS。可以用 Playwright 打开 `index.html`，注入假的 Tauri API，自动测试：

- 待机、录音、转写、完成、出错状态是否正确显示
- 最近文本是否更新
- 设置面板是否能输入模型目录和快捷键
- 错误信息是否换行、溢出、遮挡
- 小窗口尺寸下布局是否崩坏

优点：快，稳定，不需要真的启动 Tauri、麦克风、快捷键。

限制：只能测 UI 渲染和交互，不能验证真实 Windows 全局快捷键、麦克风、剪贴板和模拟粘贴。

后续可加：

```text
apps/desktop/ui-tests/
```

用例里模拟 `realtime-state` / `runtime-error` 事件，自动截图保存到 `target/ui-screenshots/`。

### 2. Tauri DevTools

开发模式下可以打开 WebView DevTools，用来查看：

- DOM 是否更新
- CSS 是否生效
- 前端 JS 是否报错
- Tauri event / command 调用是否失败

这适合定位“窗口显示不对”“按钮没反应”“前端报错”等问题。

后续可以在 debug build 中加一个临时开发开关，启动时自动打开 DevTools；release build 不打开。

### 3. 后端状态日志

真实链路问题通常不在 UI，而在 Rust 后端：

- 全局快捷键是否注册成功
- 是否收到 pressed / released
- 麦克风设备选择了什么采样率和通道数
- 本次录音有多少采样点
- sherpa 模型是否加载成功
- ASR 输出文本是什么
- 剪贴板写入是否成功
- 模拟粘贴是否成功

当前已有终端错误输出。后续建议增加结构化日志，并写到：

```text
%APPDATA%\voice-flow\logs\desktop.log
```

这样用户不需要一直截图终端，可以直接提供最近日志。

### 4. 保存最近一次录音

ASR 不准或没识别时，最有价值的是保存“刚才录到的音频”。后续可加调试配置：

```toml
[debug]
save_last_recording = true
```

开启后，每次松开快捷键，把最近一次录音保存到：

```text
%APPDATA%\voice-flow\debug\last-recording.wav
```

这样可以判断问题是：

- 麦克风没录到
- 录音音量太小
- 采样格式异常
- 模型识别能力不足
- 后处理或标点问题

### 5. 诊断包脚本

后续可以加一个 Windows 诊断脚本，例如：

```text
scripts/collect-windows-diagnostics.ps1
```

自动收集：

- `rustc -V`
- `cargo -V`
- `cl` / MSVC 环境是否可用
- sherpa Windows 静态库 archive 是否存在
- 模型目录是否存在、关键文件是否齐全
- `%APPDATA%\voice-flow\app.toml`
- 最近日志
- 最近一次录音文件路径

不要自动收集隐私文本、剪贴板内容或完整用户目录。

## 常见问题定位思路

### 窗口打不开

优先看：

- `cargo run` 是否有编译错误
- `tauri.conf.json` 是否有效
- Tauri capability 是否缺权限
- WebView2 runtime 是否正常

### 窗口打开但按钮或状态没反应

优先看：

- 前端 JS 控制台是否报错
- `window.__TAURI__` 是否可用
- Tauri command 是否注册
- capability 是否允许对应 API

### 按快捷键没反应

优先看：

- 快捷键是否被其他软件占用
- Tauri `global-shortcut` 是否注册成功
- 配置里的快捷键 label 是否合法
- 终端是否收到 pressed / released 相关日志

### 按下快捷键但麦克风报错

优先看：

- Windows 是否允许桌面应用访问麦克风
- 默认输入设备是否正确
- 设备是否只支持双声道或特殊采样率
- 终端是否出现 `no input config matches channels=1`

本项目已把录音配置改成“优先单声道，不支持时回退到设备可用通道数”。

### 有最近文本但没有自动粘贴

说明识别链路大概率已经成功，重点看输出链路：

- 目标输入框是否真的聚焦
- 剪贴板是否已写入识别文本
- 目标应用是否拦截模拟粘贴
- Windows 安全软件或管理员权限边界是否影响输入模拟

可以先手动 `Ctrl+V` 验证剪贴板内容。如果手动粘贴成功，问题主要在模拟粘贴；如果手动也没有，问题在剪贴板写入。

### 识别不准或没有标点

优先区分两类问题：

- ASR 模型本身识别不准：需要更强模型、更好音频或云端增强。
- 文本后处理不足：需要 Step 8 的模式系统、标点恢复、符号替换、代码模式等。

当前端侧 sherpa 模型主要负责语音转文字，不应期待它天然完成高质量标点和复杂中英混排修正。
