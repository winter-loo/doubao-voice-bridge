# Doubao Voice Bridge 端到端语音延迟性能研究报告

> 状态：完成；已合并视频逐帧观测、两个项目源码与本机运行日志。
> 研究日期：2026-08-27
> 本地项目版本：`893cbd2c39e101422e88d0407bf44a07be230ed6`
> 当贝项目版本：`b722152235b8cdc7a8e56b7d9b5902171f26880b`（`feat/compact-voice-panel-morph`，研究时工作树干净）

## 1. 研究目标与边界

本报告回答三个问题：

1. 从用户触发语音输入，到界面进入可说话状态，时间花在哪里；
2. 从用户发声，到首个可见文字、持续更新和最终文字返回，分别经过哪些环节；
3. `doubao-voice-bridge` 与当贝语音输入法的架构差异，哪些差异可能影响响应速度和异常恢复。

研究依据仅包括两个项目的源代码、测试、仓库文档和现有本机运行日志。没有修改远端项目或远端运行状态。凡是不能从源代码或日志直接证明的内容，均标为“假设”，不能当作已确认根因。

本报告把性能拆成四个用户可感知指标：

- **T0 → Tready（准备延迟）**：按下快捷键到 UI 明确进入 `recording/listening`；
- **Tspeech → Tpartial（首字延迟）**：开始发声到首个非空 partial 文本；
- **Tpartial(n) → Tpartial(n+1)（流式反馈间隔）**：连续文字更新的节奏；
- **Tstop → Tfinal（收尾延迟）**：停止录音到最终文字稳定并回到 idle。

## 2. Doubao Voice Bridge 当前端到端路径

### 2.1 总体数据流

仓库定义的生产链路为：远端客户端采集 48 kHz PCM → TCP 送到 Mac → CoreAudio 写入 BlackHole → 豆包输入法监听 BlackHole → 豆包把 marked text 写入隐藏 `NSTextView` → Mac 经控制 TCP 广播 `partial/text/final` → 客户端更新 UI/目标输入框（`README.md:8-19`）。

控制与音频采用两条独立 TCP 连接：控制端口默认 `4387`，音频端口默认 `5004`；Mac 默认把 BlackHole 同时作为临时默认输入和音频输出目标（`Sources/DoubaoBridgeMac/main.swift:8-27`）。

### 2.2 T0 → Tready：快捷键、会话冷启动和就绪门控

#### 客户端侧

- Windows 全局快捷键是 `Ctrl+Alt+Space`。收到 `WM_HOTKEY` 后，如果悬浮窗隐藏就立即调用 `begin_input`（`clients/desktop-client/src/main.rs:1671-1701`）。
- `begin_input` 先同步显示 `Activating`，然后才创建后台语音会话线程，因此按键到视觉反馈本身没有人为 sleep（`clients/desktop-client/src/main.rs:1302-1311`）。
- Linux 也先把 phase 置为 `Activating`，显示/请求 transcript window，再启动语音会话（`clients/desktop-client/src/main.rs:2353-2380`）。
- 后台线程连接 Mac 控制端口，设置 `TCP_NODELAY`，发送 `start`；控制连接总超时为 10 s，每个地址单次 connect timeout 为 2 s，失败后每 50 ms 重试（`clients/desktop-client/src/native_voice.rs:381-418,859-879`）。
- 发出 `start` 后有固定 **200 ms** `AUDIO_START_DELAY`，随后才连接音频端口并打开本地麦克风（`clients/desktop-client/src/native_voice.rs:21-27,403-425`）。因此用户在快捷键后的最初约 200 ms 内立即说话，音频尚未开始采集；5 秒 pre-roll 无法补回这段尚未采集的数据。

#### Mac 侧

Mac 收到 `start` 后把命令投递到主队列，再执行 `Bridge.startSession()`（`Sources/DoubaoBridgeMac/main.swift:2271-2324`）。随后严格串行执行：

1. 广播 `phase=arming`；
2. 查询、保存并切换 macOS 默认输入到 BlackHole，而且写后立即回读验证（`Sources/DoubaoBridgeMac/main.swift:1681-1701,1881-1920`；设备切换验证见 `1062-1087`）；
3. 每轮会话新建并启动 CoreAudio HAL output，再新建音频 TCP listener（`Sources/DoubaoBridgeMac/main.swift:1315-1335`）；
4. 清空捕获窗口并抢占应用/窗口/文本框焦点；若未成功，100 ms 后再抢一次（`Sources/DoubaoBridgeMac/main.swift:323-355`）；
5. 同步切换输入源到豆包（`Sources/DoubaoBridgeMac/main.swift:467-479`）；
6. 固定等待 **300 ms** `startupDelay`，按下/保持 Fn（`Sources/DoubaoBridgeMac/main.swift:23-27,1693-1712,1955-1975`）；
7. 固定再等待 **1.0 s** `voiceActivationCheckDelay`，同时检查豆包语音 UI 与 capture focus；两者都成立才广播 `phase=recording`（`Sources/DoubaoBridgeMac/main.swift:1973-2000`；双条件由 `216-223` 定义，并由 `tests/DoubaoBridgeMacTests/VoiceInputReadinessTests.swift:5-44` 覆盖）。

