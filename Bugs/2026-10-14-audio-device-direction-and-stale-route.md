# 麦克风页面的播放设备列表与失效传送端

- 发现日期：2026-10-14
- 状态：调查中
- 影响范围：Windows 安装版 0.2.11，源码 `79b1af6d02228045fa9ddf4213e92c63811786ba`；RC003 当前连接保留。
- 功能点：连接与语音、音频设备枚举、保存的端点恢复。
- 现象：Windows 已选 CABLE Output，麦克风高级列表仍显示 Speakers，应用显示传送错误。

## 现场证据

通过 Cua 后台快照读取原窗口，未点击设备、改变焦点或重启应用。
页面保存的端点为 `CABLE In 16ch (VB-Audio Virtual Cable)`，报“上次选择的输出端点当前不可用”。
当前列表有两个虚拟播放端 `CABLE In 16 Ch (2- VB-Audio Virtual Cable)`、
`Speakers (2- VB-Audio Virtual Cable)`，另有一个 Realtek 播放端。

公开 Windows 设备查询确认 `CABLE Output (2- VB-Audio Virtual Cable)` 存在。
公开 WinRT `MediaDevice.GetDefaultAudioCaptureId` 的只读结果：Default 为 CABLE Output，
Communications 为内置麦克风阵列。此事实不证明 Codex 的实际设备选择，未访问其私有配置。

当前进程日志：09:50:36（UTC+8）恢复保存端点失败，
`error_code=endpoint_identity_mismatch reason=persisted_endpoint_missing_or_changed`。
09:53:18–09:53:58 的 9 次语音启动均为 `endpoint_not_selected`。
枚举成功，活动播放端共 3 个，其中虚拟端 2 个；不是枚举 API 失败或 VB-CABLE 未安装。
本地诊断尾部副本 SHA-256：`a7c5fa9a78c9a49dff2ecb726ce04e8254e44f856dd02b2c97fb6659062fa1d1`。

## 已确认的实现边界

- `audio.rs::list_endpoints` 使用 `Direction::Render`，所以列出播放端，不会列出 CABLE Output 录音端。
- 主界面“目标应用使用的麦克风 / CABLE Output”是固定说明，不是检测结果，也没有读取目标应用的选择。
- 保存端点恢复要求 ID 与名称匹配；当前旧 ID 不在活动列表。此次未核实端点变化的历史诱因。
- 刷新列表保留已有选择，不会修复失败的音频流；因此列表已更新与实际传送仍失败可以同时出现。
- 不能直接把现有枚举改成 Capture：其选择路径用 Render 打开流并写入遥控器声音。

## 修正边界与验证

应分开检测并展示录音端 CABLE Output 和写入端的状态，避免固定文字冒充已检测/已选中。
高级内部传送列表应明确用途，普通扬声器不应混在麦克风选择中。
失效设置应明确展示不可用；重新绑定须确认对应虚拟线路，不能仅凭名称模糊匹配自动切到任意设备。
VB-CABLE 参考：[官方说明](https://vb-audio.com/Cable/)。

- `passed`：窗口、当前版本、公开设备枚举、两类默认录音设备及同进程日志交叉核对。
- `failed`：当前保存的音频线路恢复和语音启动。
- `deferred`：UI 修正、虚拟线路重新绑定、遥控器音频到目标应用的端到端验证。
- 现场未改变：音频设备选择、系统默认音频、蓝牙、应用进程及连接。
