# Windows Codex 定制版初期验证

上游基线：`451e5f9ced0deecd31eb0147c0a578a22924492d`（GetSayAll Windows）。
测试环境：Windows 11 x64；已配对的小米蓝牙遥控器 2 Pro / RC003。

这份记录对应初期 Codex 页面和听写预设阶段，不代表后续提交重新执行了全部检查。后续快捷键与日常方案记录分别见 [LocalCodexShortcuts.md](LocalCodexShortcuts.md) 和 [LocalDailyDefaults.md](LocalDailyDefaults.md)。

`passed` 表示实际执行通过，`failed` 表示观察到失败，`deferred` 表示尚未完成实测；忽略的测试不计为通过。上游历史测试不替代本版本验证。

| 项目 | 状态 | 证据与边界 |
| --- | --- | --- |
| Microsoft C++ / SDK / Rust 编译工具链 | passed | C、Windows 资源、Win32 和 Rust 编译链接及运行 |
| 完整前端回归 | passed | 听写增量后 14 文件、116 测试 |
| Codex 页面与配置测试 | passed | 针对性 3 文件、27 测试 |
| 前端生产编译 | passed | vue-tsc 与 Vite 构建 |
| 完整 Rust 单元回归 | passed | 听写增量后 182 passed、7 ignored |
| 原生程序发布构建 | passed | Windows x64 release 构建成功；验证源码提交为 `2206f170982b94976e100767c560b0fb935ed009` |
| 原生窗口与配置保存/恢复 | passed | WebView2 窗口正常显示；应用、恢复、重新应用初期七键预设，并读取程序自身配置回验。现行日常方案已扩展为十二键 |
| Codex 身份检测 | passed | 通过公开 AppsFolder/AUMID 识别目标应用 |
| 无交互的后台 Codex 聚焦 | failed | 其他应用处于前台时 Windows 拒绝切换；明确报错，未新建或重启 Codex |
| 从应用界面或遥控器触发的 Codex 聚焦 | deferred | 需要实际交互验证 |
| 遥控器 BLE 连接及型号 | passed | RC003 扫描与 ATVV 握手成功；不替代音频或按键实测 |
| 实体按键操作 | deferred | 需要实体按键；软件模拟不能替代 |
| 麦克风到输入法的语音闭环 | deferred | 测试环境未配置完整虚拟声卡音频链路 |
| Windows 10 / 其他遥控器型号 | deferred | 本次未覆盖 |
| BLE 连接中正常退出 | passed | 通过程序退出事件释放 GATT 资源，126 ms 完成并正常结束进程 |
| Codex 听写选项与设置保存 | passed | 原生选项、IPC 保存结果与程序自身配置一致；重启后 RC003 自动重连完成 |
| Codex 实际听写 | deferred | 未执行遥控器麦克风到 Codex 文字的完整流程 |
| Codex 与 WeType 专属逻辑隔离 | passed | 覆盖 Ctrl/Win 判定，以及配置切换、关闭或会话结束后的旧重试处理；不等同于输入法实际听写验收 |

定制版使用独立应用身份与设置目录，关闭上游更新通道。