因此仅显式固定等待就给 Tready 加入 **1.3 s**。设备枚举/切换、AudioUnit 创建、网络往返、主队列排队、TIS 切换和 AX/窗口扫描还会叠加在其上。

代码在 Fn 后 **250 ms** 和 1.0 s 各采一次 `voice_state`，但 250 ms 结果只广播/打日志，不会提前完成就绪门控；真正决策仍固定发生在 1.0 s（`Sources/DoubaoBridgeMac/main.swift:2181-2186` 对比 `1973-2000`）。现有日志也多次显示 `after voice shortcut +0.25s` 已发现豆包窗口，而程序仍等到 `voice activation check attempt 1` 才进入 recording。这是一个有直接代码与运行日志支持的等待空档。

若就绪失败，会释放 Fn，固定等 **250 ms** 后重新按下，并再次等待完整 1.0 s；默认最多重试 2 次，即总计 3 次尝试（`Sources/DoubaoBridgeMac/main.swift:24-26,2003-2042`）。不含各操作成本，最坏显式等待约为 `0.3 + 1.0 + 2 × (0.25 + 1.0) = 3.8 s`。

### 2.3 音频采集、pre-roll、传输与播放

客户端用 CPAL 打开默认或指定麦克风，读取设备默认格式，在 callback 中下混为单声道、线性重采样到 48 kHz、编码成 s16le（`clients/desktop-client/src/native_voice.rs:730-845`；重采样实现见 `clients/desktop-client/src/client_core.rs:58-106`）。

在 Mac 尚未发出 `phase=recording` 时，客户端只保留最近 **5 秒** PCM（`48,000 × 2 × 5 = 480,000 bytes`）；收到 recording 后按音频真实时长逐块回放 pre-roll，防止瞬间突发灌入 ASR。录音阶段总积压上限为 **10 秒**，超过即显式报错（`clients/desktop-client/src/native_voice.rs:21-27,213-297`）。相应测试证明：

- arming 期间的尾部音频会在 recording 后转发；
- pre-roll 按原始时长 paced，不会一次 burst；
- 超过 byte budget 会报错；
- 96,000 bytes 对应 1 秒 48 kHz mono s16le（`clients/desktop-client/src/native_voice.rs:657-727`）。

Mac 音频 TCP 单次 receive 上限 64 KiB，到包后先统计电平，再写进 CoreAudio 环形缓冲（`Sources/DoubaoBridgeMac/main.swift:1522-1544`）。环形缓冲为 **12,000 帧 = 250 ms @ 48 kHz**；满时丢最旧帧而不是继续增长排队延迟，欠载则补零（`Sources/DoubaoBridgeMac/RealtimeAudioOutput.swift:6-95,114-117`；`tests/DoubaoBridgeMacTests/RealtimePCMBufferTests.swift:5-29`）。

这意味着链路有两个不同目的的缓冲：客户端 5 秒 pre-roll 用于保住准备阶段说的话；Mac 250 ms 环形缓冲用于吸收网络抖动。前者会让过早说出的内容在 ASR ready 后“按原速补播”，因此可能延后实时语音追上现场的时刻；后者的上限受控，不会无限累积。

### 2.4 Tspeech → Tpartial：豆包 ASR 与文字捕获

豆包首先把识别结果写成 marked text。捕获窗口有两条 partial 触发路径：`setMarkedText` 后异步回调，以及每 100 ms 的 timer 轮询（`Sources/DoubaoBridgeMac/main.swift:205-213,303-313`）。`emitPartialIfChanged` 只在存在 marked text 且文本变化时发出，并再设 **100 ms** 节流，因此桥自身 partial 发送上限为 10 Hz（`Sources/DoubaoBridgeMac/main.swift:426-445`）。

Mac 广播 JSON-line 事件时只在串行网络队列上编码并对已授权客户端调用 send，没有额外 batch timer（`Sources/DoubaoBridgeMac/main.swift:2234-2249`）。客户端 reader 逐行解码；partial 到达后，Windows 立即做“公共前缀 + 删除旧后缀 + SendInput 新后缀”的增量替换，Linux 更新内存中的 transcript 状态（`clients/desktop-client/src/native_voice.rs:513-589`；Windows 写入见 `clients/desktop-client/src/platform_paste.rs:58-168`；Linux UI 状态见 `clients/desktop-client/src/main.rs:77-135,412-432`）。

由代码可确定的桥接层新增等待上界约为 **0–100 ms**（捕获 timer/节流），但豆包自身从声音进入 BlackHole 到生成首个 marked text 的时间不在本仓库控制范围内，必须由视频或新增时间戳埋点测量。

### 2.5 Tstop → Tfinal：停止、提交和最终反馈

