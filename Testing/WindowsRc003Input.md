# RC003 可选三键增强应用集成验证

日期：2026-09-20。范围见 [ADR 0003](../docs/decisions/0003-rc003-optional-input-helper.md)。
独立探针的三键可见性结果见 [WindowsRc003Frida.md](WindowsRc003Frida.md)，
不能替代本页的安装后高亮、映射和生命周期验证。

## 自动化结果

- passed：`cargo check --workspace --locked`。
- passed：Windows 库回归，157 passed、0 failed、6 ignored；包括三键来源合并、
  generation/sequence 拒绝、真实中性状态门禁、重复来源去重、断开及退出取消。
- passed：监督线程及启动路径定向测试，5 passed、0 failed；包括路径兼容性回归和此前 4 项测试。
  停止令牌与换代/转发共用短锁；阻塞读写在锁外。
- passed：前端全量 17 文件、145 tests；随后连接/监听前置提示定向 7 tests passed；修正未设置音量键的动作摘要后，ButtonsPage 定向 27 tests passed。
- passed：最终 Helper 21 项 Python 测试及 JS observer 测试。包括父进程失联、
  租约超时、旧代/非法帧、初始按住、中性状态、源切换和发布租约/创建 capture 交错。
- passed：最终 Helper 96 个文件的 SHA-256、根 manifest 副本与对应源码一致性；
  x64/windowed/asInvoker 打包，不要求用户自行安装 Python。
- manifest SHA-256：`65de7e42cb2a2845e14e8a790f2047a1fa37a751581657bba533c809f9678ae2`。

完整编译和测试日志保存在 ignored `target/local-launch/rc003-integration/`；
Helper 构建日志位于 ignored `target/rc003-helper/logs/`。
启动阶段脱敏证据见 [rc003-input-startup-20260920.txt](evidence/rc003-input-startup-20260920.txt)。

## Windows 实机验收

最终本地包已完成 96 个组件逐项散列检查，并在应用预先正常退出后完成主程序替换。
已验证新主程序以普通权限运行，显式启用增强后建立 IPC，并完成 capture attach/load。
应用仍运行时的首次覆盖未替换主程序，不能记为升级通过；实体三键及相关行为仍待验。

| 项目 | 状态 |
| --- | --- |
| 完整本地包安装、组件完整性与普通权限主程序启动 | passed（应用预先正常退出后；主程序及 96 个组件已核验） |
| 显式启用至 Helper 建立 IPC | passed（修复后新主程序；此前包退出码 21 为 failed） |
| 精确来源绑定和真实中性状态初始化 | 待执行 |
| 返回、音量加、音量减高亮的成对 DOWN/UP | 待执行 |
| 返回映射、长按与闲置后首用 | 待执行 |
| Helper 停止时仍按住的键清理，不触发取消后的动作 | 待执行 |
| 主程序正常退出与升级 | 正常退出 passed；运行中覆盖首次 failed；预先退出后安装 passed；活动 Helper 升级待验 |
| 活动 Helper 解钩与宿主存活 | 待执行 |
| 基础语音回归 | 待执行 |
| RC001、另一台 RC003、多目标选择、共享宿主另一设备负对照 | deferred |
| 睡眠唤醒与真实宿主异常退出 | deferred |

### 本地部署与主程序替换证据

- 本轮两次安装使用同一个安装包，SHA-256 为
  `DA68D871369197998B5AD67CA6CE6853D037C5722030B041F49F58145C4C24A3`。
- failed：应用仍运行时发起首次安装，安装器返回 0，Helper 的 96 个文件与最终 manifest 一致，
  但已安装主程序仍是旧文件（SHA-256 前缀 `2B7342FA`，16,102,400 字节），
  且缺少新代码的 `payload_preflight` / `bootstrap_path` 标记。原安装验收只检查 Helper，
  漏检主程序；安装器退出码 0 和 Helper 完整性均不能单独证明完整升级成功。
- passed：先请求应用正常退出并确认进程结束，再运行同一个安装包，主程序更新为
  16,111,616 字节，SHA-256 为
  `241CE21D867A506628940C40CF64C1E69167827BDAE00FE75E24930ABD9D2CEC`。
  它与构建目录主程序（SHA-256 为
  `5C510A3D8FBA527A7A0CBA82E517F51AEE12A9FA7831888239380D542F6CEB04`）
  等长，逐字节比较仅有 bundle 类型标记 `UNK` → `NSS` 的三字节差异；
  新代码标记及内嵌最终 manifest 均存在。因此不能要求 NSIS 安装后的文件直接等于未打包主程序的摘要。
- passed：新主程序于 `2026-09-20T11:10:45Z` 记录 `payload_preflight` 成功、
  `bootstrap_path` 路径已简化、IPC 认证成功及 `capture_attached` / `capture_loaded`。
  这只证明安装后的启动链路，不能替代实体三键、中性状态、高亮、映射或语音验证。
- 边界：首次主程序覆盖失败的根因仍未确定，不能归因为相同版本策略、MSIX 重定向或退出时文件锁。
  生成的 NSIS 脚本包含无条件主程序 `File` 指令，未设置跳过覆盖；当前退出日志也不足以证明
  当时该指令执行瞬间的文件可写性。后续运行中升级须同时检查主程序实际字节/打包标记、
  内嵌 manifest、全部 Helper 文件及进程启动日志；安装器代码本轮未因此修改。

### 启动路径兼容性缺陷与修复证据

- failed：已安装包的增强引导在 IPC 建立前退出，阶段码为 21。
  独立复现表明 Windows PowerShell 5.1 的 `Join-Path` 处理带 `\\?\` 扩展前缀的盘符路径时抛出
  `PSArgumentNullException`；相同目标的普通路径 passed，PowerShell 7 两种路径均 passed。
- Rust 在交给 PowerShell 的边界采用 `dunce::simplified`，与 Tauri 的路径处理方式一致；
  只在可安全保持语义时移除扩展前缀，不安全的 UNC 路径保留原形式。
  对应路径兼容性与字面量引用回归已纳入上述 5 项定向测试并 passed。
- 缺陷复现、路径转换回归及新包实际提权启动至 IPC 已 passed；实体三键、高亮、
  映射和语音回归仍待执行，不能以组件构建或自动化测试替代。

多 RC003/残留设备实例目前保持唯一目标门禁；拒绝歧义不会扩大到整个宿主。
本模式仍依赖非公开 Windows UMDF 实现与管理员 Helper，不是普通权限公开 API 方案。
测试期间不修改驱动、Secure Boot 或测试签名状态；先前签名实验的证书状态另计。
