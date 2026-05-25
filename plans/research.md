# 语音输入法技术调研

> 调研日期：2026-05-23
> 范围：主流语音输入法技术方案 + 端侧本地开源 ASR 模型

---

## 第一部分：主流语音输入法技术方案

### 1. 总览

当前已较为收敛，正经历第二次范式迁移：

- **第一次（2018-2022）**：GMM-HMM → 端到端神经网络，**Conformer / RNN-T / Paraformer** 成为事实标准
- **第二次（2024 至今）**：ASR 与 LLM 融合（字节 Seed-ASR、阿里 Fun-ASR、Google Rambler），把"识别 + 润色 + 上下文理解"做成一体化

工程共识：**端云混合 + 流式 Conformer/Paraformer**。Apple 已经做到完全端侧，Google Pixel 端侧识别 + 端侧 LLM 润色；国内厂商整体仍以云端为主、端侧做兜底。

主要分歧点：
- 是否走 LLM-based ASR（字节、阿里走得最激进，Apple 仍坚持传统 E2E ASR + 后处理）
- 端云比例（Apple/Google 端侧重，国内厂商云端重）
- 方言策略（多方言单模型 vs 单方言专用模型）

### 2. 各产品技术方案

#### 讯飞输入法（科大讯飞）
- **模型架构**：早期 DFCNN（2016 公开），近年迁移到 Conformer/Transformer 端到端，并接入星火认知大模型做语义理解与润色
- **端云策略**：云端为主，App 内置离线小模型做弱网兜底
- **流式与延迟**：支持流式，首字延迟未公开（行业经验值 300-500ms）
- **多语种/方言**：23+ 种方言（粤语、四川话、上海话、东北话等），中英混说免切换。**方言能力是最强差异化标签**
- **后处理**：标点、ITN、智能纠错、星火大模型润色
- **个性化**：用户词库、行业词库（医疗/金融/法律）、热词上传
- **隐私**：默认录音上传云端，提供离线模式
- **备注**：具体在线模型细节不公开，多数信息来自市场宣传与早期专利/论文

#### 百度输入法
- **模型架构**：核心是百度自研的 **SMLTA（流式多层截断注意力）**，2019 年首次部署，迭代到 **SMLTA2** 与 **Deep Peak2**（CTC + 注意力融合）
- **端云策略**：云端为主 + 离线引擎，离线引擎宣传"准确率接近在线"
- **流式与延迟**：SMLTA 是行业首批商用流式注意力模型，宣称首字延迟 < 400ms
- **多语种/方言**：中英混说、多方言一个模型搞定
- **后处理**：标点、ITN、文心大模型润色
- **个性化**：用户词库、上下文语境（与文心打通）
- **隐私**：默认上传，支持本地离线
- **备注**：SMLTA 论文/专利公开较多，国内厂商里技术细节最透明

#### 搜狗输入法（腾讯）/ 微信键盘
- **模型架构**：搜狗"知音引擎"早期 LSTM-CTC，后迁移到 Transformer/Conformer；微信键盘（腾讯天籁实验室）使用类似的端到端模型
- **端云策略**：云端识别为主，搜狗在 Android 端有离线包
- **流式与延迟**：支持流式，参数未公开
- **多语种/方言**：粤语、四川话、东北话等主流方言
- **隐私**：搜狗历史上有过隐私争议，腾讯收购后强化端侧能力。微信键盘主打"输入隐私"
- **备注**：腾讯系输入法技术细节几乎完全不公开

#### Google Gboard / Google 语音输入
- **模型架构**：2019 起在 Pixel 部署纯端侧 **RNN-T**（80MB 量化），后升级到 Conformer。云端 **USM（2B 参数）**，覆盖 100+ 语种。2026 推出 **Rambler**，调用端侧 Gemini Nano 做"语音 → 整理后的文本"
- **端云策略**：Pixel 设备纯端侧；其他 Android 端云结合
- **流式与延迟**：RNN-T 天然流式，端侧首字延迟可做到 < 100ms
- **多语种/方言**：100+ 语种（USM），代码切换开箱支持
- **后处理**：端侧 Transformer 标点模型、Rambler 用 Gemini 去口语化
- **个性化**：联邦学习 / "ephemeral learning"，原始音频不上传
- **隐私**：端侧场景音频不离开设备
- **备注**：Google 公开技术资料最丰富

