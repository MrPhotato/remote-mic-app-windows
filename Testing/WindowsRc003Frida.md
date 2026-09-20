# RC003 Frida 独立监听实验

日期：2026-09-20。目标：验证返回、音量加、音量减是否能在 Windows 转成键盘事件之前读取。
这是一项用户明确授权的注入诊断例外，尚未接入 SayAll 主程序。

## 实验范围

- 主程序已正常退出；独立 Python x64 helper 提升权限，仅在自身令牌启用 SeDebugPrivilege。
- 不安装输入驱动，不修改 Secure Boot、BCD 或系统 Frida 环境；不启动上游完整 Gadget/helper。
- Frida `17.15.3` 官方 PyPI Windows x64 wheel 在 ignored `target/local-launch/rc003-frida-probe/` 中按 `--target` 安装。wheel SHA256 为 `12349027d6a4485292db973922a81a9c5cad2b64c5a16453307a22661d79de5d`，与该固定版本 PyPI 元数据一致；import 与 x64 检查 passed。
- 实际采样改用同目录隔离安装的官方 `17.18.0` wheel，SHA256 `4bcf171a0ae184e30e95f414ce3a8f5e92e75e5c8c04ce36cea5eafe632070b1`，与 PyPI 元数据一致。旧版本不覆盖或删除，系统 Python 环境不变。
- 只选择唯一匹配的 BTHLEDevice HID 1812 / VID 012717 / PID 32b8 / REV 00a4 实例；核对 ContainerId、完整 Enum 中 HostPid 成员、系统 WUDFHost 映像、x64、进程创建时间，并持有进程句柄。多目标、权限不足、身份或成员集合变化都停止。
- 本机 HostPid 对应两个不同 Container 的 BLE HID 实例，独占方案正确闭锁。改用上游的严格来源绑定：从 WUDFHost 实际导入的 DeviceIoControl 入口捕获活动调用帧，验证调用点、UMDF 对象/接口及注册表 ContainerId，只有来源等于选中遥控器且 Nt 调用参数一致才读取报告。保留非公开实现读取的实验边界；不得通过报告内容猜来源，也不使用共享宿主上的独占回退。
- 使用上游已知 IOCTL、输入长度与 metadata、9 字节 Report 1 形状；只输出上/返回/音量±/确定的语义快照边沿及匿名计数，不输出其他按键内容、设备身份、原始报告、语音或内存地址。
- 不修改报告、返回值或按键映射。注入脚本 10 秒租约失效自动解钩，绝对上限 10 分钟；helper 约每 2 秒重新核对宿主归属后续租。Frida 同步调用可取消，attach 20 秒、其它调用 5 秒上限。
- 正常停止顺序为 listener.detach → script.unload → session.detach → 复核宿主仍存活 → 关闭自有进程句柄。不结束或重启 WUDFHost。

## 验证与边界

- passed：Python 语法检查、Node 语法与模拟验证（按下/释放成对、重复快照去重、无关 IOCTL 与失败调用过滤、未知 usage 不输出、租约到期解钩和重复停止）。模拟验证不等于真机通过。
- 首次提升预检在 attach 之前停止：诊断程序把设备服务键名要求为完全相等，而 Windows 存在额外后缀。只读核对上游的前缀 + 硬件 token 匹配后，本机服务键和实例均只有一个；修正匹配仍保留唯一性门禁。
- 另修正 HostPid 元数据接受 REG_DWORD/REG_QWORD 整数而非只接受 DWORD；本机实际为 QWORD。普通用户预检的 OpenProcess 返回 Win32 5，后续由已授权的提升 helper 完成验证。
- `17.15.3` 两次提升 attach 在约 0.3 秒后报 `ProcessNotRespondingError`，原文为宿主拒绝加载 frida-agent 或注入时退出；后续进程创建时间/归属/存活检查证明宿主未退出。脚本尚未加载，因此这些不是按键采样失败。
- `17.18.0` 于 09:40:01.052 UTC attach 成功，09:40:01.087 UTC 脚本已加载且 `hook_ready` / `capture_ready`；初始计数为零、来源绑定钩子 ready。上游自身 ACL 修复是相关参考，未证明唯一根因。该阶段只验证运行库和钩子就绪，不代表实体按键可见。
- passed：本机 RC003 三键可见性和本次正常退出清理。产品高亮、映射、异常退出和恢复验收仍 deferred；仅本次结果不能标记这些项目通过。
- 入口快照先在 onEnter 保存，成功返回后建立边沿；非零 NTSTATUS 和 pending 单独计数。返回快照是函数返回时的内存内容，记录 IO_STATUS_BLOCK 完成字节数辅助判断，不能当作独立物理报告。
- 只验证 RC003；RC001 未测试。即使三键读取成功，长按、冷首用、断连恢复、主程序高亮与映射、语音回归仍须另行验证。

