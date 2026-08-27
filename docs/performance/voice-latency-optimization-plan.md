# Doubao Voice Bridge 语音延迟优化与验证方案

> 日期：2026-08-27
> 范围：优化方案、实现记录与验证边界。
> 关联研究：`docs/performance/voice-latency-performance-report.md`

## 实施状态（2026-08-27）

本轮已完成文档中不依赖专有 ASR 接口、且可安全独立落地的 P0/P1 主链路：

- Mac 为每轮会话输出单调 `trace` 事件与音频证据摘要；
- 桌面客户端在发送 `start` 前开启麦克风并保留有界 pre-roll，删除 200 ms 未采音空窗；
- Mac 使用连续两次 50 ms 的 UI、焦点、音频数据面稳定采样，并经历 300 ms `asr_warmup` 后才发布 `recording`；
- CoreAudio 与 TCP audio listener 改为 App 生命周期预热，会话结束只清连接、缓冲与统计；
- 空文本根据 20 ms 峰值窗口 RMS、峰值电平和 PCM 字节数区分 `empty_result` 与 `no_speech`，不再发布空 final；
- 停止后的 marked-text commit 经 120 ms 稳定窗触发 final，连续修订会重置稳定窗；1.8 秒仅保留为 fallback；partial 兜底周期缩短到 50 ms；
- Linux/Windows 客户端识别新增准备阶段，并记录 Mac trace；Python 诊断客户端取消默认 1 秒额外启动等待；
- 当贝客户端先建立音频数据面再发送 `start`，绑定并校验 bridge `session_id`，只在 `recording` 后显示 LISTENING，且 final 与 completion 分离。
- `stop` 由 start ACK 建立的连接所有权和 `session_id` 双重校验，旧客户端的迟到 cleanup 不能停止新会话。

仍保留为实验项：豆包私有日志/AX 的更强 ready 信号、VAD 静音裁剪、积压 ETA、保音调 time compression、当贝 recorder warm-up 窗口内排队一次 begin，以及固定 WAV 冷/热各 30 次统计。这些项目需要真实音轨、投影仪噪声条件和设备批量试验，不能用单元测试替代。

### 本轮验证记录

| 目标 | 结果 |
|---|---|
| macOS Swift | 50 个测试通过；release App 构建、签名验证通过 |
| Python 诊断客户端 | 18 个测试通过 |
| Linux 桌面客户端 | 隔离副本中 55 个 Rust 测试通过 |
| Windows 桌面客户端 | 隔离副本中 35 个 Rust 测试通过 |
| 当贝 Android | `testProductDebugUnitTest` 与 `assembleProductDebug` 通过；APK 已安装到 `192.168.10.100:5555` |
| 投影仪运行状态 | 默认 IME、当贝与 ProjectorScroller 无障碍服务均保留；Doubao bridge 重连后 `ready=true` |
| Mac 运行状态 | 新 App 已启动，签名有效，TCP `4387` 与 audio `5004` 正常监听，CoreAudio/BlackHole 已预热 |

Mac 当时处于锁屏，前台应用为 `loginwindow`，因此无法完成依赖豆包 UI 的真实发声识别验收；程序在约 4.45 秒后明确进入 `voice_activation_failed`，恢复默认麦克风并回到 idle。该失败边界已验证，但 `1234567` 首段完整率和 p50/p95 仍必须在解锁后的真实设备批量试验中测量。

## 1. 目标与原则

这轮优化首先要解决“用户已经说了，但首段没有进入最终文本”，其次才是让状态岛更早变蓝。建议用四个产品指标约束所有改动：

1. **首段完整率**：固定音频 `1234567` 的最终结果必须从 `1` 开始，不能只看是否出现任意 partial；
2. **准备延迟**：触发到“可以安全说话”的真实数据面 ready，而不是到某个窗口出现；
3. **首字延迟**：首个非静音 PCM 被 ASR 接受后到首个 partial；
4. **收尾延迟与正确性**：停止到 final，空 final 必须是明确的 empty/error 状态，不能伪装成绿色成功。

核心原则是把当前一个 `recording` 拆成三个不同事实：