#### Apple 听写 / Siri Dictation
- **模型架构**：iOS 13 起逐步切换端侧；iOS 26（2025 WWDC）推出 **SpeechAnalyzer + SpeechTranscriber** API，模型完全在 Apple Neural Engine 跑，宣称快于 Whisper。推测为 Conformer + Transducer 类
- **端云策略**：Apple Silicon Mac（M1+）和较新 iPhone 默认端侧
- **流式与延迟**：流式，端侧首字延迟极低（开发者实测 < 200ms）
- **多语种/方言**：60+ 语言，中英混说、Siri 多语模式
- **后处理**：标点、ITN，iOS 26 起可调用 Apple Intelligence 润色
- **个性化**：联系人姓名注入、SFCustomLanguageModelData（开发者可定制）
- **隐私**：端侧处理是核心卖点
- **备注**：Apple 极少公开模型细节

#### 豆包（字节跳动 Seed-ASR）
- **模型架构**：**Audio-Conditioned LLM（AcLLM）**——把连续语音 embedding 直接喂给 LLM 解码，相当于多模态 LLM 做 ASR。LLM backbone 推测数十亿参数。三阶段训练：自监督预训练 → SFT → 上下文 SFT + RL
- **端云策略**：完全云端，模型规模决定无法端侧
- **流式与延迟**：支持流式，具体延迟未公开
- **多语种/方言**：中文 + 英文 + 多种方言；支持上下文人名/术语注入
- **后处理**：LLM 内生标点、ITN、纠错；可结合上下文做语义改写
- **个性化**：通过 prompt 注入热词、人名、领域术语
- **备注**：技术报告（arXiv 2407.04675）公开较详细，是 LLM-ASR 路线代表

#### 通义听悟 / 阿里 FunAudio-ASR
- **模型架构**：底座 **Paraformer**（非自回归 Transformer + Continuous Integrate-and-Fire），单步并行解码、推理快。Fun-ASR 1.5（30B MoE LLM-ASR）覆盖 30 语种 + 汉语七大方言。SenseVoice 是另一条线，多语言 + 情感 + 事件检测
- **端云策略**：通义听悟以云端为主；FunASR 开源版可端侧部署（onnx/sherpa）
- **流式与延迟**：Paraformer 有流式版本；非流式版因并行解码有 RTF 优势
- **多语种/方言**：Fun-ASR 1.5 单模型覆盖七大方言，方言 CER 相比上代降 56.2%
- **后处理**：标点、ITN、热词、LLM 整理
- **个性化**：热词列表 + 上下文偏置
- **备注**：阿里达摩院技术报告与开源生态（FunASR、SenseVoice、CosyVoice）公开度极高，**是中文 ASR 最佳的技术参考来源**

### 3. 关键技术取舍点

#### 准确度 vs 延迟
流式识别只能看到有限的右侧上下文，准确率天然比离线低。主流解法：
- **U2/U2++ 两遍解码**（流式 CTC 出第一遍 → 完整 attention 重打分）
- **CIF/Transducer** 类天然流式架构

输入法场景一般要求 < 500ms 首字延迟。

#### 准确度 vs 成本
- 云端大模型（Seed-ASR、Fun-ASR）准确率最高但每分钟语音都要消耗 GPU
- 端侧蒸馏模型（Gboard 80MB RNN-T、Apple SpeechTranscriber）成本几乎为零但准确率落后云端 2-5 个 CER 点
- **常见折中**：高频短语句走端侧，长语音/低频场景上云

#### 通用性 vs 个性化
基础大模型对长尾人名、专业术语、方言口音是天然弱项。三种增强路径：
- 热词列表 + WFST 偏置（成本最低）
- prompt-tuning（LLM-ASR 适用）
- 联邦学习 / 用户词库自适应（Google、Apple 路线）

