# 确认键长按 Ctrl+Enter 的按键标识缺失

- 发现日期：2026-09-21。
- 状态：扫描码缺陷已修复并通过安装版实际事件验证；目标应用实体动作待复验。
- 影响范围：Windows、当前 RC003 配置，以及按 KeyboardEvent.code 匹配 Enter 的 WebView / Chromium 界面。
- 功能点：普通快捷键注入，不涉及语音时序或手势阈值。
- 现象：用户已将确认键长按配置为左 Ctrl+Enter，但报告长按无预期效果。
- 复现条件：0.2.7 本地候选，按键映射启用；原生界面读到确认键长按已配置。18:19:27.764Z 收到 Ok DOWN，18:19:28.315Z 触发 Long / LeftControl+Enter，随后记录提交成功，18:19:28.721Z 收到 UP。
- 正常预期：接收窗口得到一次带 Ctrl 的主 Enter DOWN/UP，随后 Ctrl 释放，不先发普通 Enter。
- 证据：在自有 SayAll WebView 中调用已存在的 test_button_mapping IPC，捕获原生可信键盘事件；修复前 Ctrl 标识为 ControlLeft，Enter 的 key 正确而 code 为空，Ctrl 状态和四条边沿正确。未读取第三方内部数据，也未向真实聊天发送测试消息。
- 根因：Enter 原来仅发送 VK_RETURN、wScan=0，导致 Chromium 无法恢复物理按键标识。已确认这是注入数据缺陷；尚不能仅凭该实验断言目标应用没有其他上下文条件。
- 修复：沿用 PageUp/PageDown 已验证的扫描码路径，为主 Enter 使用 0x1C、非扩展标志；增加编码日志和单键、Ctrl / 左右 Ctrl / Shift 的边沿回归。不改变用户配置、默认预设或长按阈值。
- 验证：修复前真实 WebView 复现 passed；SendInput 定向 29 tests passed；完整七步 preflight passed（前端 163、Rust 288，既有 ignored 7）。新安装版实际 Ctrl+Enter 四条可信事件、普通 Enter 两条事件均 passed，Enter.code 已为 Enter、Ctrl 最后释放；间隔约 80 秒的首个软件注入同样 passed。生产仿真隔离、96 文件 Helper 与实际安装文件核对 passed。此构建来自 b9c3f7d 加本条未提交修复，不能误认成干净 b9c3f7d 产物；最终公开包由合入 main 后的精确源码重新构建。目标应用实体长按、严格硬件冷首用仍 deferred。见 [独立证据](../Testing/evidence/enter-shortcut-identity-20260921.json)。
- 隐私检查：只记录固定键名、布尔值和时序，不包含输入文本、个人路径、设备身份或凭据。