```text
capture_ready       客户端麦克风已开始产生 PCM
transport_ready     音频 TCP、CoreAudio、BlackHole 已可连续传输
asr_audio_ready     豆包已经真正开始消费输入音频
```

只有第三个成立时，用户界面才应表达“现在说话不会丢”。

## 2. 优先级结论

| 优先级 | 工作项 | 主要收益 | 风险/代价 |
|---|---|---|---|
| P0 | 建立跨端 session trace 与固定 WAV 回放基线 | 把猜测变成每阶段毫秒数据；能证明是否漏首段 | 少量日志与测试基础设施 |
| P0 | 麦克风先采、控制后启；去掉未采音的 200 ms 空窗 | 保住触发后的第一帧语音 | 需要管理常驻/预热采集生命周期 |
| P0 | 将 `recording` 改成真实音频 ready gate，并加入可验证的 ASR warm-up | 直接针对 `12345→34567` | 豆包没有公开 ready API，需要实验确定代理信号 |
| P0 | 空 final 进入 `empty_result/error`，不进入绿色成功 | 修复 06:15–06:28 的错误成功语义 | UI/协议需增加一类终态 |
| P1 | 事件驱动的 50 ms 稳定检测替代固定 1.0 s 等待 | 常见路径预计可省 0.4–0.7 s | 不能单独上线，否则可能加剧漏字 |
| P1 | 预热 Mac 音频 listener/CoreAudio 与客户端控制连接 | 减少每轮冷启动和抖动 | 常驻资源、设备切换与睡眠恢复要处理 |
| P1 | VAD 裁掉 pre-roll 静音并观测积压，不对语音盲目加速 | 降低首字与松键后排空债务 | VAD 阈值需要设备噪声标定 |
| P1 | final 由 marked-text commit 事件快路径触发，1.8 s 仅作 fallback | 正常收尾预计省约 1–1.7 s | 必须防止豆包迟到修订 |
| P1 | 当贝 final 与 completion 分离，并消费 bridge `session_id` | 避免提前 READY 与跨会话晚事件 | Android 状态机与测试需同步修改 |
| P2 | partial 观察从 100 ms 降到 30–50 ms、增加 trailing emit | 文字更新更顺滑，最多再省几十毫秒 | 主线程和网络事件频率上升 |
| P2 | 评估小比例、保音调的 backlog time compression | 长准备后的实时性追赶 | DSP 复杂，必须证明不伤识别率 |

## 3. P0：先建立可复现的性能反馈环

### 3.1 统一单调时间戳

每个事件输出 `session_id`、`elapsed_ms`、事件名和计数，不记录原文或 PCM：

- 客户端：hotkey/F5、worker 开始、capture ready、首个 callback、首个非静音、control/audio connected、start/stop write、pre-roll depth、首/末 audio write、partial/final receive、目标输入完成；
- Mac：start receive、设备切换前后、listener ready、AudioUnit started、capture focused、Fn down/up、voice window first seen/stable、recording broadcast、首包、首个非零 render、首个 marked text、commit、final broadcast；
- 当贝：editor eligible、recorder arm/ready、route locked、engine begin/finish、matching F5 UP、completion、IME composing/commit。

建议同一 session 结束时输出一行聚合摘要，直接包含 `prepare_ms`、`first_pcm_ms`、`first_partial_ms`、`final_ms`、`preroll_ms`、`dropped_bytes`、`empty_final`。

### 3.2 固定输入测试

视频没有音轨，下一轮必须使用可对齐的固定输入：

1. WAV 开头 100 ms 内直接说 `一二三四五六七`，前面不留静音；
2. 另做一份开头带 500 ms 静音的对照；
3. 连续运行冷启动、热启动、激活重试各 30 次；
4. 将 PCM 中的同步脉冲、状态岛录屏和单调日志对齐；
5. 判定必须检查最终文本前缀，不以“出现过 partial”代替完整性。

首轮验收门槛建议：100% 试次保住 `1`，空结果率为 0；达成后再以 p50/p95 优化速度。

## 4. P0：消除采音空窗与错误 ready

### 4.1 客户端先采音

当前桌面客户端发送 `start` 后固定 sleep 200 ms，再连音频端口并打开 CPAL。建议改为：