输入法场景前两种最实用。

#### 隐私 vs 体验
云端能用更大模型也能持续学习，但音频上传是隐私红线。Apple 端侧 + Google 联邦学习是两条隐私差异化路线；国内厂商默认上传，但需提供合规的离线兜底。

#### LLM-ASR vs 传统 E2E ASR
- LLM-ASR（Seed-ASR、Fun-ASR）准确率上限更高、纠错更智能、能直接做"口语顺滑"，但模型大、延迟高、端侧不可行
- 传统 E2E（Paraformer、Conformer-Transducer）成熟、可控、可端侧
- **短期内输入法应该走"传统 E2E 实时识别 + LLM 异步润色"的混合架构**，与 Google Rambler 思路一致

### 4. 信息缺口

- 讯飞、搜狗、微信键盘的具体模型架构、参数规模、训练数据规模、首字延迟、CER 指标几乎全部未公开
- Apple SpeechTranscriber 模型细节未公开
- 百度 SMLTA2/Deep Peak2 的最新版本与输入法实际部署版本是否一致未明确
- Seed-ASR、Fun-ASR 的实际部署延迟与端侧策略未公开
- 各产品 CER/WER 指标：除非来自论文公开测试集（AISHELL、LibriSpeech），市场宣传的"准确率 98%"不可比

权威来源：百度 SMLTA 论文、Google AI Blog（RNN-T 2019、USM 2023）、Apple WWDC 2023/2025、字节 Seed-ASR（arXiv 2407.04675）、阿里 Paraformer/Fun-ASR/SenseVoice 论文、WeNet 论文。

---

## 第二部分：端侧本地开源 ASR 模型

### 1. 总览

经过 2024-2026 年三波迭代（Whisper.cpp 量化生态成熟、k2-fsa Zipformer/Paraformer ONNX 化、Moonshine v2 流式编码器、Kyutai 延迟流建模），端侧 ASR 已从"PC 玩具"走向真正可商用的手机本地方案。

但**中文输入法场景下可选范围远小于英文场景**——很多 SOTA 模型（Moonshine、Distil-Whisper、Parakeet、Kyutai STT、Canary）要么不支持中文，要么仅有研究质量的中文权重。真正能在普通安卓机上跑、又有可信中文 CER 的方案集中在 **Paraformer / Zipformer / SenseVoice / WeNet U2++**。

#### 输入法第一梯队推荐
（流式、首字 < 500ms、中文为主、中端手机 1-2 GB 内存预算）

- **首选**：sherpa-onnx + Streaming Paraformer（zh-en 双语版）
- **次选**：sherpa-onnx + Streaming Zipformer（14M-XL 多档）
- **桌面端可加大**：sherpa-onnx + SenseVoice-Small（pseudo-streaming）
- **不推荐主路径**：Whisper 系——非流式、tiny 中文 CER 差、small 以上手机吃不消

### 2. 模型逐一分析

#### Whisper 系（OpenAI）

| 档位 | 参数 | FP16 | Q5_K 量化 | 中文 |
|---|---|---|---|---|
| tiny | 39M | 75 MB | ~30 MB | 弱 |
| base | 74M | 142 MB | ~57 MB | 一般 |
| small | 244M | 466 MB | 175 MB | 可用，非 SOTA |
| medium | 769M | 1.5 GB | ~480 MB | 较好但手机难跑 |
| large-v3-turbo | 809M | 1.6 GB | 574 MB | 较好 |

**硬伤**：原生 30 秒窗、非流式。Distil-Whisper **仅英文**。faster-whisper 主要面向 GPU 服务器，移动端价值有限。License MIT 最干净。仅作离线长音频补充。

#### Paraformer（阿里 DAMO）⭐ 输入法首选
非自回归 CIF + 注意力解码，工业界中文 ASR 事实标准。sherpa-onnx 提供 `streaming-paraformer-bilingual-zh-en` 与离线版（含四川话方言切换）。流式原生支持 chunk 解码，首字延迟典型 < 500ms。AISHELL-1 CER ~3-5%（FunASR 公开）。