客户端停止时先停止 CPAL stream，排空已进入队列的 PCM，并按实时节奏送完 pre-roll/backlog；之后发送 `stop`，再额外发送 **500 ms 静音**并关闭音频写端（`clients/desktop-client/src/native_voice.rs:464-480`）。

Mac 收到 stop 后立即广播 `phase=optimizing`，释放 Fn 让豆包提交 marked text，但固定等待 **1.8 s** `finalDelay` 才读取捕获窗口、停止音频、恢复默认输入、发送 `final` 和 `idle`（`Sources/DoubaoBridgeMac/main.swift:27,1728-1782`）。客户端最多等 final **8 s**，若 final 为空则以最近 committed text 兜底（`clients/desktop-client/src/native_voice.rs:21-24,482-505`）。Windows 在没有活动语音线程可停止的 UI 异常分支中还会用 2.4 s fallback 隐藏 optimizing overlay（`clients/desktop-client/src/main.rs:1105-1107,1636-1658`）。

所以正常路径的收尾存在两个串行的固定保护窗：客户端 0.5 s 静音与 Mac 1.8 s final delay。二者目的不同，但当前没有“豆包已提交/marked text 已消失后立即完成”的事件驱动快路径。

### 2.6 状态异常与恢复路径

状态机在客户端表现为 `Hidden → Activating → Listening → Optimizing → Hidden`；Mac 协议将 `arming/voice_retry` 映射到 Activating，将 `recording` 映射到 Listening，将 `optimizing` 映射到 Optimizing（`clients/desktop-client/src/main.rs:68-75,388-410`）。

Mac 只有在“豆包语音 UI active”和“隐藏 capture 文本框仍获得焦点”同时成立时才宣布 recording。录音中若焦点丢失，会回到 `arming`，每 200 ms 尝试恢复；默认重试次数用同一个 `voiceActivationRetries=2`。恢复成功后先释放 Fn，再等 250 ms，重新走一次完整激活；失败则停止音频、恢复输入设备、发 error + idle（`Sources/DoubaoBridgeMac/main.swift:2057-2153`）。

`VoiceStateDetector` 的 `likelyVoiceUIActive` 判据是“命中关键词”**或**豆包窗口存在且名称非空/层级大于零（`Sources/DoubaoBridgeMac/main.swift:583-607`）。因此它更像启发式可见性检测，不等价于 ASR 后端真正可收音。**假设：** 视频中的“显示 listening 但没有文字”可能是此判据假阳性，也可能是豆包云端/本地 ASR 未工作；单凭现有状态无法区分。

现有运行日志提供了反例：session 18 和 20 均通过第一次 activation check，之后每秒收到约 96–100 KB 音频且有明显非静音能量，但全程没有任何 `[capture] partial`，最终文本为空（本机 `~/Library/Logs/DoubaoVoiceBridge/bridge.log:24137-24169,24207-24239`，行号为研究时快照）。这证明该异常发生在“客户端采集 → TCP → Mac 音频计量 → 语音 UI 检测”之后；但仍不能只凭该日志区分 BlackHole→豆包、豆包识别、或 marked-text 注入哪一段失效。

## 3. 当贝语音输入法项目

远端项目位于 `ldd@192.168.10.104:/home/ldd/lab-dangbei/DangbeiVoiceInput`。以下引用均为该仓库相对路径；只写 Java 文件名的引用均位于 `app/src/main/java/cn/deeloo/dangbeibusprobe/`。审计全程只读，没有运行 Gradle、连接设备或改变进程状态。

### 3.1 双引擎架构

当贝项目不是单纯的“本地 ASR”对照组，而是提供两个并列 adapter：

- `ProjectorRecognitionEngine`：直接调用投影仪 Rocky/AISpeech DDS；
- `DoubaoRecognitionEngine`：通过本报告第 2 节的 Mac `doubao-voice-bridge` 完成 ASR。

共同 seam 接收 16 kHz PCM 与 session ID，并回调 phase/partial/final/completion/error；架构图和职责分层见 `docs/ARCHITECTURE.md:12-55`，选择策略见 `docs/adr/0003-use-mac-doubao-bridge-for-asr.md:5-25`。

### 3.2 服务、引擎和麦克风的预热

Accessibility 服务连接后即创建并启动长生命周期 `VoiceDictationController`（`VoiceKeyAccessibilityService.java:93-109`），controller 立即加载所选引擎（`VoiceDictationController.java:111-115`）。选择豆包时，控制 TCP 是长连接并在服务期常驻，而不是每次 F5 再建连（`DoubaoBridgeClient.java:108-113,184-243`）。连接超时 3 s，启用 keepalive 和 `TCP_NODELAY`，收到 `hello` 或鉴权成功后才 ready（`DoubaoBridgeClient.java:43,199-205,246-268`）；断线重连为 250 ms 起、5 s 封顶的指数退避（`RetryingWorker.java:5-6,31-49`）。

