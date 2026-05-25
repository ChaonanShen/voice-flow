# Windows Manual QA Matrix

本文用于 `voice-flow` 的 Windows 人工交互验收。

用途：

- 告知当前版本有哪些需要人工确认的功能。
- 提供统一的手工测试顺序。
- 记录兼容矩阵，避免只在单一应用里验证。

---

## 1. 基本信息

| 项目 | 填写 |
|---|---|
| 测试日期 |  |
| 测试人 |  |
| Windows 版本 |  |
| Rust / Cargo 版本 |  |
| 启动方式 | `cargo run` / `voice-flow-desktop.exe` / `MSI` / `NSIS` |
| 构建产物 |  |
| 模型目录 |  |
| `SHERPA_ONNX_ARCHIVE_DIR` |  |
| local / cloud ASR |  |
| rewrite provider |  |
| 备注 |  |

---

## 2. 当前功能清单

本轮建议人工验收覆盖这些功能：

| 功能域 | 当前能力 | 需要人工确认 |
|---|---|---|
| 启动与常驻 | 桌面端可启动、关闭进托盘、托盘可恢复/暂停/退出 | 是 |
| 悬浮窗模式 | 小圆形麦克风悬浮窗、录音状态动效、自动粘贴到外部应用 | 是 |
| 文稿模式 | 内部编辑区、原始转写/最终输出差异、插入/替换/追加三种写入方式 | 是 |
| Output Adapter | `floating_input` 粘贴外部应用；`voice_pad` 写内部编辑区 | 是 |
| ASR | `local` / `cloud` engine selection | 是 |
| Rewrite | `clean` / `polish` / `email` / `wechat` / `bullets` / `commit` / `prompt` / `multi` | 是 |
| Voice Command | 语音命令覆盖默认 rewrite profile | 是 |
| Variants | multi variants 切换、复制、写入文稿 | 是 |
| Diagnostics | config/log/model/asr/rewrite 状态显示 | 是 |
| Trace | trace overlay 展示 preprocess / LLM / fallback | 是 |
| Keyring | rewrite provider key / cloud ASR key 持久化 | 是 |
| 打包产物 | `exe` / `MSI` / `NSIS setup.exe` 已生成 | 是 |

---

## 3. 预检查

| 检查项 | 预期 | 结果 |
|---|---|---|
| 应用可启动 | 主窗口或悬浮窗出现 |  |
| 默认快捷键可见 | `Ctrl+Alt+Space` |  |
| 模型目录已配置 | diagnostics 显示存在 |  |
| 日志目录可打开 | 设置页“打开日志目录”可用 |  |
| output mode 可切换 | 悬浮窗模式 / 文稿模式 / 设置 |  |
| ASR engine 可切换 | local / cloud |  |
| rewrite 设置可见 | provider / profile / model / timeout |  |

---

## 4. 主流程验收

### 4.1 悬浮窗模式

| 编号 | 场景 | 预期 | 结果 | 备注 |
|---|---|---|---|---|
| F1 | 点击麦克风开始/结束录音 | 状态从 idle -> recording -> transcribing/rewriting -> completed |  |  |
| F2 | `Ctrl+Alt+Space` 按住说话 | 不用点窗口即可录音 |  |  |
| F3 | local ASR + 外部输入框 | 文本自动粘贴到当前外部应用 |  |  |
| F4 | cloud ASR + 外部输入框 | 文本自动粘贴到当前外部应用 |  |  |
| F5 | rewrite=off | 粘贴 ASR 原文 |  |  |
| F6 | rewrite=clean | 粘贴 clean 结果 |  |  |
| F7 | paste failure 场景 | GUI 有可理解错误提示 |  |  |

### 4.2 文稿模式

