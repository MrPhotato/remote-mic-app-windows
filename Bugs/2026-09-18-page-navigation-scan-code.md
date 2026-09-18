# 遥控器 Ctrl+PageUp/PageDown 无法切换 Codex

- 发现日期：2026-09-18。
- 状态：已修复扫描码缺失，等待 Codex 实体遥控复测。
- 影响范围：本地 Windows 版，RC003；用户确认实体键盘组合键可用，遥控器不可用。
- 功能点：普通快捷键注入。
- 复现条件：配置菜单单击 Ctrl+PageDown 后按遥控器。用户另将音量±设为 Ctrl+PageUp/Down，调查期间保留该配置。
- 正常预期：Codex 收到与实体键盘相同的快捷键并切换任务或标签页。
- 证据：本机 2026-09-18 10:12:34–10:12:51 UTC 日志反复出现 Menu DOWN/UP、`map_fire ... Control+PageDown`、`map_inject result=ok`。这只证明触发和提交，不能证明目标应用处理成功。
- 根因：原生对照探针实际确认 PageUp/Down 走 VK 注入时消息扫描码为零，Control 状态正确；显式物理注入对照为 E0+49/E0+51。Chromium 从消息扫描码推导 DOM code；该编码差异已证实，是否完全解释 Codex 现场症状仍需目标复测。
- 调查边界：近期日志未见 VolumeUp/Down 边沿，不能将菜单注入与音量上报问题合并为已确认的同一原因。
- 修复：PageUp/PageDown 使用原有物理扫描码路径发送 E0+49/E0+51；保持 Ctrl DOWN、主键 DOWN、主键 UP、Ctrl UP 顺序，未调整语音、双击、长按或组合键间隔。注入日志明确标为 submitted、目标结果 unknown。
- 验证：Windows 平台库 145 passed、6 ignored；独立原生窗口修复前两项运行时注入均 scan=00，物理对照两项 passed；修复后即时四项均 passed，Ctrl 按下/释放完整。60秒闲置测试因窗口关闭/失去前台而安全取消，零注入，记 deferred；Codex 实体遥控复测 deferred。
- 探针：`cargo run -p sayall-windows --example shortcut_scan_probe -- --run`；可加 `--idle-seconds=60`。仅自己的测试窗口持有前台与焦点时发送，不向其他窗口注入。
- 隐私检查：仅记录配置键名、扫描码、修饰键状态与匿名结果，不记录设备身份、用户文本或第三方私有状态。
