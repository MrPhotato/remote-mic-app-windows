# RC003 Helper 通信线程空闲空转

- 发现日期：2026-10-07
- 状态：已修复，等待安装版真机复验
- 影响范围：Windows，个人维护版 0.2.8 的 RC003 可选三键 Helper。
- 功能点：Helper TCP IPC。
- 现象：增强启用后，主程序 `sayall-rc003-input` 线程在没有操作时持续占用约一个 CPU 核心。
- 复现条件：非阻塞监听 socket 接受连接，接受端设置接收超时但没有恢复阻塞模式；Helper 心跳使连接保持存活。
- 正常预期：空闲时有界等待，收到帧后立即处理，EOF、停止和心跳失联仍进入原有清理路径。
- 证据：现场线程公开名称及 CPU 计数：3.11 秒内消耗 2.9375 秒线程 CPU。独立 Winsock 对照：继承非阻塞模式时，100ms 内空读 82,557 次；显式恢复阻塞模式后，100ms 超时实际等待约 112ms，无可测线程 CPU 消耗。没有修改现场 socket。
- 根因：Windows `accept` 继承监听端非阻塞属性；`SO_RCVTIMEO` 不会把非阻塞读取改为等待。`WouldBlock` 被转为空帧集合，外层无等待再次读取。
- 修复：接受端先恢复阻塞模式，再设置原有 100ms 读超时、1s 写超时及 nodelay；配置结果记录阶段、系统错误码和耗时。
- 验证：`cargo test -p sayall-windows rc003_input::windows_runtime::tests`，7 passed。新增真实 Windows loopback 回归覆盖非阻塞 accept、空闲读取频率、帧顺序和 EOF；安装版空闲 CPU、实体按键、正常退出仍 deferred。
- 边界：该线程不承载 ATVV；此次语音不触发在用户重新配置 Codex 快捷键后恢复，不将语音问题归因于空转。
- 隐私检查：本记录无设备标识、地址、端口、个人路径、用户输入或语音数据。