1. 快捷键触发后立即启动或复用已预热的麦克风 stream；
2. 首个 callback 到达后标记 `capture_ready`；
3. PCM 立即进入有界 pre-roll；
4. 控制连接和 Mac 激活并行执行；
5. `transport_ready/asr_audio_ready` 前不丢弃已采语音。

当贝已经在 editor eligible 时提前保持 BLE HID `AudioRecord`，可复用其“长期 channel + session-scoped sink”的结构；桌面客户端也应把设备生命周期与每轮识别会话分开。

### 4.2 两阶段 ready gate

不要直接把 `likelyVoiceUIActive` 当成 `recording`。建议协议阶段变为：

```text
arming -> ui_ready -> asr_warmup -> recording -> optimizing -> final/empty_result
```

- `ui_ready`：豆包窗口与 capture focus 连续两次稳定，例如相隔 50 ms；
- `asr_warmup`：确认 BlackHole/CoreAudio 已持续运行，发送短静音/校准序列，并等待可观测的豆包“录音真正开始”信号；
- `recording`：此刻才从 pre-roll 的第一个语音样本开始播放，并通知底部状态岛切为蓝色波形。

豆包没有公开的音频 ready API，因此代理信号按可靠性排序：

1. 豆包自身日志中的 ASR/audio capture started 事件（若版本间稳定）；
2. AX 窗口中比“窗口存在”更具体且稳定的录音状态；
3. 窗口稳定 + CoreAudio 首个 render + 经实验得到的短 warm-up guard。

第三项只能作为有界 fallback。建议从 300 ms guard 做 A/B，逐步缩短，而不是直接把现有 1.0 s 改成 250 ms。

### 4.3 空结果是失败，不是成功

当 session 有明显非静音 PCM、进入过 recording、但从未产生 partial/final 文本时，Mac 应发：

```json
{"type":"error","phase":"empty_result","session_id":123,"message":"No speech result was produced"}
```

顶部和底部状态岛显示可重试提示；客户端保留目标输入内容，不执行空提交。若全程只有静音，可使用更温和的 `no_speech` 状态。两者需要用本地 RMS/VAD 证据区分。

## 5. P1：缩短准备阶段

### 5.1 用早期稳定检测取代固定 1 秒

现有日志在 Fn 后 250 ms 已经发现豆包窗口，但决策固定等到 1.0 s。完成两阶段 gate 后，可每 50 ms 检查一次 UI 条件，连续两次成立即进入 warm-up。示例预算：

```text
旧：startup 300 ms + 固定 check 1000 ms = 至少 1300 ms
新：并行预热 + UI stable 250–350 ms + warm-up 250–350 ms
目标：典型真实 ready 约 500–700 ms，且不牺牲首段完整率
```

这里的 500–700 ms 是实验目标，不是当前已实现性能。

### 5.2 预热和并行化

按副作用从低到高实施：

1. 启动时缓存 BlackHole device ID、豆包 input source 与 AX 元素；失效时再枚举；
2. App 生命周期内保持控制 listener，音频 listener 也改成长生命周期并以 session ID 划界；
3. 提前创建 CoreAudio unit，空闲时输出静音，session 只清环形缓冲并打开 gate；
4. 客户端保持控制 TCP；音频 TCP 可进一步复用；
5. 默认输入设备切换与 capture focus、输入法切换并行执行，但要保留失败回滚。

默认输入切换会影响用户全局系统状态，不建议为了几十毫秒永久固定 BlackHole；应通过预热其他组件取得大部分收益，并保持恢复记录机制。

## 6. P1：降低 pre-roll 债务

当前 5 秒 pre-roll 在 recording 后以 1× 播放。它能保存已采音频，却会把准备耗时整体平移到首字和停止排空。

优先采用安全策略：

1. 用轻量 VAD 只裁掉 pre-roll 前后的确定静音；
2. 记录 speech-first offset、queued speech duration 和 drain ETA，并在状态岛区分“已就绪、正在补送”；
3. 准备超过 5 秒时显式报告 `preroll_truncated_ms`，不可静默裁掉；
4. 用户松键后继续排空语音，但给 UI 展示准确的剩余时间/阶段。