编辑器状态刷新有固定 **80 ms debounce**（`VoiceKeyAccessibilityService.java:20,246-250`）。editor eligible 且 engine ready 时，controller 在用户按 F5 之前异步启动并保持 BLE HID `AudioRecord`（`VoiceDictationController.java:127-147,578-698`）。因此正常热路径不承担麦克风创建成本。

遥控器采集格式为 16 kHz mono PCM16、`VOICE_RECOGNITION` source，并通过反射强制 vendor port 14。AudioRecord buffer 是 `max(2560, minBuffer + 1600)`；每次 blocking read **1,280 bytes = 40 ms** 音频（`DangbeiRemoteAudioChannel.java:44-46,82-123,164-185`）。sink owner 线程启动最多等待 2 s；停止最多 join 1.5 s（同文件 `125-141,215-235`）。

**冷路径风险：** 若用户刚进入文本框就立刻按 F5，80 ms debounce 和异步 AudioRecord 初始化可能尚未完成。此时 `tryBeginCustom()` 会因为 `recorderReady=false` 立即拒绝本次会话；它不会等 recorder 随后 ready 后自动续接（`VoiceKeyAccessibilityService.java:165-176`；`VoiceDictationController.java:246-290`）。这是准备阶段偶发“首按没进入识别”的高置信度代码风险。

### 3.3 F5 DOWN 与 Mac 豆包路径

首个 F5 DOWN 会同步选择并锁存 CUSTOM/OEM/BLOCKED route；正常 CUSTOM 热路径绑定 IME session、调用 `engine.beginSession()`，随后立即 `setForwarding(true)`，key callback 中没有网络等待或 sleep（`VoiceDictationController.java:246-329`）。

豆包 adapter 创建 session 和 5 秒 pre-roll，立即回调 `arming`，在 command executor 异步发送 `start`，同时启动音频线程（`DoubaoBridgeClient.java:55-72,116-131`）。控制命令每次 write 后立即 flush（同文件 `341-350,492-501`）。

音频线程仍固定先睡 **200 ms**，之后在 5 s 总窗口内连接 Mac 音频端口；单次 connect 最多 1 s，失败每 50 ms 重试（`DoubaoBridgeClient.java:43-52,353-421`）。连接成功后等待 Mac `phase=recording` 最多 12 s。理论故障窗口并非单一 12 s，而可能接近 `0.2 + 5 + 12 = 17.2 s`。

当贝原生 PCM 为 16 kHz，adapter 用跨 chunk 连续的线性插值升到 48 kHz（`Pcm16To48Resampler.java:8-44`）。激活前 pre-roll 为 480,000 bytes/5 s，recording 后 backlog 上限为 960,000 bytes/10 s；超出 5 s 的早期音频从头裁掉（`DoubaoBridgeClient.java:48-59`；`PrerollAudioBuffer.java:25-35,90-101`）。

### 3.4 pre-roll 的结构性延迟

收到 Mac `recording` 后，audio thread 才打开 playback gate，并对每个 chunk 按 PCM 时长以 **1× realtime** pacing 写入（`DoubaoBridgeClient.java:295-306,363-385`）。这保证不 burst、也不丢准备阶段的首字，但产生一个重要结果：

> 若 Mac 激活耗时为 A 秒，那么 recording 到来时已有约 A 秒音频积压。新音频以 1× 进入，旧音频也只以 1× 发出，所以用户持续说话期间积压不会缩短；首个 partial 会整体后移约 A，松键后也要再等待约 A 排空才有资格发送 stop。

这是当贝 + Mac 豆包组合路径最强的结构性延迟源。它不是 LAN 带宽不足，而是有意的实时 pacing 策略把激活延迟平移到了识别反馈和收尾。

### 3.5 partial 到 IME 的反馈路径

控制 socket 收到 `partial`/`text` 后立即回调 listener（`DoubaoBridgeClient.java:246-280,309-317`）；controller 只做文本规范化再转交目标（`VoiceDictationController.java:497-503`）；IME 经 main Handler 校验 session/editor generation 后调用 `InputConnection.setComposingText()`，重复 partial 会跳过（`VoiceInputMethodService.java:179-187,1156-1207`）。这段没有人为 debounce/batching，主要成本是 socket read、一次 Handler hop、格式化和 InputConnection IPC。

历史内置引擎实机记录显示：F5→PCM 43 ms、ASR begin 196 ms、首 partial 2.45 s、松键→final 约 120 ms；另一轮首 partial 2.082 s，而 ASR final 到 `commitText` 约 8 ms（`docs/research/dangbei-remote-voice-input-research.md:581-619,752-765`）。这些数据只说明 Android 文字提交层不是秒级主瓶颈，不能替代本次豆包链路视频的实测。

### 3.6 F5 UP、排空与 final

