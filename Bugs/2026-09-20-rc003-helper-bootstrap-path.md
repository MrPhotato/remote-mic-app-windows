# RC003 三键增强引导与 Windows PowerShell 5.1 路径不兼容

- 发现日期：2026-09-20。
- 状态：已修复（仅本机已安装包的启动路径缺陷；三键功能验收未完成）。
- 影响范围：SayAll 0.2.6 本地三键增强测试包，Windows 11 x64（系统 build 26200），系统 Windows PowerShell 5.1；Tauri 2.11.5、dunce 1.0.5。PowerShell 7 用于对照。该缺陷发生在 Helper 建立 IPC 前，未进入 RC003 报告读取；基础语音不依赖此引导。
- 功能点：显式启用 RC003 三键增强后的管理员 Helper 引导。
- 现象：UAC 启动请求成功，随后引导进程退出码为 21，主程序报告 `helper_start_failed`，未收到三键状态。
- 复现条件：将带 `\\?\` 扩展前缀的盘符路径交给 Windows PowerShell 5.1 的 `Join-Path`，用于定位组件清单。
- 正常预期：在保持路径语义的前提下读取和验证固定清单，完成受保护目录部署、启动 Helper 并建立 IPC；启动请求成功不能等同于组件就绪。

## 证据与根因

1. 此前 96 文件安装包的组件散列、普通权限启动和正常退出已通过；这些事实不能单独证明主程序覆盖升级成功。ignored `target/local-launch/rc003-integration/installed-app-warm.log` 在 `2026-09-20T10:43:38.644Z` 记录 `helper_launch result=started main_elevated=false`，随后在 `10:43:41.110Z` 记录 `startup_exited exit_code=21` 和 `helper_start_failed`；报告数、边沿数均为 0。阶段码 21 对应清单读取、散列或解析阶段，单独看此码不足以确定具体原因。
2. 同会话最小对照已确认：PowerShell 5.1 对扩展前缀形式的 `Join-Path` 抛出 `PSArgumentNullException`；相同目标的普通路径通过，PowerShell 7 两种形式均通过。普通路径的提升权限清单预检也通过，脱敏结果保存在 ignored `stage-prefix-elevated.json` 和 `stage-prefix-elevated-actual.json`，后者确认读取 96 项清单。
3. 已确认的机制是 PowerShell 5.1 路径提供程序不接受本次扩展前缀形式；它与安装包失败阶段一致。对短命提升子进程的观察未捕获完整启动命令，不能声称已逐字核对该次失败的编码脚本或所有路径来源。此前缺少 UCRT 副本是另一个已处理问题，不能混为本次前缀异常的证据。
4. 修复后新安装包已重新执行原始启用流程。ignored `target/local-launch/rc003-integration/installed-app-final.log` 在 `2026-09-20T11:10:34.733Z` 记录 `payload_preflight manifest_match=true`，`11:10:34.738Z` 记录 `bootstrap_path simplified=true`，`11:10:45.262Z` 记录 `ipc phase=authenticated`，`11:10:45.982Z` 记录 `capture_attached` 和 `capture_loaded`。独立进程令牌检查确认主程序 `TokenElevation=0`、Helper `TokenElevation=1`。这证明本机安装后的启动路径已修复；此时报告数、边沿数仍为 0，尚待用户实体按键。

## 最小修复与来源

`crates/sayall-windows/src/rc003_input/windows_runtime.rs` 新增 `powershell_source_literal`，在 Rust 向 PowerShell 传递资源目录的边界调用 `dunce::simplified`，再完成单引号字面量转义；不改变清单、文件权限或目标选择规则。日志仅记录是否进行了简化，不输出路径。

参考锁定的 [Tauri 2.11.5 路径插件源码](https://github.com/tauri-apps/tauri/blob/7cd71369c00978a3783b6ae3e9972358abbe4ae6/crates/tauri/src/path/plugin.rs) 中 `resolve_directory`/`resolve` 的做法。版本及提交已由本机 Cargo 包、`Cargo.lock` 和包内 VCS 元数据核对；本次网页读取未成功，不将网页可访问性列为已验证。

[dunce 1.0.5 的公开 API](https://docs.rs/dunce/1.0.5/dunce/fn.simplified.html) 只在可安全转换时返回普通路径，不执行文件 I/O；[同版本实现](https://docs.rs/dunce/1.0.5/src/dunce/lib.rs.html#152-181) 保留不能安全简化的路径。这里没有自行无条件剥离前缀，也没有改为要求用户安装 PowerShell 7。

## 验证与限制

- passed：`cargo test -p sayall-windows rc003_input::windows_runtime::tests --lib` 的既有运行结果为 5 passed、0 failed，证据在 ignored `target/local-launch/rc003-integration/path-tests.log`。新增回归覆盖扩展盘符路径、保留网络 UNC 形式和单引号转义；这些是转换函数测试，不是实际 PowerShell 启动测试。
- failed：上述修复前安装包的增强启动，以及 PowerShell 5.1 扩展前缀最小对照。
- passed：修复后本机已安装包的原始启用流程、路径预检、IPC 认证及 capture 附加/加载，主程序与 Helper 权限边界符合预期。结论仅覆盖本记录的启动路径缺陷。
- 待执行：实体三键、高亮与映射、闲置首用、退出清理及语音回归；附加/加载成功和 0 边沿不能视作三键功能通过。后续结果同步到 [三键集成验证](../Testing/WindowsRc003Input.md)。
- 未知边界：网络 UNC、超长路径、保留名称或其它必须保留扩展语义的路径，尚未执行部署验收。`dunce` 保留这些路径不代表 PowerShell 5.1 会接受它们；本修复不能宣称覆盖所有安装位置或所有 Windows 版本。
- 隐私检查：本记录仅含公开产品版本、脱敏错误和相对证据位置，不含个人路径、设备身份、语音内容、令牌或凭据；本地原始探针文件不随记录提交。