来源与许可见 [ATTRIBUTION.md](../ATTRIBUTION.md)。本地诊断源码与日志留在 ignored 目录；不分发上游改编代码或二进制。

## 实体采样结果

用户确认序列：上键 2 次 → 返回 3 次 → 音量加 3 次 → 音量减 3 次 → 上键 2 次。
`sample-04.jsonl` 的捕获窗口为 09:40:01.087 至 09:41:12.371 UTC，
匿名关键事件见 [evidence/rc003-frida-20260920.txt](evidence/rc003-frida-20260920.txt)。

| 测试键 | 入口快照 DOWN | 入口快照 UP | 结论 |
| --- | ---: | ---: | --- |
| 上键（前后正对照） | 4 | 4 | passed |
| 返回 | 3 | 3 | passed |
| 音量加 | 3 | 3 | passed |
| 音量减 | 3 | 3 | passed |

- 共 26 个成功的目标 copy 调用、26 个 Report 1 入口快照；失败、pending、解析异常和未知 usage 均为 0。来源验证无失败，报告读取前均通过选中 Container 的调用帧验证。本次另一设备未产生被过滤的候选调用；这不等价于共享宿主中另一实体设备的负对照已验收。
- 返回时快照得到同样 26 个边沿，不能重复计数为 52 个物理边沿。`IO_STATUS_BLOCK.Information == 9` 计数为 0，符合本实验观察的是进入内部 copy 前已存在的报告缓冲区；不能将返回时内容表述为系统又交付了一份 9 字节输入。
- 停止时所有测试键均已释放。09:41:12.375 UTC script.unload passed，随后 session.detach 为 application-requested / crash=false；09:41:12.401 UTC 宿主原创建时间和成员集合复核 passed。helper 已退出，Secure Boot 仍为 enabled，SayAllHidFilter 服务仍不存在。本次没有更改驱动或启动安全设置；更早驱动签名准备留下的本地测试证书与本实验无关，不能宣称整台机器的信任状态从未变过。
- 结论：本机遥控器实际发送了这些 usage，普通 Raw Input / 此前 GameInput 路径未交付，但提前观察 WUDFHost 内部报告可取得完整三键生命周期。纯软件旁路可行，不需要因此关闭 Secure Boot 或安装三键过滤驱动；该旁路仍需要管理员 helper、进程注入和对非公开 Windows 实现的依赖，不等于普通权限公开 API 方案。
- 当前没有改动或重新启动主程序，UI 高亮与自定义动作仍未接入此来源。正式集成、分发许可审查、长按/闲置首用/睡眠/断连/进程异常退出及语音回归均另行验收，不把独立诊断成功称为产品完成。

采样源码 SHA256（本地 ignored 文件，无设备信息）：

| 文件 | SHA256 |
| --- | --- |
| run_probe.py | `99D00A7654044AD7F78923EF6244BD57243E4B28F9E4CAE2E0E8452FAE09D012` |
| observe.js | `3369D9BF40A2C23B93C704F55247A6775E7850D5538F0AC126A630B164E0274E` |
| source_binding.js | `93E987F0A5AD3FC2C34BEE431CB063277C0A0932E83FF65715001C8DF82BF34E` |
| target_guard.py | `AF9AB12D826C2C4CD1D42ED9DDD78523CCA1515C5A09BA896568E6FD181B624A` |