不要直接把 PCM 以 2× burst 写给 Mac：Mac 只有 250 ms ring buffer，满时会丢最旧帧，且未经变速处理的 48 kHz PCM 会改变时间轴。若 VAD 后仍需追赶，只评估 1.05–1.15× 的保音调 time compression，并以识别完整率作为硬门槛。

## 7. P1/P2：文字反馈与 final

### 7.1 partial 快路径

`setMarkedText` 已提供事件回调，100 ms timer 应作为漏事件兜底。可改为：首个变化立即 emit；100 ms 内的高频变化合并；窗口末尾保证 trailing emit。把 timer 降到 30–50 ms 只能优化几十毫秒，优先级低于 ASR ready 和 pre-roll。

### 7.2 final 事件驱动

Fn up 后，若观察到对应 `textDidChange` commit，应进入短稳定窗；窗口内后续修订重置计时，稳定后发送 final。当前实现采用 120 ms，1.8 秒保留为 fallback，而不是正常路径固定等待。客户端的 500 ms 静音在对端已关闭音频连接时会提前终止。

验收必须覆盖：迟到 partial 不覆盖 final、空文本不提交、焦点切换不串 session、豆包长句修订不被过早截断。

## 8. 当贝项目专项修复顺序

1. `DoubaoRecognitionEngine` 收到 final 时只调用 `onFinal`；直到 matching F5 UP、bridge idle/完成条件满足后再 `onSessionCompleted`。
2. `DoubaoBridgeClient` 解析并校验 bridge `session_id`，拒绝上一轮的迟到 partial/final/idle。
3. `VoiceInputStatus` 使用 engine phase：arming 显示“准备中”，只有 bridge recording 后才显示 LISTENING。
4. editor 刚 eligible 或 OEM 返回的 recorder warm-up 窗口内，F5 不应直接永久 BLOCKED；可短暂排队一次 begin，并在 recorder ready 后继续，超时再明确失败。
5. 豆包 adapter 上报 pre-roll depth/drain ETA；completion 必须晚于 backlog 排空、stop/final 和 matching key lifecycle。

这些改动既能修正 06:15–06:28 类状态异常，也能让上下两个状态岛表达同一个真实阶段。

## 9. 分阶段实验与验收

| 阶段 | 改动 | 实验 | 通过条件 |
|---|---|---|---|
| A | 只加 trace，不改行为 | 当前版本冷/热各 30 次 | 每轮可计算 capture、transport、ASR、partial、final 时间；无缺口事件 |
| B | 先采音 + 两阶段 ready/warm-up | `1234567` 无前置静音 WAV，3 个 warm-up 值 A/B | 100% 保住 `1`；空结果 0；选择最低 p95 的安全值 |
| C | 50 ms stable check + 预热 | 冷/热、睡眠恢复、网络重连各 30 次 | 首段完整率不回退；Tready p50 目标 ≤700 ms、p95 ≤1 s |
| D | VAD 静音裁剪 | 安静房间、投影风扇噪声、远近讲话 | 不裁语音；首字/排空 p95 显著下降；超 5 秒有显式指标 |
| E | event-driven final | 短数字、长句、停顿、撤销/修订 | final 正确率不回退；正常收尾 p50 目标 ≤500 ms |
| F | 当贝 session 状态修复 | 快速连按、F5 长按、上一轮迟到事件、空结果 | 无提前 READY、无跨会话文字、上下状态岛阶段一致 |

## 10. 推荐落地顺序

建议按以下顺序提交，每一步都保持可单独回滚：

1. trace + 固定 WAV/视频对齐工具；
2. empty/no-speech 终态与 session ID 完整校验；
3. 客户端预热采音，删除 200 ms 未采窗口；
4. `ui_ready/asr_warmup/recording` 分层与首段完整率 A/B；
5. 50 ms 稳定检测和 Mac 音频预热；
6. VAD 静音裁剪与积压指标；
7. event-driven final；
8. partial 刷新与可选 time compression。

不建议第一步就调小所有 sleep。当前最严重的问题是状态“看起来 ready”却丢掉开头；先建立数据面 ready 和零漏字基线，才能安全地把准备阶段压短。
