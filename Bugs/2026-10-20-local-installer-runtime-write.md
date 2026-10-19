# 本地安装辅助组件时拒绝写入运行库

- 发现日期：2026-10-20。
- 状态：调查中，安装未完成。
- 影响范围：本机 0.3.12 本地测试包；不涉及遥控器协议。
- 功能点：NSIS 静默安装及组件完整性核验。
- 现象：可见安装器报告不能写入 `rc003-helper/_internal/VCRUNTIME140.dll`；静默运行后组件仍不完整。
- 正常预期：主程序、96 个 Helper 组件、清单、快捷方式图标及卸载程序全部安装成功后才启动应用。

## 已确认事实

- 构建源：`4e21a746f72abe7f9ba717aa28f655a9081145f4`。Rust release、NSIS 构建及打包前完整 Helper 校验 passed；96 个组件共 152722088 字节，内嵌清单一致。
- 安装器大小 49772171 字节，SHA-256 为 `0D5B9ED3F1C4673716CC8021DB79D260AE634E7442F37A6CB0C7C6B703B35C33`。
- 工具进程与普通 Explorer 启动的进程存在 AppData 文件视图差异。普通用户环境通过文件句柄解析确认实际安装目录，不能以工具视图中的文件代替普通用户安装验收。
- 普通用户安装目录只有 21 个文件。主程序版本为 0.3.12，内容与构建产物仅存在预期的 Tauri `UNK` → `NSS` 三字节标记差异；Helper 清单、快捷方式 ICO 和卸载程序缺失。
- 独立普通用户探针对缺失的 `VCRUNTIME140.dll` 执行不覆盖的 `File.Copy`，实际返回拒绝访问。源文件属性为 Archive；目标不存在，Restart Manager 未报告占用。不能把这个结果描述为已证实的 DLL 文件锁。
- 权限探针实际通过：安装检查进程为 Medium integrity、非 AppContainer、非 restricted token，文件虚拟化未启用。实际安装根目录、Helper 和 `_internal` 目录均由当前用户拥有，当前用户继承 FullControl、无 Deny ACE 或显式强制完整性标签，`FILE_ADD_FILE` 权限检查成功；已有的抽样组件可独占读写打开。缺失运行库并非同名目录，`OPEN_EXISTING` 返回文件不存在。未找到需要修复目录 ACL、完整性级别或文件占用的证据。
- 一次保留进程句柄的观察读到安装器及启动进程退出码均为 0，但启动脚本没有写出完成收据；返回 0 不代表安装完成。正常目录的独立完整性检查明确 failed。
- 未发现能够确认原因的对应崩溃事件。尚不能确定拒绝写入与进程结束的来源，不归因于某个安全软件，也不据此改变系统防护或权限。

## 保留与验证边界

原工具视图中的配置保留，三份配置另有逐文件哈希备份。正常用户配置迁移以完整安装核验为前置条件，本次没有迁移，也没有启动缺件的应用。未强杀应用、重启 Explorer 或操作系统蓝牙。

- `passed`：本地编译、打包前清单校验与失败现场的独立文件检查。
- `failed`：本机静默安装及安装后组件完整性检查。
- `deferred`：新安装版启动、任务栏可见图标、遥控器连接驱动的麦克风切换／恢复与实体听写。

脱敏证据索引：ignored `target/taskbar-icon-20261019/` 下的 `local-build.log`、`helper-package-check.log`、`normal-verify-receipt.json`、`explorer-install-resource-probe-result-v4.json`、`runtime-copy-receipt.json`、`observed-process-exits.json`、`normal-permission-probe.json`。本记录不含个人路径、设备身份、语音内容或凭据。
