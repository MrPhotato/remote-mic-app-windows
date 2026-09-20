# RC003 返回与音量三键：可选过滤驱动候选

日期：2026-09-20。此项仍待加载驱动后的 Windows 真机验收，不能标记功能完成。

## 复用范围

来源和 MIT 许可见 `ATTRIBUTION.md`、`drivers/sayall-hid-filter/LICENSE`。
驱动在 Windows 键盘转换之前，将返回、音量+、音量-分别变成 F15、F13、F14。
SayAll 在确认来源是选中的遥控器后，再还原为原来的语义键，沿用高亮与自定义动作。
普通键盘的 F13–F15 不被 SayAll 钩子吞掉；代理键也不武装其它键的吞键窗口。
语音 F5、ATVV、音频输出和用户配置均不改变。

这是单独的可选 KMDF lower filter，不是应用运行前提，不打进默认 NSIS 安装器。
服务名 `SayAllHidFilter`，ExtensionId `{25b528e7-f8e7-4f92-9d0d-f56458dda41c}`。
唯一绑定条件是上游完整 Hardware ID（含 `REV&00a4`），本机 SetupDi 已确认匹配一个设备。
它是产品/修订标识，不是设备唯一身份，也不足以单独证明不同遥控器型号隔离。
RC001 未测试，不宣称支持通过；不扩大匹配、不安装到其它遥控器。

驱动加载后，即使应用未运行，系统收到的也是 F13–F15。未配置的音量键不会自动
调系统音量；可在应用中显式绑定音量动作。其它软件若已有 F13–F15 快捷键，可能
同时响应这些代理键。这一版没有逐设备吞掉代理键的能力。

## 本机前置事实

- 映射关闭时的独立 Raw Input 实验：Up、误按的 Ok、Up 均各有 DOWN/UP；
  用户测试的返回/音量±没有键盘或 Consumer HID 事件。
- 只读 HID caps：键盘顶层集合；Report 1 为 keyboard page，6/7/8 为 vendor page；
  InputReportByteLength=121。单次打开读句柄失败，Win32=5。
- 这些事实定位到用户态输入交付之前；尚未在本机抓到过滤前的三键原始报告，
  因而不能把上游根因与修复结果直接称为本机通过。
- `scripts/inspect-rc003-filter.ps1` 只读输出通用产品 Hardware IDs、匹配数、
  Secure Boot、testsigning/HVCI 可观察状态、两项目驱动服务注册情况；权限不足为 unknown。
  不输出设备实例路径，不打开 HID 输入，不修改安全设置。
- 初始预检 Secure Boot 为 enabled。驱动未安装，启动测试模式尚未变更；后续本地签名准备见文末记录。

原始临时取证在被 Git 忽略的 `target/local-launch/rc003-input-observer/`；
结构化结论归档在本文件，原始设备身份和用户输入不提交。

## 可重复构建

使用 VS 2022 C++ x64 工具、Spectre 库和 `Component.Microsoft.Windows.DriverKit.BuildTools`。
从微软官方 NuGet 准备下列固定版本到 `target/local-launch/driver-toolchain/packages`：

```powershell
nuget install Microsoft.Windows.WDK.x64 -Version 10.0.26100.6584 -OutputDirectory target/local-launch/driver-toolchain/packages -Source https://api.nuget.org/v3/index.json -NonInteractive
# 此版本默认解析到 SDK CPP / SDK CPP.x64 10.0.26100.1；构建脚本逐项验证固定路径。
./scripts/build-rc003-filter.ps1
./scripts/inspect-rc003-filter.ps1
```

构建脚本不自动下载、不签名、不导入证书、不安装驱动、不修改 BCD。
host C 测试与 WDK 构建启用 `/W4 /WX`，驱动启用 PREfast 和 Spectre。
NuGet x64 工具使用 x64 MSBuild 与 StampInf；不关闭 InfVerif 或静态检查来绕过构建失败。
输出在 `target/sayall-hid-filter/x64/Release/`，包在其 `SayAllHidFilter/` 子目录。
`build-receipt.json` 记录工具版本、构建时来源状态与产物 SHA-256。
构建日志可能含本地路径，留在忽略目录，不提交。

## 安装前的明确边界与回滚

原始构建产物为未签名候选，不能在普通 Secure Boot 环境直接加载；另行准备的本地测试签名包也不等于微软正式签名。
本地测试需自建测试签名、明确授权的独立提权操作，并满足微软测试签名政策；
本机这意味着关闭 Secure Boot、开启 TESTSIGNING 并重启。不要关闭内存完整性来绕行。
正式 Secure Boot 部署需微软认可的内核签名，本次未申请、不发布。
先向用户展示本地构建/预检结果与以下操作范围，再取得其明确选择；等待期间不改系统安全状态。

