# 确认键长按前原生 Enter 泄漏

- 日期：2026-10-20。
- 状态：旧版实体闲置首按已复现原生 Enter 泄漏；有限扩展已实现，修复版待实体回验。
- 影响：本机 RC003、0.3.13。用户反馈长按确认键直接发送，普通键盘 Ctrl+Enter 正常。
- 配置：Ok Single=Enter、Double=Disabled、Long=LeftControl+Enter，映射已开启。

## 现场证据

第一轮日志：

1. 上一个遥控器方向键完成时，累计泄漏计数为 1。
2. 04:09:06.653 UTC，`map_edges detail=Ok=true gate(sw=0 lk=2)`。
3. 04:09:07.213 UTC，`map_fire button=Ok trigger=Long action=shortcut chord=LeftControl+Enter`。
4. 编码记录为主 Enter 扫描码 `1c`，随后 `map_inject result=submitted target_result=unknown`。
5. 04:09:08.406 UTC，Ok 释放，泄漏计数仍为 2。

第二轮同样在 Ok DOWN 后约 556ms 执行 Long；累计泄漏数已增长到 7。
计数是全局累计，不能将这次全部增量归给确认键，也不能以 submitted 证明目标应用收到排队动作。
未读取聊天内容。原始日志留在忽略目录，仓库仅保留以上脱敏结论。

## 对应实现与判断

- `key_gate.rs` 的常见键归因最多等待 60ms，武装有效期 4 秒。
  已有真实实验说明部分 RC003 首沿在返回钩子链之后才收到 Raw Input；
  因而闲置首按不能及时归因，原生 Enter 可能先放行。
- Ok 配置长按后，手势层不会在 DOWN 或长按后的 UP 额外触发 Single；
  550ms 后只触发 Long。本次两次日志均为 Long。
- `native_pending` 只抵消“Single 且与原生同键”的重复注入，
  Long 无法撤回已经交付的 Enter。
- Ctrl+Enter 的编码顺序仍为 Ctrl DOWN、Enter DOWN、Enter UP、Ctrl UP；
  既有软件注入测试只验证这部分，没有覆盖实体遥控器闲置首沿。
- 当前 Helper 覆盖返回、音量±以及单独授权的 TV，不覆盖确认键。

用户随后在 0.3.13 本地接收页复现异常，并导出仅含固定键名、修饰键与时间的记录。
测试框真实闲置 88,121ms 后的事件为：

| UTC | 接收事件 | Ctrl | repeat |
| --- | --- | --- | --- |
| 04:28:31.566 | Enter DOWN | false | false |
| 04:28:32.115 | ControlLeft DOWN | true | false |
| 04:28:32.118 | Enter DOWN | true | true |
| 04:28:32.119 | Enter UP | true | false |
| 04:28:32.120 | ControlLeft UP | false | false |

五条事件均为 trusted，Shift/Alt/Meta 均为 false；与应用日志
31.555 Ok DOWN、32.111 Long、32.118 submitted、33.397 Ok UP 精确关联。
由此确认：普通 Enter 先泄漏，552ms 后的 Ctrl+Enter 被接收端标为重复 Enter。
计数器将 repeat 单独计数，所以页面的普通 Enter=1、Ctrl+Enter=0、重复 DOWN=1
不能解释为没有发送组合键。该轮证明旧版故障，不代表修复后实体验收通过。

## 拟议修复范围

需要将现有 RC003 Helper 的有限报告处理授权从 TV 扩展到确认键：

- 仅所选 RC003、已验证来源、有效会话且观察过真实中性状态。
- 只接管确认键对应的报告槽位，读取与清除成功后才上报语义边沿；
  普通键盘 Enter 不进入这条处理路径。
- 复用既有单击/长按配置及 550ms 阈值，不更改用户映射或语音时序。
- Helper 协议、租约与状态合并同步增加确认键所有权，防止原生与增强来源重复触发。
- 配置更换、断连、Helper 失效与按住退出均取消未完成手势；严格保证泄漏 DOWN 的 UP 放行。
- 本地测试包先验证。不开关蓝牙、不装驱动、不扩展到其它按键或第三方应用。

用户已明确答复“允许，仅扩展确认键并本地验证”；授权同步到 AGENTS.md 与 ADR 0003。
不会通过延长全局吞键窗口或一律吞掉物理键盘 Enter 来规避来源隔离。

## 验收

- [x] 用户当前配置、两次长按日志和源码时序交叉核对。
- [x] 获得仅扩展确认键并本地验证的明确授权；实现与失败边界自动回归另行记录。
- [x] 实现与失败边界自动回归：Rust 419 passed / 10 ignored，
  Helper JavaScript 45 项、Python 63 项及页面 83 项 passed。模拟不替代实体结果。
- [ ] 本机冷首按及闲置首用：接收端不先收到普通 Enter，长按只收到 Ctrl+Enter。
- [ ] 短按仍为一次 Enter，普通键盘 Enter / Ctrl+Enter 不受影响。
- [ ] 按住时停止增强、切换配置或断开连接，不粘键、不补发单击。
- [ ] 用户在同一目标应用确认长按排队/插队正常；RC001 不据此宣称通过。

### 停止增强的边界

停止或失效后 Helper 会卸载报告处理，恢复遥控器原生行为。若此时仍按住确认键，
后续原始报告可能重新触发 Enter；内部取消只保证不补发映射 Single，不代表 Windows
绝不会收到原生 Enter。不会以全局吞键或无期限保留注入规避这一边界。
界面提醒关闭增强前先松开遥控器按键。按住停用的真实采样仍须单独记录。