UP 先关闭新的 PCM forwarding，再调用 engine finish（`VoiceDictationController.java:332-362`）。adapter flush resampler、结束输入并立即显示 optimizing，但必须等 1× paced backlog 完全排空后才发 `stop`（`DoubaoBridgeClient.java:147-165,370-385`）。随后发送 25 个 1,920-byte chunk，每个间隔 20 ms，即固定 **500 ms 静音**，再关闭写端；final fallback 为 8 s，届时提交 latest stable text（同文件 `423-447`）。

因此松键后延迟近似为：`剩余 activation/pre-roll backlog + Mac final latency`；完全没有 final 的 fallback 上界还要再加 `500 ms + 8 s`。

### 3.7 内置投影仪 ASR 路径

内置 engine 的 subscriber 与 control client 同样是长生命周期并提前 ready（`ProjectorRecognitionEngine.java:79-94,162-170`）。每轮 begin 进入队列，control thread 设置 ASR model、清 phrase hints、切 near pickup，固定等待 **150 ms**，然后启动 recorder 与 `asrOnly`，再发 `recording`（同文件 `97-123,317-332`）。PCM 通过本机 `127.0.0.1:50001` DDS/`local_recorder.pcm`，没有跨机 TCP、BlackHole、Mac 焦点、输入法 Fn 激活和 pre-roll 回放链（同文件 `18-25,499-526`）。

停止时它最多轮询 3 s 等 final，每 50 ms 检查一次；然后停止 recorder、固定等 80 ms、恢复 far pickup并清理状态（`ProjectorRecognitionEngine.java:335-365`）。partial/final 带 session ID，并只接受当前 active session（同文件 `368-404`）。

### 3.8 状态异常的高置信度候选

#### 候选 A：豆包 adapter 过早把 final 等同于 completion

统一接口要求 final 只交付 committed text，session completion 与物理按键生命周期分离；架构时序也明确 `final` 后仍要等 F5 UP 和 engine completion（`docs/ARCHITECTURE.md:57-84`）。内置 engine 符合此约束：收到 final 只记录，直到 stop/restore 后才 completion（`ProjectorRecognitionEngine.java:335-365,368-404`）。

但 `DoubaoRecognitionEngine` 在收到 final 时先 `listener.onFinal(...)`，紧接着就在同一回调调用 `listener.onSessionCompleted(...)`（`DoubaoRecognitionEngine.java:31-35`）。controller 随即清除 active session、关闭 forwarding、发布 READY，即使物理 F5 可能尚未松开（`VoiceDictationController.java:517-575`）。这能直接导致“按键仍按住但状态提前恢复”、允许过早开始下一 session，是 `06:15-06:28` 异常片段的高优先级候选；最终仍需和视频/日志时间线对齐确认。

#### 候选 B：UI 的 LISTENING 不代表 Mac 已 recording

`VoiceInputStatus.resolve()` 只要 `activeSessionId != null && !stopQueued` 就返回 LISTENING（`VoiceInputStatus.java:19-32`）。session 在 begin 时立即变 active，而 Mac phase 回调在 controller 中只写日志，不参与 UI 状态（`VoiceDictationController.java:311-329,491-495`）。所以从 F5 DOWN 到 Mac recording 的整个激活期，UI 已可能显示“正在听”，放大了准备慢或未真正 ready 的体感。

#### 候选 C：Android client 没有消费 bridge 的 session ID

远端文档仍写“bridge 回包没有 session ID”（`docs/adr/0003-use-mac-doubao-bridge-for-asr.md:22-25`），但本地当前 Mac bridge 已在 status/final/text/partial 中加入 `session_id`（本地 `Sources/DoubaoBridgeMac/main.swift:1686-1691,1767-1777,2465-2484`）。Android `DoubaoBridgeClient.handleControlLine()` 没有解析该字段，而是把事件绑定到当下 `activeSession`（远端 `DoubaoBridgeClient.java:246-338`）。结合候选 A 的过早 completion，上一轮晚到的 idle/partial/final 理论上可能落入下一轮。

#### 候选 D：首个 editor 与 OEM 返回后的 recorder 未 ready

除了 80 ms editor debounce，OEM UP 还故意等 **800 ms** 才重新 arm recorder（`VoiceDictationController.java:237-244`）。如果用户从 OEM 助手快速返回文本框并立即按 F5，或刚聚焦 editor 就按键，route 会进入 BLOCKED，而不是等待几百毫秒后自动继续。

## 4. 两个项目的延迟架构对比

