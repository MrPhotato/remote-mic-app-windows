# 社区反馈：一句后听写停止并重复触发语音热键

- 发现日期：2026-10-19；用户提供 2026-10-18 评论截图。
- 状态：调查中，未确认根因。
- 影响范围：对方版本、Windows 版本、RC001/RC003 型号、听写软件和热键均未知。
- 功能点：语音快捷键生命周期、WeType 响应检测、ATVV 续期。
- 现象：对方描述只能输入一句，之后停止；按键检测似乎显示每隔几秒重新输入语音组合键。重新配对未改善，另一款遥控软件未遇到。不能把“停止”直接记为 Windows BLE 断开。
- 复现条件：未知；本机用户未复现，尚未收到对方日志。
- 正常预期：物理按住期间持续听写，释放后结束，不重复触发已正常工作的听写。

## 已确认的源码线索

1. `send_input.rs::is_wetype_voice_chord` 按两键 Ctrl+Win 识别微信输入法集成，不检查实际目标听写应用。`ble.rs::apply_control_events` 对该组合启动 WeType 检查；其他快捷键（如 Ctrl+Shift+D）不走此路径。
2. `ble.rs::spawn_wetype_check` 在 700 ms 后检查麦克风历史。如果结果是 `NotObserved`，会切换 WeType profile，再等待 2/3/5 秒并安排最多三轮重试。`WorkerMessage::RetryVoiceChord` 真正发送旧和弦 UP 和新和弦 DOWN。重试仍要求同一有效语音会话、同一热键及再次未观察到开麦。
3. 现有 [历史故障](2026-09-09-wetype-history-false-recovery.md) 曾由麦克风旧记录误判造成持续按住时反复停止/重启。现版已聚合历史、处理未知观察并复查会话；截图不能证明该旧根因回归。当前观察仍只覆盖 `ConsentStore/microphone/NonPackaged` 中名称包含 `wetype` 的条目；没有绑定当前进程或目标听写工具。实际记录是否缺失/迟到，需要现场证据。
4. RC003 通过每 2.5 秒写 GATT `MIC_EXTEND`（0x0E）续期，该操作不发送键盘快捷键。已有记录显示缺失续期会使 RC003 约 5.7 秒停止音频。当前定时处理仅在 worker 接收超时分支运行，持续消息积压或续期写入失败可作为另一调查方向，但本次没有命中证据。
5. 同一活跃流内重复 `StreamStarted` 被 core 的 `AlreadyActive` 拒绝，不会直接再次注入热键；重连本身也不按热键。收到新的成功流开始事件、微信重试、音频中止和真实连接失效需分别判断。

## 最小补充证据与判据

先收集应用版本、遥控器型号、听写软件及语音快捷键，以及一次失败前后约一分钟诊断日志和摘要。

- 同一 `epoch` 中 `wetype_check reacted=false` → `wetype_revive` → `chord_retry result=ok`：能确认程序主动重按热键。现日志足以确认此分支，仍不足以证明麦克风观察为什么未响应。
- `direction=C preview=[04 ...]` / `[00 ...]`：遥控器开始/停止流；结合 `voice_hotkey` 与 `chord_release` 判断配对。
- `direction=T preview=[0E ...]`：续期是否持续，结合写入错误判断。
- `ble_connection_invalidation` / `ble_reconnect`：真实连接失效及重连；只见热键重试不能认定蓝牙断开。
- `abort_voice_session` 和音频错误：音频路径中止，需与 BLE 失效分开。

## 修复与验证

- 只读源码审查：passed，两条独立审查得到一致的 WeType 重试线索。
- 对方现场复现、根因及修复：deferred，缺配置和日志；未修改语音时序、重试策略、输入法状态或蓝牙。
- 不要求重新配对、重启蓝牙或重装程序作为本次常规排查步骤。
- 隐私检查：未记录评论者身份、个人路径、设备身份、输入内容或凭据。
