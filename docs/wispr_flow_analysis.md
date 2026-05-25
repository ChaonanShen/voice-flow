# Wispr Flow 功能与实现分析

## 1. 产品定位

Wispr Flow 本质上不是普通语音转文字工具，而是 **AI 语音输入法 / AI Dictation 工具**。

它解决的不是：

> 我说什么，它打什么。

而是：

> 我自然说话，它自动变成可直接发送、可阅读、符合场景的文本。

## 2. 核心功能

| 功能 | 说明 |
|---|---|
| 语音输入 | 用户通过快捷键/悬浮入口说话，系统自动转文字 |
| 自动清理 | 去除口头禅、修正语法、补标点、优化句子 |
| 智能格式化 | 根据语境自动断句、分段、形成自然文本 |
| 上下文感知 | 结合当前输入框/应用场景理解用户想写什么 |
| 个性化 | 适应用户常用词、专有名词、表达习惯 |
| 命令模式 | 用户可以说“改正式一点”“变短一点”等语音指令 |
| 跨应用输入 | 在桌面、移动端不同 App 中使用 |
| 多语言/混合语言 | 支持 code-switching，即中英混说等情况 |

Wispr 官方技术文章提到，他们在建设 context-aware、personalized、code-switched ASR，以及 personalized LLMs with token-level formatting control。

参考链接：
- https://wisprflow.ai/post/technical-challenges
- https://www.baseten.co/resources/customers/wispr-flow/

## 3. 公开资料中的实现方式

从公开资料看，Wispr Flow 并不是简单调用一个通用大模型。

它更像是：

```text
语音输入
→ 自研/优化 ASR
→ 上下文理解
→ 个性化处理
→ 微调 LLM 改写/格式化
→ 低延迟推理部署
→ 插入到用户当前应用
```

Baseten 客户案例明确提到，Wispr Flow 使用 **fine-tuned Llama models**，并且通过 Baseten + AWS 做低延迟推理和多步骤 inference pipeline。

公开案例还提到，他们希望模型在低延迟条件下处理并生成 100+ tokens，目标是 250ms 级别的推理延迟。

## 4. 值得关注的设计点

### 4.1 Auto Cleanup 分级

Wispr Flow 有类似 Auto Cleanup 的能力，可以选择不同清理强度：

| 清理强度 | 可能效果 |
|---|---|
| None | 尽量保留原始转写 |
| Light | 去除口头禅、轻度语法清理 |
| Medium | 让表达更清楚、更简洁 |
| High | 明显重写和润色 |

这个非常值得借鉴，因为它把“AI 润色”变成了用户可控功能。

### 4.2 多步 Pipeline

它不是一步完成，而是多步处理：

```text
听清楚
→ 理解用户意图
→ 识别上下文
→ 应用用户偏好
→ 输出最终文本
```

这说明核心壁垒不只是模型，而是 **AI 编排层 + 产品体验层**。

### 4.3 个性化

它强调 personalized ASR 和 personalized LLM。也就是说，系统需要逐渐理解：

- 用户常说的人名
- 公司名
- 专业术语
- 常用表达方式
- 常用写作风格

### 4.4 低延迟体验

语音输入法非常依赖速度。用户不能等太久，所以 Wispr Flow 需要专门优化推理和部署。

比赛版可以不用做到这么极致，但要在体验上模拟：

```text
先出原始转写
再出清理版
最后出场景化版本
```

## 5. 总结

Wispr Flow 的核心不是单个模型，而是：

```text
优化 ASR
+ 微调 LLM
+ 多步 AI Pipeline
+ 上下文感知
+ 个性化
+ 低延迟体验
+ 跨应用交互
```

不需要复刻它的底层模型能力，但可以借鉴它的产品结构和交互逻辑。