**License 是 FunASR Model License，非纯 Apache，商用需法务确认**。流式版不支持时间戳；contextual biasing 当前不支持 Paraformer（输入法热词需走 Zipformer transducer）。

#### SenseVoice（阿里 FunAudioLLM）
SenseVoice-Small 234M 参数，5 语种（中英日韩粤）+ 情感/事件标签。官方"70ms 处理 10 秒"是 H800 GPU 数据，非手机。**原生非流式**，需借助 `streaming-sensevoice` 项目做 pseudo-streaming（带热词）。手机能跑但内存占用高于 Paraformer streaming。License MIT。社区有 RKNN/sensevoice.cpp 端 Q3-Q8 量化二进制。

#### Moonshine（Useful Sensors）
专为端侧设计。tiny 27M / base 61M / streaming-small 142M / streaming-medium 245M。500ms 延迟下界，v2 引入"ergodic streaming encoder"（滑动窗注意力）做低延迟流式。

**官方有 `moonshine-tiny-zh`**（2025-09 论文配套发布，6 语种含中文），但中文 CER 在 AISHELL/WenetSpeech 上的**第三方独立验证未公开**，仅论文宣称"显著优于 Whisper-Tiny，与 Whisper-Small/Medium 相当"。License MIT。GGUF 量化版社区已发布。

**风险**：流式版当前在 sherpa-onnx 中支持有限，中文版工业部署案例稀少。建议作为"观察名单"而非首发。

#### NVIDIA Parakeet / FastConformer / Canary
- **Parakeet-TDT 0.6B / 1.1B**：CC-BY-4.0，CoreML/ONNX/GGUF 都有，M4 Pro 上批量 RTF ~110×。**v2 仅英文**；v3 多语种（25 种主要欧洲语言），**不含中文**。Int8 仍占 1.2 GB RAM
- **Canary-1B / 1B-v2**：1B 参数，FastConformer + Transformer，25 种欧洲语言 + ASR/AST，**不含中文**。GGUF F16 1.97 GB

**结论**：NVIDIA 整条线对中文输入法场景直接不可用。

#### WeNet U2++
出门问问开源，工业生产线常见。U2++ 在 AISHELL-1 流式 320ms 延迟下 CER 5.05%，非流式 4.63%（论文数据）。**fast-U2++ 进一步把延迟从 320ms 压到 80ms**。runtime 提供 LibTorch / ONNX / ncnn 多后端，原生支持 Android/iOS。模型规模典型 ~50M，Q8 量化后 50-80 MB。中文 CER 数据公开充分，是流式中文最透明的选项之一。License Apache 2.0。

劣势：相比 Paraformer / Zipformer，社区活跃度近一年下降。

#### Vosk（Kaldi-based）
中文 small 50 MB，运行内存 ~300 MB；big 模型需 16 GB。**中文准确率社区差评**（vosk-android-demo issue #56：用户反馈"低到让人泪目"），未公开 AISHELL/WenetSpeech CER。架构基于 Kaldi nnet3+TDNN，相对 E2E 模型已落后一代。License Apache 2.0。

优势：Android/iOS/RPi/嵌入式 SDK 极成熟，集成成本最低。**结论**：仅在"对中文精度要求低、对包体极敏感"场景考虑，输入法不推荐作主模型。

#### Silero
Silero 主要价值是 **VAD**（~1.8 MB ONNX，业界标杆），其 STT 部分中文支持薄弱、benchmark 不透明。**输入法不推荐用 Silero STT**，但 VAD 强烈推荐配合任何 ASR 使用。

#### Kyutai 流式 ASR（Delayed Streams Modeling）
`kyutai/stt-1b-en_fr`（1B，0.5 秒延迟，含语义 VAD）、`kyutai/stt-2.6b-en`（2.6B，2.5 秒延迟，仅英文）。**不支持中文**。架构基于 Moshi 的延迟流建模，工程上是优秀的流式范式参考，但中文场景不可直接用。

