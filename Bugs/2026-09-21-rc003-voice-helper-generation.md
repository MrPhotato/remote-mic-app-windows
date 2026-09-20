# 开始语音时三键 Helper 不必要重绑

- 发现日期：2026-09-21。
- 状态：代码已修复，等待新包真机验证。
- 影响范围：本 fork 0.2.6、来源 `b67a397b5d62cbb8dcb5003bf5d65547d5fd4548` 的 Windows RC003 安装包；不能外推为 RC001 的实测结果。
- 功能点：可选三键 Helper 的连接选择与失效代次。
- 现象：同一 BLE 连接上，每次开始语音都会停止旧三键来源、重新 attach/load，并回到等待真实中性状态。
- 复现条件：RC003 已连接、普通按键监听运行、三键增强已初始化，连续发起多个语音会话。
- 正常预期：Ready → Streaming → Draining → Ready 期间保持同一三键来源；真实连接替换、监听失效、目标变化或 Helper 故障仍须取消旧来源并重新初始化。

## 原始证据与根因

安装版日志 `installed-app-final-profile.log` 中，UTC 2026-09-20 17:21:06.549 至 17:35:52.599 共 12 次音频会话开始，全部紧跟 Helper generation 3 至 14 的重建。音频开始完成与 Helper 进入等待间隔为 0–1 ms；同段电池日志的真实 `connection_generation` 始终为 1。脱敏逐次统计见 [证据](../Testing/evidence/rc003-voice-helper-generation-20260921.json)。这里确认的是不必要重绑，未据此声称某次用户按键已经丢失。

`ConnectionSnapshot.generation` 由 `ble.rs` 的 `PipelineOutput::StreamStarted` 和音频处理路径写入，是 ATVV 语音流代次。Helper 的 availability 闭包误把它作为连接代次，supervisor 比较选择元组时因此取消来源。Python Helper 收到新代次后按既有契约清理、重新 attach 并等待真实 neutral。这不是有意避让语音：availability 原本明确允许 Streaming 和 Draining。

## 最小修复

- `ConnectionSnapshot` 增加内部 `connection_generation`，通过 `serde(skip)` 保持公开 JSON 中原语音 `generation` 契约不变。
- `attempt_connection` 的 Connecting 与 AwaitingCapabilities 完整快照使用现有 BLE worker 连接计数；代次、phase、型号在同一个 state 锁下一起发布和读取。非活动默认/终态快照的计数为 0，且不可用；新活动连接不会复用旧代次。
- Helper availability 改读此连接字段；语音开始/结束不改它。原有语音代次、重连策略、时间常量、Helper 清理、监听 epoch 和真实 neutral 要求不变。
- 成功连接与 Helper 选择变化日志增加匿名连接代次、可用性和监听 epoch，用于区分真正换连接与语音会话变化，不记录目标 ID。

## 补修：清理阻塞前发布失效

交叉代码审查另发现既有窗口：`invalidate_connection` 原先先增加 worker 局部代次，再执行快捷键释放、音频与 BLE 清理，最后由调用者发布断连/重连状态。如果 Windows 清理阻塞，共享快照仍可能是旧 Ready。此项是代码审查发现，**不是新增真机失败记录**。

所有调用 `invalidate_connection` 的路径现在先在同一 state 短锁内发布新连接代次；原本仍在线或重连的 phase 暂置为 Disconnected，既有 Failed 等非活动状态与错误信息保留。随后同步既有普通键门控/F5 状态并记录 `ble_connection_invalidation phase=published`，再调用原清理逻辑。状态锁不跨清理调用；调用者原有最终重连、睡眠、失败策略不变，不调整按键边沿或时间常量。

回归通过生产代码使用的同一个清理闭包边界阻塞模拟清理，观察者在阻塞期间使用 `try_lock` 验证锁已释放、新代次已可见、Helper 与普通门控不可用；覆盖 Ready、Streaming、Draining、AwaitingCapabilities、Reconnecting、Failed，且模拟清理失败不会重新开放旧状态。该测试只证明发布顺序和状态守卫，不证明 Windows 清理本身或新包硬件恢复。

## 验证与边界

- `cargo test -p sayall-windows rc003_ --lib --locked`：18 passed，0 failed。新增回放覆盖 12 次语音会话及各阶段不改变选择；两次轮询之间完成真正重连，即使语音代次相同也改变选择；监听 epoch、监听停止、连接阶段和 RC001 排除继续生效。
- `cargo test -p sayall-windows --test ipc_contract --locked`：1 passed，0 failed，原 JSON 契约保持不变。
- 上述清理窗口补修后的 `rc003_ --lib` 定向复验：19 passed，0 failed，其中包含受阻清理的新回归。
- 新包 RC003 实体语音/三键、真实断连与睡眠恢复：**deferred**。本次未构建安装包、未启动或退出应用、未操作遥控器。
- 隐私检查：归档只含 UTC 时间、公开提交、匿名计数和代次；不含个人路径、设备身份、语音、输入内容或凭据。ignored 原始日志不提交。