| 编号 | 场景 | 预期 | 结果 | 备注 |
|---|---|---|---|---|
| P1 | 切换到文稿模式 | 窗口展开，显示编辑区 |  |  |
| P2 | local ASR 录音 | 结果写入内部编辑区，不往外部应用粘贴 |  |  |
| P3 | cloud ASR 录音 | 结果写入内部编辑区，不往外部应用粘贴 |  |  |
| P4 | `插入光标` | 新结果插入当前光标位置 |  |  |
| P5 | `替换全文` | 新结果替换全文稿 |  |  |
| P6 | `追加末尾` | 新结果追加到文稿末尾 |  |  |
| P7 | 差异视图 | `原始转写` / `最终输出` 同步更新 |  |  |
| P8 | 复制全文稿 | 当前编辑区内容可复制 |  |  |
| P9 | 清空文稿 | 编辑区清空 |  |  |

### 4.3 Rewrite

| 编号 | 场景 | 预期 | 结果 | 备注 |
|---|---|---|---|---|
| R1 | clean | 去口头禅、补标点 |  |  |
| R2 | email | 输出邮件正文 |  |  |
| R3 | prompt | 输出结构化 prompt |  |  |
| R4 | multi | variants 面板出现多个版本 |  |  |
| R5 | variants 切换 | tab 切换显示正确 |  |  |
| R6 | variants 复制 | 当前 variant 可复制 |  |  |
| R7 | variants 写入文稿 | 当前 variant 可写入编辑区 |  |  |
| R8 | voice command | 语音命令可覆盖默认 profile |  |  |
| R9 | trace overlay | 可看到 preprocess / llm / fallback / error |  |  |
| R10 | 缺 key / 超时 / fallback | UI 提示清楚且不崩溃 |  |  |

### 4.4 Settings / Diagnostics / Tray

| 编号 | 场景 | 预期 | 结果 | 备注 |
|---|---|---|---|---|
| S1 | 保存模型目录 | 保存后可热重载生效 |  |  |
| S2 | 保存快捷键 | 保存后可热重载生效 |  |  |
| S3 | 保存 ASR cloud key | 重启后仍保留 |  |  |
| S4 | 保存 rewrite key | 重启后仍保留 |  |  |
| S5 | 清除 ASR cloud key | cloud ASR 给出清晰报错 |  |  |
| S6 | 清除 rewrite key | rewrite fallback / 报错符合预期 |  |  |
| S7 | 刷新 diagnostics | 状态更新正确 |  |  |
| S8 | 打开日志目录 | Explorer 打开日志目录 |  |  |
| S9 | 关闭窗口 | 进入托盘，不退出进程 |  |  |
| S10 | 托盘暂停 | 暂停后不能触发录音 |  |  |
| S11 | 托盘恢复 | 恢复后可再次录音 |  |  |
| S12 | 托盘退出 | 进程退出并释放资源 |  |  |

---

## 5. 兼容矩阵

说明：

- `悬浮窗模式`：是否能自动粘贴到该应用。
- `文稿模式`：是否保持只写内部编辑区，不影响外部应用。
- `local ASR` / `cloud ASR` / `rewrite`：该应用场景下是否整体可用。

| 目标应用 | 悬浮窗模式自动粘贴 | 文稿模式只写内部编辑区 | local ASR | cloud ASR | rewrite | 备注 |
|---|---|---|---|---|---|---|
| Notepad |  |  |  |  |  |  |
| VS Code |  |  |  |  |  |  |
| 浏览器输入框 |  |  |  |  |  |  |
| Outlook / Web Mail |  |  |  |  |  |  |
| 微信 / 企业微信 |  |  |  |  |  |  |

---

## 6. 问题记录

| 编号 | 问题描述 | 复现步骤 | 严重级别 | 是否稳定复现 | 备注 |
|---|---|---|---|---|---|
| 1 |  |  |  |  |  |
| 2 |  |  |  |  |  |
| 3 |  |  |  |  |  |

---

## 7. 最终结论

| 项目 | 结论 |
|---|---|
| 是否可演示 |  |
| 是否可日常试用 |  |
| 是否满足本轮验收目标 |  |
| 仍需修复的问题 |  |