#### Zipformer（k2-fsa icefall）⭐ 综合最佳
sherpa-onnx 中文流式主力之一。提供：
- `streaming-zipformer-bilingual-zh-en`
- `streaming-zipformer-ctc-zh-xlarge-int8-2025-06-30`
- 专为 ncnn/手机的 `sherpa-ncnn-streaming-zipformer-zh-14M-2023-02-23`（**14M 参数**）

中文 CER 在 AISHELL test 与 WenetSpeech test_net/test_meeting 上官方页面有公开列表（14M 版稍弱、xlarge 版接近 SOTA）。**License Apache 2.0**。

**综合"中文精度 + 流式 + 手机部署 + 许可清洁"最佳候选**。

### 3. 推理框架对比

| 框架 | 平台 | 量化 | 流式 | 中文模型生态 |
|---|---|---|---|---|
| **sherpa-onnx** | Android/iOS/HarmonyOS/Linux/macOS/Win/RPi/RK NPU/**Qualcomm QNN** | INT8 主流 | 原生 | Paraformer/Zipformer/SenseVoice/Whisper/Moonshine 全覆盖 |
| sherpa-ncnn | Android/iOS/RPi/RISC-V | ncnn 量化 | 原生 | Zipformer 专用，14M 中文小模型 |
| whisper.cpp | 全平台 + WASM/CoreML | Q4-Q8 GGML/GGUF | 切片伪流式 | 仅 Whisper 系 |
| ONNX Runtime Mobile | Android/iOS | INT8 | 取决于模型 | 通用底座，包体 ~12 MB |
| CoreML | iOS/macOS | FP16/INT8 | WhisperKit 等封装 | WhisperKit M4 Pro 0.46s 延迟、2.2% WER（英文） |
| TFLite | Android 主流 | INT8/动态 | 多为离线 | Whisper.tflite ~40MB，中文 ASR 例子很少 |

要点：
- **sherpa-onnx 是当前中文端侧 ASR 综合最强的胶水层**，模型选择最多，Android/iOS/HarmonyOS 都有官方 demo，并支持 Qualcomm QNN（NPU）加速
- whisper.cpp 适合"已选定 Whisper"的场景，跨平台一致性好，但与流式架构不匹配是硬伤
- CoreML/WhisperKit 在 iOS 上是 Whisper 系最佳运行环境
- TFLite 中文 ASR 端到端例子很少，生态深度逊于 sherpa-onnx

### 4. 输入法场景选型建议

#### 推荐组合（优先级递减）

1. **Android + iOS 主路径**：sherpa-onnx + streaming Paraformer 双语 zh-en（INT8） + Silero VAD + sherpa-onnx 自带热词。包体增量：sherpa-onnx so + Paraformer ONNX 估计 200-400 MB
2. **包体敏感子路径**：sherpa-ncnn + 14M streaming Zipformer zh。模型 ~14M 参数，量化后几十 MB
3. **桌面端**：可升级到 Zipformer-XL int8 或 SenseVoice-Small，CER 接近云端水平
4. **iOS 极致工程**：sherpa-onnx 已支持 iOS Swift；若团队倾向 Apple 原生栈，可对 Paraformer 做 CoreML 转换

#### 不推荐方案

- **whisper-tiny 直接做中文输入法**：30 秒窗 + 中文弱 + 无原生流式
- **Vosk 中文 small**：精度社区差评、架构落后
- **Parakeet / Canary / Kyutai STT**：不支持中文
- **Distil-Whisper**：仅英文
- **Moonshine-tiny-zh**：方向正确但缺乏第三方独立中文 benchmark
- **大模型 LLM-ASR（Fun-ASR 7.7B、Qwen2-Audio）**：手机端不现实

#### 未解决难点

- **端侧热词/个性化**：sherpa-onnx contextual biasing 仅 transducer 模型支持（Zipformer transducer 可，**Paraformer 当前不可**）。输入法常见的"通讯录人名/App 名/网络流行词"动态热词若用 Paraformer 需自研后处理或切到 Zipformer transducer
- **增量更新**：用户级声学/语言模型端侧微调没有成熟 OSS 方案，常见做法是个人词典做成热词列表 + 文本端 rerank
- **中英混说**：Paraformer-bilingual / Zipformer-bilingual 直接覆盖；SenseVoice 也 OK
- **方言**：Paraformer 2025-10-07 离线版含四川话；粤语首选 SenseVoice 或 Paraformer trilingual zh-cantonese-en
- **首字延迟实测**：sherpa-onnx 各模型 chunk 默认 ~0.32s，实际首字 300-600ms 强依赖芯片，中端骁龙 7 Gen 1 / 天玑 8000 档需自行实测，**官方未公布统一表**

### 5. 信息缺口

- Moonshine-tiny-zh 在 AISHELL-1 / WenetSpeech 上的独立 CER 未公开
- Paraformer streaming int8 在中端安卓 SoC 的首字延迟与 RTF 官方未公布
- SenseVoice-Small 在普通手机（无 NPU）上的实际推理时延无第三方数据
- sherpa-onnx + Paraformer streaming 在 iOS A14 及更早 SoC 的内存占用未公开
- Vosk 中文模型的 AISHELL/WenetSpeech CER 数字官方未给
- whisper.cpp 中文 small Q5_K 在中端安卓的逐 token RTF 未见统一公开 benchmark
- Parakeet/Canary 0.6B 在普通安卓手机原生跑通的公开案例几乎没有
- Kyutai STT 端侧手机部署案例公开数据缺失

### 6. 关键提醒

- **"端侧" ≠ "手机端"**。论文里的 "on-device" 经常指 Mac mini / Jetson / RPi 5。Whisper-large、Parakeet 1.1B、Kyutai 2.6B、SenseVoice-Large 在普通 4-6 GB RAM 安卓机上**会 OOM 或不可用**
- **INT8/Q5 量化是手机部署必要条件**，FP16 在主流安卓上不要直接用
- **License 风险**：阿里 FunASR/Paraformer 系是"Model License Agreement"而非纯 Apache，输入法商用前必须法务复核。**完全干净的可选项**：Zipformer / WeNet / Moonshine / Vosk / Whisper（MIT/Apache）
- 中文输入法这种**长尾词汇 + 实时性**双重要求场景，**没有任何单一开源模型可以"开箱可用"**，需要：流式 ASR + VAD + 热词/词典 + 标点恢复 + 文本后处理 + 个人化 rerank 的完整管线

---

## 主要参考来源

### 论文与技术报告
- 百度 SMLTA 论文
- 字节 Seed-ASR 技术报告（arXiv 2407.04675）
- 阿里 Paraformer 论文（arXiv 2206.08317）
- 阿里 Fun-ASR 技术报告（arXiv 2509.12508）
- 阿里 SenseVoice / FunAudioLLM 论文（arXiv 2407.04051）
- WeNet 论文（Interspeech 2021，arXiv 2102.01547）
- WeNet U2++ 论文（arXiv 2106.05642）
- fast-U2++ 论文（arXiv 2211.00941）
- Moonshine v2 论文（arXiv 2509.02523）
- WhisperKit 论文（arXiv 2507.10860）
- Whisper 量化论文（arXiv 2503.09905）

### 官方资料
- Google AI Blog（RNN-T On-device 2019、USM 2023）
- Apple WWDC 2023/2025 SpeechAnalyzer 资料
- sherpa-onnx 官方文档 https://k2-fsa.github.io/sherpa/onnx/
- FunAudioLLM/SenseVoice https://github.com/FunAudioLLM/SenseVoice
- modelscope/FunASR https://github.com/modelscope/FunASR
- Vosk Models https://alphacephei.com/vosk/models
- Kyutai STT https://kyutai.org/stt
- whisper.cpp https://github.com/ggerganov/whisper.cpp

### 排行榜与评测
- Open ASR Leaderboard（HuggingFace）
- Picovoice real-time transcription benchmark