| 阶段 | Doubao Voice Bridge | 当贝语音输入法 | 性能含义 |
|---|---|---|---|
| 激活入口 | Windows `WM_HOTKEY` / Linux X11、Portal、GNOME shortcut | F5 Accessibility route；CUSTOM/OEM/BLOCKED 锁存 | 两者都可立即给视觉反馈；当贝需先满足 editor、engine、recorder 三项 ready |
| 麦克风打开 | 发 `start` 后固定等 200 ms，再连音频 TCP、打开 CPAL | editor eligible 时提前常驻 16 kHz BLE HID AudioRecord | 当贝热路径保住按键后的首个 200 ms；冷 editor 仍可能因 80 ms debounce/异步 arming 拒绝首按 |
| 控制连接 | 每轮 native client session 新建到 Mac 的 control TCP | 豆包 control TCP 服务期常驻；内置 DDS client 也常驻 | 当贝移除了稳定热路径的 control connect 成本 |
| ASR 就绪 | Mac 切设备、建 AudioUnit/listener、抢焦点、切输入法、等 300 ms、按 Fn、再固定等 1 s检查 | 内置：本机 DDS + 150 ms near-pickup wait；豆包：仍依赖完整 Mac 链 | 当贝内置明显短链；选择豆包时没有消除 Mac 的 1.3 s 固定门控 |
| 准备期音频 | 5 s 尾部 pre-roll，ready 后按实时速度回放 | 豆包同样 5 s/1× pacing；内置直接本机 publish PCM | 豆包组合路径把 activation offset 平移到首字和 final；内置没有这段 pre-roll 债务 |
| 网络/传输 | 两条 TCP；48 kHz mono s16le，`TCP_NODELAY` | 豆包：长控制 TCP + 每轮音频 TCP；内置：127.0.0.1 DDS | 豆包音频仍每会话冷连；内置无 LAN/BlackHole/跨机环节 |
| partial | 豆包 marked text；事件 + 100 ms timer；最多 10 Hz | Android 无额外 debounce；main Handler 后 `setComposingText` | 豆包 partial 上限仍由 Mac 10 Hz 捕获决定，Android 提交层不是秒级瓶颈 |
| final | 客户端 500 ms 静音；Mac固定 1.8 s；client 最多等 8 s | 豆包：先排空 1× backlog，再 500 ms 静音、8 s fallback；内置：最多 3 s 等 final + 80 ms restore | 当贝豆包路径可能比桌面客户端多出显著 backlog 排空；内置历史实测松键→final约 120 ms |
| 状态异常 | 窗口/关键词 + capture focus 启发式 | 豆包 adapter final 后立即 completion；phase 不参与 UI；client 忽略 bridge session_id | “正在听”可早于真正 recording，并存在提前 READY/晚事件串会话风险 |

## 5. 当前可确认的性能结论

### 5.1 高置信度结论

1. **准备阶段的主要可控固定成本是 1.3 s。** 其中 300 ms 在按 Fn 前，1.0 s 在按 Fn 后；实际还要叠加设备、AudioUnit、焦点、输入法和网络操作。
2. **250 ms 时已有就绪观测，却没有快路径。** 日志多次在 `+0.25s` 检出豆包语音窗口；代码仍固定等到 1.0 s 决策。
3. **远端最初 200 ms 尚未开始采音。** pre-roll 只能保存麦克风打开后的音频，不能恢复这段窗口。
4. **partial 桥接刷新上限为 10 Hz。** 这给文字反馈带来最多约 100 ms 的观察/节流等待，但通常小于 ASR 首字延迟。
5. **final 有固定 1.8 s 等待。** 即使豆包更早完成提交，当前也不会提前 emit final。
6. **存在“音频强、语音窗口判定 active、但无任何 partial”的真实日志样本。** 当前健康判据不足以证明 ASR 数据面通畅。
7. **当贝热路径预热了控制连接和遥控器麦克风。** 相比桌面客户端，它消除了每轮 control connect 和 CPAL/AudioRecord cold open；但音频 TCP 和 Mac 激活仍是每会话成本。
8. **当贝豆包路径的 1× pre-roll pacing 会保留 activation offset。** 它不仅影响首字，还会让 F5 UP 后先排空积压再发送 stop。
9. **当贝状态机存在与接口契约冲突的明确实现。** 豆包 final 同步触发 completion，可能早于 matching F5 UP；这应作为异常片段的第一优先核对项。

### 5.2 需要实验验证的假设

- **假设 A：** 每会话切换默认输入设备与重建 AudioUnit/listener 是可观的冷启动成本；目前日志没有单调时钟，无法拆分它们各自耗时。
- **假设 B：** `likelyVoiceUIActive` 可能提前把“窗口出现”当成“ASR 已真正可收音”，造成 UI 显示 listening 但数据面尚未 ready。
- **假设 C：** 若用户在 1.3 s arming 期间已经讲话，5 秒 pre-roll 按原速播放会让 ASR 文字相对真实讲话持续落后约 arming 时长，直到说话结束或积压追平。
- **假设 D：** 1.8 s final delay 比豆包实际 commit 所需时间更保守；需要测量 `hasMarkedText true→false` 与 `textDidChange` 的真实分布。
- **假设 E：** 当贝视频中的准备异常可能来自 editor 首次进入时 recorder 仍在 80 ms debounce/异步 arm；需用 F5 down、editor refresh、recorderReady 时间戳确认。
- **假设 F：** 当贝 `final → immediate completion` 与视频 06:15–06:28 的异常直接相关；代码风险已确认，但片段归因仍需视频时间线验证。

## 6. 可观测性缺口与下一轮测量设计