后续安装流程必须：

1. 验证唯一精确设备匹配、应用正常退出且 BLE 清理完成、无上游过滤驱动冲突。
2. 将测试证书私钥留在本机证书存储，不提交或复制上游证书。SYS/CAT 签名后重新校验
   哈希及签名；只对这一个已审查的 INF 执行提升权限的 PnPUtil 安装，记录准确的 `oemNN.inf`。
3. 重启后确认过滤栈实际加载及无设备问题，再开始功能验收；安装 API 成功不能代替加载与按键成功。
4. 回滚先核对并打印唯一属于 SayAll 的 `oemNN.inf`，再针对它卸载；禁止通配删除驱动包，
   不删除 MiRemoteHidFilter 或其它第三方驱动。驱动卸载、重启恢复与输入验证后，才关闭
   TESTSIGNING、恢复 Secure Boot 并移除这次独有的测试证书。不能反过来先禁止已安装驱动加载。

这是驱动加载的签名要求，不是将重启或重新配对当作日常连接故障的解法。

## 验证记录与剩余门禁

- passed：MSVC host C 改写测试 1102 checks，三键转换、全部其它 usage/report ID、
  释放、重复、短报告、空指针、附加槽以及语音保持不变。
- passed：WDK Release x64、PREfast、InfVerif 构建及 `/w /v` 检查、ApiValidator（Universal）、Inf2Cat 可签名性检查；无编译错误或警告。
- passed：Windows 平台 Rust release 库测试 150 项，0 failed、6 ignored；包括代理键语义、
  实体键盘别名不吞/不武装、重复/迟到释放/重启清理，以及代理音量仍注入而原生音量不重复。
- passed：PowerShell 5.1 预检及 PowerShell 7 连续调用；唯一精确设备匹配，两个过滤服务均未注册。
- passed：2026-09-20 本地测试签名及公钥信任准备（详见下节）；这不表示驱动能在当前启动安全配置下加载。
- deferred：驱动安装、实际加载、卸载回滚及 HVCI/Driver Verifier 稳定性。
- deferred：RC003 三键逐一短按 DOWN/UP 高亮、长按重复、音量同键映射单次注入、
  冷/闲置后首用、断连/睡眠、按住退出/重启、普通键盘 F13–F15 与其它键无回归。
- deferred：RC003 语音快速按下/释放、连续会话及音频到目标应用端到端回归。
- deferred：RC001 独立真机验收。

未通过上述真机门禁前保持 TODO 未完成。

## 2026-09-20 用户授权后的本地测试签名准备

用户在了解 Secure Boot、测试签名、重启及正式签名边界后明确同意试验。
本阶段未修改固件、BCD、HVCI、磁盘加密或安装过滤驱动。

- 提权只读预检：Secure Boot enabled；TESTSIGNING 未显式设置；系统卷全解密、
  ProtectionStatus=0、加密比例 0%，无需暂停 BitLocker；未读取恢复密码或密钥保护器。
  Windows Driver Policy 为 audit、非 enforcement。
- `scripts/prepare-rc003-test-signing.ps1` 实际执行通过：校验原始三文件/来源哈希、
  已审查提交包含关系、完整 REV00a4 匹配；创建一把 CurrentUser/My 不可导出的
  RSA 3072/SHA-256 代码签名私钥，只导出公钥证书；SYS 签名后重建 CAT，再签 CAT。
  脚本仅在目标目录不存在时运行，不覆盖旧包，不隐式导入信任、安装驱动或改启动设置。
- 独立、显式提权 Helper 已将本次公钥加入 LocalMachine Root/TrustedPublisher；
  四项 SignTool 检查均 exit=0：SYS 签名、CAT 签名、SYS 目录成员关系、INF 目录成员关系。
  证书指纹、实际新增存储和签名包哈希保存在忽略目录的回执中，不提交证书或私钥。
- 应用正常退出并收到 `session_cleanup_acked`，蓝牙收尾 128ms。

本地签名包：`target/sayall-hid-filter/local-test-signed/`。
准备时的不可变 `signing-manifest.json` 与后续 `test-trust-receipt.json` 分开保存；
前者的 `trust_imported=false` 描述创建当时的状态，后者记录实际信任与四项验证结果。
完整预检和回执位于 `target/sayall-hid-filter/`，均未上传或发布。

下一步需要用户在 UEFI 中将 Secure Boot 改为 Disabled 并返回 Windows。
随后再次检查实际状态，显式启用 TESTSIGNING，重启后以
`NtQuerySystemInformation(SystemCodeIntegrityInformation)` 的测试签名标志确认运行态，
再安装唯一目标包。BCD 写入成功只代表下次启动配置，不能直接当作运行态已生效。
保留原始 TESTSIGNING 缺省状态，以便卸载后精确恢复。
