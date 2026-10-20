# 旧默认麦克风恢复目标缺失，阻塞当前设备选择

- 发现日期：2026-10-20。
- 状态：根因已定位，修复及安装版复验待完成。
- 影响范围：本机 0.3.12；RC003 保持连接。故障独立于微信输入法快捷键。
- 功能点：默认录音设备恢复、麦克风选择与错误呈现。

## 现象与预期

界面显示目标 CABLE Output「暂不可用」，同时诊断显示传送状态「已就绪」、
内部传声线路为 VB-CABLE 的 Speakers 接口。CABLE 同时列为默认录音与通信设备。
手动重试 CABLE、选择物理麦克风均失败：
`默认麦克风操作失败（get_capture，0x80070490）`。

旧恢复记录不能恢复时，应准确说明缺失的是上次的原默认设备。
不能将当前目标或传声线路误报为失效，也不能通过重启驱动处理应用恢复状态。
原默认设备无法确认时，不猜测另一设备，不自动抹掉恢复记录。

## 现场证据

- 03:17:00 UTC 启动：当前虚拟传声线路验证通过，随后三个角色均在
  `restore_write` 返回失败；紧邻错误为 `get_capture / 0x80070490`。
  `restore_final_readback` 保持 pending，最终 startup failed。
- 03:31:15–03:31:44 UTC 用户重试：每次均重复上述旧记录恢复失败。
  选择物理麦克风时，跳过虚拟传声线路准备，仍在恢复旧记录时失败。
- 独立、只读 Core Audio 检查：当前 CABLE Capture 可打开且 active；
  三个 Capture 默认角色均指向该目标。旧记录的三个 original 相同，
  GetDevice 均返回 `0x80070490`，全部状态的 Capture 枚举中也没有该端点。
- 旧 original 为原生端点标识，没有 `capture:` 前缀或模拟 ID。
  当前信息不足以确定该设备过去的名称、类别或消失原因。
- 同一 original 已存在于 0.3.12 安装前的备份，并与此前真实 Core Audio
  基线记录的一个角色一致。未发现模拟测试 ID 污染证据，不能归因于本次安装。

## 根因

`default_microphone.rs` 在新会话首次选择前要求恢复上次仍持有的 lease。
目标 Capture 验证成功后，恢复尝试访问 `lease.original`；旧设备不存在，
导致整个选择失败。重试仍走同一路径。

前端将所有选择失败统一映射为目标「暂不可用」，并在诊断中重复显示同一错误。
`restorePending` 文案还承诺退出恢复，未区分恢复已失败的情况。

Microsoft 的 GetDevice 文档将 E_NOTFOUND 定义为设备标识未对应当前系统设备；
端点标识可能在驱动重装或更新后变化。这些可能原因尚未在本现场确认。
参考：
[GetDevice](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdevice)、
[Endpoint ID Strings](https://learn.microsoft.com/en-us/windows/win32/coreaudio/endpoint-id-strings)。

## 修复边界与验证

- passed：当前安装版 UI、结构化失败链和独立只读设备检查相互吻合。
- deferred：安全重新指定失效的退出恢复目标；用户尚需确认新的恢复设备。
- deferred：生产修复、正常退出恢复、连续听写、输入法切换及闲置首用复验。
- 未重启应用、遥控器、蓝牙、音频服务或驱动；未修改恢复记录或默认设备。
- 未将当前结果推断为驱动无任何问题，仅确认本次报错的直接阻塞点。

详细证据留在忽略目录。本文不含个人路径、端点标识、设备身份、语音内容或凭据。
