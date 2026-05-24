# voice-flow demo script

本文是当前版本的总演示脚本，覆盖：

- 悬浮窗模式
- 文稿模式
- local / cloud ASR engine
- AI rewrite

## 1. 启动桌面端

```powershell
$env:SHERPA_ONNX_ARCHIVE_DIR = "$env:USERPROFILE\.cache\voice-flow\sherpa-onnx"
& C:\Users\16867\.cargo\bin\cargo.exe run --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## 2. 悬浮窗模式 + local ASR

1. 确认设置页 `ASR engine = local`。
2. 切回悬浮窗模式。
3. 聚焦 Notepad 或浏览器输入框。
4. 按住 `Alt+Space` 说一句普通中文。
5. 松开后，观察：
   - 悬浮窗状态变化：Recording -> Transcribing -> Completed
   - 文本自动粘贴到当前输入框

## 3. 悬浮窗模式 + rewrite

1. 设置页开启 `AI 改写`。
2. profile 选 `clean`。
3. 聚焦外部输入框。
4. 说一段带口头禅的话。
5. 松开后，观察：
   - 输出为 clean 改写结果
   - 文稿模式中的最近转写 / 最终输出可以看到差异

## 4. 文稿模式

1. 切到文稿模式。
2. 说一句话。
3. 松开后，观察：
   - 不再自动粘贴到外部应用
   - 最终结果写入内部编辑区
   - 原始转写 / 最终输出差异区同步更新

## 5. 文稿模式写入策略

分别切换：

- `插入光标`
- `替换全文`
- `追加末尾`

每种模式各录一条，确认写入行为正确。

## 6. multi variants

1. 设置页把 rewrite profile 改为 `multi`。
2. 文稿模式下说一句适合多版本改写的话。
3. 观察：
   - variants panel 展示多个版本
   - 点击 tab 可切换版本
   - “写入文稿”会把当前版本写进编辑区

## 7. trace overlay

1. 录一条启用 rewrite 的结果。
2. 点击“查看 trace”。
3. 确认能看到：
   - Profile
   - Preprocess 耗时
   - LLM 耗时
   - 是否 fallback
   - fallback error（如有）

## 8. cloud ASR（人工实测前置）

1. 设置页切 `ASR engine = cloud`。
2. 保存 DashScope key。
3. 再录一条。

预期：

- 如果 key 正确且网络可用，转写成功。
- 如果 key 缺失或网络异常，GUI 应给出明确错误，不静默 fallback 到 local。

## 9. diagnostics / logs

1. 打开设置 -> 诊断。
2. 点击“刷新诊断”。
3. 点击“打开日志目录”。
4. 确认能看到：
   - config path
   - log path
   - model dir 状态
   - runtime 状态
   - ASR / rewrite key 状态

## 10. 托盘

1. 关闭主窗口，确认进入托盘。
2. 从托盘恢复窗口。
3. 从托盘暂停 / 恢复监听。
4. 从托盘退出应用。