现有日志没有为每一行附带单调时间戳，因此无法从同一 session 精确计算阶段耗时。建议以后用同一 `ContinuousClock`/monotonic epoch 为下列点输出 `session_id + elapsed_ms`：

1. 客户端 hotkey 收到；Activating 首帧显示；worker thread 开始；控制 TCP connected；`start` write 完成；音频 TCP connected；CPAL `play()` 完成；首个 callback；首个非静音 chunk。
2. Mac 收到 start；默认输入查询/写入/回读各自完成；AudioUnit init/start 完成；audio listener ready；capture focus ready；TISSelectInputSource 完成；Fn down；首次检测到豆包 window/keyword；广播 recording；首个音频 byte；首个 CoreAudio render 非零样本。
3. 捕获窗口首次 `setMarkedText`；首次 partial；每次 partial；Fn up；marked text 消失；`textDidChange`；final broadcast；客户端 final receive；目标窗口 SendInput 完成。

同时应为每个 session 汇总：pre-roll bytes/时长、从 recording 到 pre-roll 排空的时长、Mac ring buffer 高水位与 dropped bytes、音频首包 RTT/连接耗时、partial 数量及最大间隔。这样才能把“界面状态快”与“音频数据面/ASR 真 ready”分开。

当贝端还需要记录：editor eligible、recorder arm start/ready、F5 route、session accepted、start sent、audio connected、recording phase、首个 PCM offer、首个 audio write、pre-roll depth、buffer drained、stop sent、final receive、IME dispatch、commit 完成、matching F5 UP 和 completion。只记录 session ID、字符数和毫秒，不记录文本或 PCM。

## 7. 视频逐帧证据

### 7.1 测量方法与限制

源文件为 `/Users/ldd/Downloads/doubao-voice-bridge-test.mov`，时长 428.402 秒、1088×1706、约 60 fps。`ffprobe` 显示文件**只有 H.264 视频流，没有音轨**，因此不能从文件本身测得真实发声起点 `Tspeech`。下表采用 10 fps 逐帧采样，时间精度约 ±0.1 秒，并同时观察：

- 屏幕顶部 Mac 状态岛：橙色“正在启动语音输入”→蓝色“正在聆听/实时转写”→橙色“正在整理识别结果”→绿色结果；
- 屏幕底部客户端状态岛：灰色准备态、蓝色波形、识别优化态及消失；
- 顶部状态岛从 compact 扩成 expanded 且出现非空文本的首帧，作为 `Tpartial-visible`。

用户确认测试实际从 `1、2、3、4、5…` 开始读数；多个片段最终只保留 `3、4、5、6、7`。因此“约 2 秒首段语音被漏识别”是用户口述事实与最终文本共同支持的**内容完整性结论**，不是从缺失的音轨反推出来的声学测量。

### 7.2 双状态岛时间线

表内时间均为各片段起点的相对秒数。“顶部准备”以首个橙色状态岛为起点；若片段一开始已经是橙色，则记为左截断（`≤0.0`）。“底部波形→首字”比“顶部聆听→首字”更接近投影端用户看到开始响应后的等待。

| 片段 | 顶部准备出现 | 底部蓝色波形 | 顶部进入聆听 | 首个可见文字 | 首批文字 | 顶部开始整理 | 绿色结果 | 底部波形→首字 | 顶部聆听→首字 | 结果/备注 |
|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---|
| `00:00–00:16` | 0.7 | 2.8 | 3.6 | 4.4 | `34` | 11.2 | 13.2 | 1.6 | 0.8 | 最终从 `3` 开始，`1、2` 未补回 |
| `00:49–01:11` | 0.7 | 7.3 | 7.4 | 9.5 | `1` | 17.4 | 19.4 | 2.2 | 2.1 | 准备期异常长；本轮保住开头并继续生成完整序列 |
| `02:40–02:56` | 1.2 | 3.3 | 4.1 | 4.8 | `3` | 10.6 | 12.6 | 1.5 | 0.7 | 最终 `34567`，漏 `1、2` |
| `03:00–03:15` | 1.1 | 3.2 | 4.0 | 4.9 | `34` | 10.6 | 12.6 | 1.7 | 0.9 | 最终 `34567`，漏 `1、2` |
| `03:24–03:38` | ≤0.0 | 0.8 | 1.6 | 2.9 | `34` | 9.2 | 11.1 | 2.1 | 1.3 | 片段起点已在准备态；最终 `34567` |
| `03:44–03:59` | ≤0.0 | 1.2 | 2.0 | 3.1 | `34` | 9.2 | 11.2 | 1.9 | 1.1 | 片段起点已在准备态；最终 `34567` |
| `06:15–06:28` | 1.4 | 3.5 | 4.3 | — | — | 7.9 | 9.9 | — | — | 全程零文本；绿色状态显示“语音输入”，随后正常消失 |

五个出现首段丢失的短片里，“底部波形→首字”的可见等待为 **1.5–2.1 秒**，首批文字又都从 `3/34` 开始。这一重复模式排除了“只是某一帧没有截到 `1、2`”的解释；后续 partial 和最终结果均未把它们补回。

### 7.3 视频揭示的状态语义问题

1. **底部波形与顶部聆听并不严格同步。** 六个正常片段中，底部蓝色波形通常比顶部蓝色聆听早约 0.8 秒出现；长重试片段两者几乎同步。代码上两个 UI 都应由 bridge phase 驱动，因此这段可见偏差需要用统一单调时间戳确认是渲染/状态传播差异，还是底部 UI 在数据面真正 ready 前就使用了“正在听”的外观。
2. **`recording/listening` 不能证明 ASR 已消费首帧。** 视频在进入蓝色状态后仍持续 0.7–2.1 秒才有首字，多轮最终丢掉最前面的 `1、2`；而 `VoiceStateDetector` 只验证窗口启发式与 capture focus。
3. **空结果被展示为成功结束。** 异常片段中系统经历准备、聆听、整理、绿色结束，最终文字为空且没有错误提示；这与日志中的 session 18/20“音频电平明显但零 partial、空 final”一致。
4. **长准备并不必然漏开头。** `00:49–01:11` 虽然准备约 6.7 秒，但首批文字从 `1` 开始。这说明 5 秒 pre-roll/重试链有时能保住开头，也说明问题不是简单的固定裁剪规则；更可能与真正 ASR ready 时刻、5 秒尾部裁剪边界或回放时序相关。

### 7.4 对根因假设的更新

按当前证据排序：

1. **高概率：ready 判据早于豆包真正接受音频。** 窗口在 Fn 后 250 ms 已可见，但“窗口存在”不等于 ASR 数据面已启动；过早放行 pre-roll 会让最前面的缓存音频进入尚未消费的 BlackHole→豆包阶段。
2. **确定存在：快捷键后的前 200 ms 尚未采音。** 这部分无论 pre-roll 多长都无法恢复，但它本身不足以解释约 2 秒丢失。
3. **中概率：准备超过 5 秒时 pre-roll 尾裁。** 长片段的顶部准备为 6.7 秒，理论上最早约 1.7 秒会被裁掉；该轮却从 `1` 开始，说明实际发声时刻可能晚于视觉触发，仍需带音轨实验判定。
4. **低概率：partial 只是在 UI 被覆盖。** 如果只是展示层覆盖，最终文本应有机会补回 `1、2`；五轮最终结果都没有，因此不符合主要现象。

## 8. 证据索引

- `README.md:8-19,66-94,168-173,246-280`：产品数据流、默认运行配置、phase 语义和诊断事件。
- `Sources/DoubaoBridgeMac/main.swift:8-27,1681-1782,1955-2186`：Mac 会话状态机、固定等待、激活/重试/恢复/最终提交。
- `Sources/DoubaoBridgeMac/main.swift:251-445`：capture focus 与 partial 观察/节流。
- `Sources/DoubaoBridgeMac/main.swift:1239-1618`：音频 listener、TCP receive、音量统计。
- `Sources/DoubaoBridgeMac/RealtimeAudioOutput.swift:6-117`：250 ms 环形缓冲及低延迟丢旧策略。
- `clients/desktop-client/src/native_voice.rs:21-27,381-511`：客户端连接、200 ms 起音延迟、pre-roll、停止静音和 final timeout。
- `clients/desktop-client/src/native_voice.rs:213-297,657-727`：pre-roll/pacing/backlog 算法与测试。
- `clients/desktop-client/src/main.rs:388-449,1260-1355,1636-1701,2353-2436`：客户端 phase 映射、Windows/Linux 快捷键与 UI 状态。
- `clients/desktop-client/src/platform_paste.rs:58-168`：Windows 增量文字反馈。
- `~/Library/Logs/DoubaoVoiceBridge/bridge.log:24137-24169,24207-24239`：研究时快照中的“强音频但零 partial”样本；该运行日志会继续增长，行号可能随日志轮换变化。
- 当贝 `docs/ARCHITECTURE.md:12-100`、`docs/adr/0003-use-mac-doubao-bridge-for-asr.md:5-26`：双引擎架构、时序与不变量。
- 当贝 `DoubaoBridgeClient.java:43-59,108-165,184-243,246-459`：常驻控制连接、每轮音频连接、pre-roll/pacing、timeout 和 final。
- 当贝 `DangbeiRemoteAudioChannel.java:44-46,68-185,215-241`：16 kHz BLE HID AudioRecord 预热、40 ms read chunk 和生命周期等待。
- 当贝 `VoiceDictationController.java:246-329,470-575,578-720`：会话门控、结果分发、recorder 预热与 UI 状态。
- 当贝 `ProjectorRecognitionEngine.java:18-25,79-159,317-404`：本机 DDS 引擎、150 ms 启动等待、partial/final 和清理。
- 当贝 `DoubaoRecognitionEngine.java:21-41`：豆包 final 同步触发 completion 的状态机风险。
