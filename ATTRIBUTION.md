# 来源与归属

本仓库是面向 Windows 的 Rust/Tauri 工程。

## RC003 可选三键增强接入（2026-09-20）

- 在下节独立实验成功后，用户明确要求完善软件，授权可选三键 Helper 集成；以 [ADR 0003](docs/decisions/0003-rc003-optional-input-helper.md) 为当前范围。主程序普通权限、基础语音不依赖注入，只读取 RC003 返回/音量±。
- `helpers/rc003-input/guard.py`、`source_binding.js`、`observer.js` 实质改编同一固定上游 `1e6b1d285f9cd50f30c5bc92ac7787a693fc993d` 的宿主定位、来源绑定和报告入口快照。保留 GPLv3 全文于 `helpers/rc003-input/licenses/`，并说明本地三键限制、只观察、不吞写报告、租约、父进程和选择关联等修改；本仓库本身为 GPL-3.0-only。
- 独立 Helper 固定 Frida 17.18.0，Python binding 的 wxWindows Library Licence 与随包第三方许可保留在 Helper 许可目录。使用 [PyInstaller onedir](https://pyinstaller.org/en/stable/operating-mode.html) 打包固定运行环境；不使用管理员 onefile 临时解包执行。hash 锁定构建依赖与包内完整 manifest；主程序内嵌 manifest 摘要，提权后先复制到管理员控制目录并复核文件，再启动载荷。
- Windows 主程序以公开 [ShellExecuteExW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw) 的 runas 启动独立引导进程；Helper 自行核对 loopback 端口所属父 PID、存活句柄以及公开设备 ContainerId 关联。UI 显式启用，不安装服务/驱动或改启动安全设置。
- 引导路径兼容处理参考 [Tauri 2.11.5 的路径插件](https://github.com/tauri-apps/tauri/blob/7cd71369c00978a3783b6ae3e9972358abbe4ae6/crates/tauri/src/path/plugin.rs)（官方 Cargo 包 VCS 提交已核对，MIT/Apache-2.0），使用相同的 [dunce 1.0.5 `simplified`](https://docs.rs/dunce/1.0.5/dunce/fn.simplified.html) API，在 PowerShell 边界仅安全简化扩展盘符路径，不复制路径解析实现。PowerShell 5.1 的前缀失败已最小复现；不能安全简化的 UNC/长路径仍保留，部署能力未知。新包原始流程待验，证据及来源核对边界见 [缺陷记录](Bugs/2026-09-20-rc003-helper-bootstrap-path.md)。
- Rust 新增独立三键状态来源、generation/sequence/epoch 失效机制，复用原有映射和高亮；这些主程序机制自行实现，不复制上游吞键或其它应用注入实现。未配置动作只高亮，Helper 中断先取消手势再释放该来源，原语音生命周期和普通键来源保持原有行为。
- 打包最低系统沿用 Windows 10 1809，仅排除系统 `ucrtbase.dll`，保留 Python、Frida、VCRUNTIME 和 API-set 文件。[微软 UCRT 部署说明](https://learn.microsoft.com/en-us/cpp/windows/universal-crt-deployment?view=msvc-170)明确 Windows 10/11 始终使用系统 UCRT；[PyInstaller 6.19.0 官方依赖选择实现](https://github.com/pyinstaller/pyinstaller/blob/v6.19.0/PyInstaller/depend/dylib.py)也说明仅面向 Windows 10+ 时无需附带这类库。本机初包对此 DLL 的 Rust 复制返回 `os error 5`，安装后完整性检查也发现该副本缺失；只记录观察到的现象，不猜测 Windows 拒绝的具体机制。按系统支持范围移除冗余副本后重新生成清单和完整包验证。

## RC003 Frida 独立诊断例外（2026-09-20）

- 用户在明确获知管理员权限、向 `WUDFHost.exe` 注入监听代码和非公开输入接口的边界后，授权一次独立实验；不代表授权接入正式产品、修改驱动或启动安全设置。实验方案见 [Testing/WindowsRc003Frida.md](Testing/WindowsRc003Frida.md)。
- 参考并实质改编 [ZSTDJan/windows-remote-mic-app](https://github.com/ZSTDJan/windows-remote-mic-app/tree/1e6b1d285f9cd50f30c5bc92ac7787a693fc993d) 固定提交 `1e6b1d285f9cd50f30c5bc92ac7787a693fc993d` 的 `apps/windows/rc003/src/ovb_rc003/frida_hid_tap_runtime.py`、`frida_hid_tap_injector.py`：注册表 HostPid 定位、宿主独占性检查、`NtDeviceIoControlFile` 的 `0x80018483`/8 字节 metadata/9 字节 Report 1 识别及入口快照。该提交根目录 `LICENSE.md` 是 **GPLv3**，不是 MIT；来源与许可副本仅存本地 ignored 实验目录，不将其代码或第三方二进制接入或分发到产品。
- 去掉上游吞键、清零、映射、套接字协议和持久 Gadget 注入流程。使用官方 [Frida Injected 模式](https://frida.re/docs/modes/#injected) 的 Python binding `17.15.3`，仅对唯一目标对应的宿主直接 attach；本机宿主另有一个 BLE HID 实例，因此复用上游 `DeviceIoControl` 活动调用帧、UMDF 设备对象与注册表 ContainerId 来源验证，在匹配选中容器之前不读取报告，不允许独占回退。白名单事件只用于验证可见性。所谓只观察指不写设备报告，Frida hook 本身仍临时修改目标进程代码，来源验证也读取了非公开 UMDF 实现。
- 依据 [Frida Interceptor API](https://frida.re/docs/javascript-api/#interceptor) 与固定版本 Python binding 的 `Cancellable`、`Script.unload`、`Session.detach` 实现有界调用、租约到期解钩和正常退出清理。入口与返回快照不当作两次物理输入，也不作为真实硬件延迟证据。之前调研中的“Frida IOCTL 无捕获”仅是当时尝试结果，不能覆盖本次不同入口快照实现或证明纯软件不可能。
- 本机 `17.15.3` direct attach 两次报 `ProcessNotRespondingError`，宿主存活；切换隔离的官方 `17.18.0` 后 attach/load/hook_ready 通过。该版 [官方说明](https://frida.re/news/2026/09/09/frida-17-18-0-released/) 与 [ACL 修复提交](https://github.com/frida/frida-core/commit/65e713c76202a9266b13061245c302be25c8bb03) 给 Frida 自己的临时目录/文件增加 LOCAL SERVICE 读执行权限，并完善自身管道权限，不修改设备、目标进程 ACL 或系统安全策略；版本对照与修复方向吻合，不能单凭对照认定唯一根因。上游 Gadget 本来就为自身文件授予 LOCAL SERVICE 读执行，因此旧 binding 失败不等于 Gadget 路线不通。

## RC003 GameInput 免驱接口实验（2026-09-20）

- 依据微软 [GameInput 3.4 公告](https://developer.microsoft.com/en-us/games/articles/2026/05/gameinput-update-now-available/) 的 raw HID 新能力，使用官方 [Microsoft.GameInput 3.5.274](https://www.nuget.org/packages/Microsoft.GameInput/3.5.274) 固定包进行独立诊断。该包 README 声明 3.5 支持应用目录并排部署；包内 `native/src/GameInput.cpp` 实际包含应用目录加载分支，不能用落后的 GitHub main loader 推断不支持。
- 本地只读查询 MSI 数据库、读取内嵌 CAB 并解包；未执行 MSI 安装或行政安装序列。只将微软签名的 x64 `GameInputRedist.dll` 放在探针目录，由同包 `GameInput.lib` 加载；未升级系统已安装的 3.3.221.0 运行库/服务，未安装输入驱动或改变启动设置。不在仓库提交第三方二进制。
- 探针自行编写，仅参考公开 [设备回调](https://learn.microsoft.com/en-us/gaming/gdk/docs/reference/input/gameinput/interfaces/igameinput/methods/igameinput_registerdevicecallback)、[设备信息](https://learn.microsoft.com/en-us/gaming/gdk/docs/reference/input/gameinput/structs/gameinputdeviceinfo)、[原始报告读取](https://learn.microsoft.com/en-us/gaming/gdk/docs/reference/input/gameinput/interfaces/igameinputrawdevicereport/methods/igameinputrawdevicereport_getrawdata)；没有复制竞品输入实现。包内官方头文件/静态库按其 MIT 许可仅用于本地构建。
- 仅对唯一匹配的遥控器读取白名单按键状态，拒绝聚合设备和多匹配；不记录设备身份、其它键盘输入、语音数据，不注入或吞键。实际结论及边界见 [Testing/WindowsRc003GameInput.md](Testing/WindowsRc003GameInput.md)。

## RC003 三键可选 HID 过滤驱动（2026-09-20）

- 实质改编 [QL-4/RemoteMapper](https://github.com/QL-4/RemoteMapper/tree/be8b57330c26a70d8b8ec9ff1e60c23251a2fc31/driver/MiRemoteHidFilter)，固定提交 `be8b57330c26a70d8b8ec9ff1e60c23251a2fc31` 的 `driver/MiRemoteHidFilter/driver.c`、`driver.h`、`remap.c`、`remap.h`、INF 和 vcxproj；对应本仓库 `drivers/sayall-hid-filter/`。MIT，Copyright (c) 2026 QL-4；完整许可保留在该目录 `LICENSE`，随本地驱动包附带。不复用第三方二进制、证书、私钥或安装/卸载脚本。
- 沿用 IRP_MJ_READ 转发、下层完成后原地等长改写 Report ID 1 首槽 `report[3]` 的做法。上游八键缩减为三键：usage `80→68`（F13/音量+）、`81→69`（F14/音量-）、`F1→6A`（F15/返回）。F5 语音、其它按键、释放、其它槽和 vendor reports 保持不变。新的服务名/ExtensionId 避免与原项目混用，增加失败路径和匿名 ETW 聚合计数。
- 上游 [三键修复记录](https://github.com/QL-4/RemoteMapper/blob/cf89615487efcfcf4ff3f78e9bfc3b9bd69597ad/NOTES.md) 是复用依据；上游 Windows 11/HVCI 实测不等于本仓库真机通过。本机关闭映射后的独立 Raw Input 实验有 Up/Ok 正对照，返回/音量±均未收到；SetupDi 读取的 Hardware IDs 包含上游精确 `REV&00a4` 匹配。因此保留该匹配，不扩大到 VID-only 或键盘类过滤器。该产品 ID 不能独立证明 RC001 型号隔离，RC001 仍未验收。
- 应用端别名仅在已选择的遥控器设备归因后解码；不进入无设备身份的全局钩子解码/武装，避免误吞普通键盘 F13–F15。代理音量不能当作已交付原生音量，仍执行用户配置的动作。未配置动作不新增隐式音量行为。
- 构建采用微软官方 [WDK NuGet](https://learn.microsoft.com/en-us/windows-hardware/drivers/install-the-wdk-using-nuget) 与 [Windows-driver-samples 的包导入方式](https://github.com/microsoft/Windows-driver-samples/blob/main/Directory.Build.props)（2026-09-20 查阅，仅参考属性导入方式，不复制示例实现）；本地锁定 WDK `10.0.26100.6584`、SDK CPP `10.0.26100.1`、VS 2022。驱动仅为可选增强轨；普通用户主程序、基础语音路径不依赖它。不引入 Frida、虚拟 HID 或私有协议。
- 验证与签名/安装边界见 [Testing/WindowsRc003Filter.md](Testing/WindowsRc003Filter.md)。2026-09-20 用户明确同意本地试验后，另行生成测试签名包，尚未修改 Secure Boot/BCD、安装内核驱动或发布。
- 本地测试签名准备按微软公开 [New-SelfSignedCertificate](https://learn.microsoft.com/en-us/powershell/module/pki/new-selfsignedcertificate)、[测试证书安装](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/installing-test-certificates)、[测试签名验证](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/verifying-the-test-signature) 实现：一把本机不可导出的代码签名私钥，SYS→Inf2Cat→CAT 顺序，精确公钥信任及目录成员验证。只参考公开工具行为，无外部实现复制。启动模式与当前运行态按 [TESTSIGNING 文档](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/the-testsigning-boot-configuration-option) 和 [NtQuerySystemInformation 文档](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntquerysysteminformation) 分开验证。

## 鼠标动作扩展

- 鼠标单击/双击参考 AutoHotkey v2 Click 的成对按下/释放行为，不复制其代码或引入依赖；通过 Windows SendInput 单批发送 2/4 个边沿，部分提交时补发释放，不新设双击等待常量。参考： https://www.autohotkey.com/docs/v2/lib/Click.htm 。
- 鼠标移动使用 Microsoft GetPhysicalCursorPos / SetPhysicalCursorPos；本机 150% 缩放实测发现 DPI-unaware 调用的 37 单位会变成约 56 物理像素，因此为该调用显式设置线程级 PER_MONITOR_AWARE_V2，并用 RAII 恢复原线程上下文。修正后右/左 37、下/上 53 物理像素均通过。参考： https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext 。
- 滚轮动作参考 AutoHotkey v2 的 WheelUp/WheelDown 动作粒度，仅参考行为，不复制实现或依赖 AutoHotkey。来源：`https://github.com/AutoHotkey/AutoHotkeyDocs/blob/v2/docs/lib/Send.htm`。
- 滚轮使用 Microsoft 公开 SendInput / MOUSEINPUT API：INPUT_MOUSE + MOUSEEVENTF_WHEEL，mouseData 是带符号的滚轮位移；一个刻度为 WHEEL_DELTA（120）。来源：`https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-mouseinput`。动作是用户可选配置，不绑定固定遥控器按键、不修改默认配置。测试和首按边界见 `Testing/WindowsMouseActions.md`。

## Windows 注册应用扩展

- 应用发现使用 Microsoft AppsFolder / IShellItem / BHID_EnumItems，启动使用 ShellExecuteExW + SEE_MASK_NOASYNC；只读取系统公开注册的可启动项，不扫描第三方私有文件或修改 Windows 注册。按本机缓存的 Microsoft windows-rs 0.62.2 API 签名核对实现；没有复制外部算法。参考： https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid 、https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-shellexecuteinfow 。
- 应用库仅保存在用户确认后的按键配置中；扫描不是启动，多选添加不是绑定。日志只记录数量、阶段和耗时，不记录应用身份或个人路径。验收方法见 `Testing/WindowsRegisteredApps.md`。

## 遥控器缓存电量显示

- Microsoft 公开 Configuration Manager API `CM_Get_Device_ID_List_SizeW` / `CM_Get_Device_ID_ListW` / `CM_Locate_DevNodeW` / `CM_Get_DevNode_PropertyW`：只枚举当前存在的 BTHLE 设备，按连接所选对端的完整地址组件匹配唯一节点，读取 OS 设备属性。官方文档：`https://learn.microsoft.com/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_get_devnode_propertyw`。标准 `System.Devices.BatteryLife` / PKEY_Devices_BatteryLife 的 GUID/PID/type 由本机 Windows SDK 10.0.22621.0 `propkey.h` 核对。
- `Gronsten/razer-tray`，提交 `8e7e395417023bf2446779a4c5237716183da69f`，`src/DeviceMonitor.cpp`：参考其使用公开 Configuration Manager API 读取 Windows Bluetooth 电量缓存属性 `{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2` 的路径和未知值语义；未复制代码、无运行时依赖。该键不是微软承诺跨版本稳定的标准 BatteryLife 属性，故仅作可失败的兼容读取，严格检查 BYTE、长度为 1、0..100；缺失/异常保持未知。
- 不访问注册表，不读取第三方 App 数据，不使用设备管理写入 API，不另开 BLE/GATT 会话。独立后台线程每 60 秒查询一次系统缓存，不代表遥控器每 60 秒上报新电量；界面提示缓存来源。连接纪元隔离迟到结果，断连/睡眠后停止监视并隐藏旧值；可选电量功能不影响语音错误状态。详见 `Testing/WindowsBattery.md`。

## 治理规范迁移

- `HD838A/remote-mic-app`，提交 `b233a88cc4457b00413dda6b37ec8b4af12c5121`：迁移其平台无关的分支/提交纪律、日志脱敏与完整链路记录、Bug 复现取证顺序、测试手册要求、发布来源可追溯和资产不可变原则；本仓库将其改写为 Windows/RC001/RC003、Tauri/NSIS、updater minisign 与 Authenticode 边界。
- 有意排除：Swift/SwiftPM、CoreBluetooth、AppKit/SwiftUI、Developer ID/Apple 公证、Sparkle、DMG/PKG、Apple Team ID、macOS/iOS/Web 专属流程，以及任何 macOS 私有路径或凭据。
- 迁移文档：`LOGGING.md`、`RELEASING.md`、`TECHNICAL.md`、`TROUBLESHOOTING.md`、`Bugs/README.md` 与 `Testing/WindowsRelease*.md`。这些文件记录的是规范与经验，不复制参考仓库业务代码。

## App Logo 版权

- App Logo 与 App Icon 沿用 `HD838A/remote-mic-app` 的版权边界：属于 HD838A 保留版权的专有品牌资产，不纳入 GPL-3.0-only；Windows 版适用范围和授权条件见 [LOGO-LICENSE.md](LOGO-LICENSE.md)。

## 产品与 UI 基准

- `HD838A/remote-mic-app`：无线麦 macOS 原版的信息架构、产品文案、RC003 图片、RC001/RC003 型号识别、ATVV 行为和测试边界；RC001 支持参考提交 `b233a88cc4457b00413dda6b37ec8b4af12c5121`。
  - 2026-09-05 按键映射功能移植补充（均为语义移植，非代码复制）：`RemoteButtonGestureRecognizer` + `HIDRemoteScheduler` 的手势参数（双击窗口 300ms、长按 550ms、连发起始 350ms、返回 50ms/方向与音量 100ms 连发）与"按配置动态启用双击/长按识别、未配置时单击零延迟"的语义；`KeyboardEventSuppressor` 的预测式武装 + 有限窗口匹配吞键模型；`RemoteMappingCanvas` 的按键卡片布局表（锚点/目标 Y 坐标逐键移植）与三态高亮（按下=橙、选中=强调、普通=中性）；`MappingSelectionPolicy` 的"锁定当前按键"默认值。Mac 版 `KeyboardEventSuppressor` 的 UP 沿无配对兜底（DOWN 泄漏+UP 吞下=粘键缺陷）未移植——Windows 版沿用本仓库 2026-09-05 规则（DOWN 漏进 OS 则 UP 必放行）。
  - 2026-09-09 按键映射配置导入导出补充（本地 Mac 仓库 HEAD `feba1d6`，语义参考，未复制代码）：参考 `AppSettings.exportedConfigurationData/importConfiguration` 的版本化 JSON、导入前完整解码校验与一次性应用，以及 `SettingsView.exportConfiguration/importConfiguration` 的系统文件选择器、用户取消静默、成功/失败反馈。Windows 版仅迁移按键映射，不导入 Mac 专属设置或统计；格式使用独立 `formatVersion: 1` + `buttonMappings` 契约，不宣称与 Mac 配置文件互通。
- RC003 图片 SHA-256：`658d9333853958c13ff721eb76e1a6816c1dbea16006a84e8577ad410812549f`。

## Windows 行为与测试参考

- **登录时自动启动（2026-09-14）**：产品行为参考 macOS 仓库
  `LoginItemService.swift` 的“读取系统状态 → 注册/取消 → 失败反馈”模式；Windows
  不移植 `SMAppService`，改用微软公开的当前用户登录启动项
  `HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run`，只写本应用值且不需管理员权限。
  设置默认关闭，应用启动时以持久化偏好同步系统状态，失败只记录结构化日志、不阻断启动。

- **LL 吞键对 Raw Input 交付影响的本机实证（2026-09-05，`docs/investigations/2026-09-05-ll-swallow-vs-raw-input.md`）**：双线程探针（钩子线程 + Raw Input INPUTSINK 线程分离，key_suppressor 同构）两轮一致证实 **WH_KEYBOARD_LL 返回 1 吞掉的键盘事件不会再投递 WM_INPUT**——按键映射门控（`key_gate.rs`）据此采用"被吞键盘边沿由钩子线程直接喂引擎 + 监听器喂 HID 报文与透传键盘事件"双源合并架构；HID 报文归因武装 + 60ms 有界等待沿用 key_suppressor 实证参数。

- `HD838A/remote-mic-app#249`，提交 `090a3cfc24f0e3e733b2347ee2daf87c60e10097`：Windows 独立实现、ATVV 测试夹具、语音边沿、安装升级、公开边界和 Mac 风格 UI 原型；Raw Input 参考了 `hid_identity.py` 与 `raw_input_windows.py`，SendInput 的批量提交、物理修饰键和失败回滚参考了 `win32_input.py` 与 `win32_keys.py`，均以 Rust/windows-rs 重新实现。
- `GetSayAll/hardware-simulation`，提交 `65248499cac7da3ad46cd0c11dca1478f7733255`：RC001 短语音时间线的控制通知、40 + 80 字节音频拆包和停止通知；本仓库只保留纯 ATVV 回放所需字段。
- `ZSTDJan/windows-remote-mic-app`：WinRT BLE、Raw Input、音频输出、发布门禁和真实硬件验证边界；其语音页按语音程序配置"按住说话快捷键"、按下注入 DOWN/松开释放的行为，是本仓库按住说话快捷键设置的产品参考。Round 1 拆解曾记两项技巧参考，后续实证修正（Round 2/3）：**physicalize 技巧——结构性无效（勿模仿）**：`legacy_key_suppressor_windows.py` L142-155 的做法（仅对自家 keybd_event 注入的带 "RMICRC03" 标记右 Alt，在自家钩子的私有副本上清 INJECTED 标志→转发→恢复）曾被解读为"使下游应用钩子视为物理键，前提是自家钩子位于目标应用钩子之前（链头）"——该解读不成立（Round 2 E 三层实证：LL 钩子每钩子收到私有结构副本，修改不跨钩子传播，CallNextHookEx 转发通道不存在，应用层收到原始键；Round 3 J 语义复查：清标志对下游钩子/应用层均不可见，且 ZSTDJan 进程内也无读者——对声明目标是 no-op；其真正能影响豆包读值的是 `doubao_rpc.py` 的 Frida 版 attach 方案，未接线进生产流程，违反本仓库 A2/A4/A5 边界，仅作机理记录）。**WeType 语音触发配方——本机实证有效（Round 3 J 翻案）**：SendInput 注入 Ctrl+Win 按住（纯 wVk 或扫描码配方均可）可唤起 WeType 语音（会话级 TSF 激活前提下：开麦/吞键/释放关麦全链实证，注入 ground truth 由常驻捕获器独立记录）；**Round 2 F 曾判"三配方无反应"，系其 TSF 激活用了线程级 flags（dwFlags=0，会话级应为 TF_IPPMF_FORSESSION=0x20000000）、WeType 从未真正激活所致——教训：测试 IME 行为前必须以会话级激活 + 行为判据（候选框版式）双重确认活动输入法**。**配方形态约束（2026-09-04 P 实证，evidence/p）：和弦必须逐事件注入且两键间隔 ≥80ms——WeType 拒绝单次 SendInput 批量零间隔提交的 Ctrl+Win（sent=2/2 全到达仍无吞键无开麦；逐事件 80ms 两轮 2/2 触发，A 失败→B 通过→A 失败→B 通过交替序列排除状态漂移）**；应用曾把该配方误合并为单批零间隔导致真机不出字（Bugs\2026-09-04-wetype-zero-gap-injection.md，含第二层缺陷：遥控器 F5 须由抑制器吞掉，否则"额外按键"拒绝；钩子链头 bump 加固同日落地），已修复并 RC001 真机端到端 passed（2026-09-04，用户确认文字上屏）。
- `richlearntodo-debug/vibe-flow`，提交 `047f9d3ead54bf30de9b884adf8f7b5adefe9993`：自然 ATVV 会话、WASAPI 音频生命周期和硬件验收清单。
  - **Windows 深色模式专项调研补充（2026-09-08，本地参考库 HEAD `b47f7cdce8b753fade0c64c97332bebe80f17d2d`；主应用 UI 源码未开源，依据为 `docs/ARCHITECTURE.md`、`docs/PRODUCT_AUDIT_2026-09-01_ZH.md` 与用户指南）**：其产品支持浅色、深色、跟随 Windows 三档且运行中切换不重启 Host/Bridge/Capture；审计结论要求深色采用低饱和中性色层级，并完成各页实际截图检查。本仓库只借鉴“三档主题、主题是纯显示行为、不得重启后台服务”和视觉验收边界，不复制实现；SayAll 将选择器放在“关于”页面，并通过自身 `SettingsStore` 持久化，详见 `docs/plan/2026-09-08-windows-dark-mode.md`。
  - **按键映射/双响应专项调研补充（2026-09-07，本地参考库 `Documents\Codex\reference-repos\vibe-flow`，HEAD `b47f7cdce8b753fade0c64c97332bebe80f17d2d`；主应用源码未开源，依据为其文档 + `scripts/VoxDeckInputBridge.cs` + `driver/rc003-filter`）**：
    1. **用户态"拦截↔设备身份互斥"独立复证**：其 V1.3 根因报告（`docs/V1_3_INPUT_ROUTING_ROOT_CAUSE_ZH.md`）实测——LL 钩子拦截 → Windows 不投递对应 WM_INPUT → Raw Input 拿不到 RC003 设备身份 → 动作永不执行（旧候选日志：钩子暂存 166 边沿/真正到达 Raw Input 4/配对 0/实体路由 0）。与本仓库 2026-09-05 `ll-swallow-vs-raw-input` 实证同结论，两库独立互证；本仓库以 GATT 前信号（0x04 早于 HID 60-90ms）做武装归因，不受此陷阱影响，为同类实现中结构更优。
    2. **其用户态发布版（V1.5）的答案=接受共存**：钩子对非语音映射键一律放行（不拦截、也不暂存回放），Raw Input（INPUTSINK + 设备句柄指纹）负责设备归因与动作执行，明确接受"遥控器原始键系统效果与配置动作同时发生"，文档要求"不应在 UI 或发布说明中描述为精确拦截"；默认 Profile 全部映射=该键原生效果（上→上、确认→Enter）使共存不可见，非默认 Profile（如左→browserback、上→Ctrl+Z）实际存在与本仓库同款的"原生+注入"双响应。语音键（RC003 固件形态=F5）是唯一在钩子层无条件按 VK 抑制的键（物理键盘 F5 冲突被接受，或交由驱动路径解决）。
    3. **唯一彻底解=KMDF per-device upper filter（候选未发布）**：INF 精确绑定 VID 0x2717/PID 0x32B8（不做键盘类过滤器，普通键盘零影响），按扫描码位图抑制 + 全边沿环形队列入队上抛用户态；250ms 心跳、2s 超时 fail-open 全放行、策略 generation 变更清队列防陈旧事件、控制句柄关闭即解除抑制。与本项目 ADR 0002/Helper 轨定位同构；其 `driver/rc003-filter/README.md` 的 10 项发布门禁（SDV/HLK/微软签名/Secure Boot+内存完整性/卸载回滚/万包压测）可作 Helper 轨验收清单参考。
    4. **已退役路线警示**：独占 GATT 抢占 HID 服务/强制禁用 HID 子设备 → Windows 将键盘子设备判为 critical，`/force` 禁用成 reboot-pending 状态而非安全热交接——本仓库 GATT 归因为并行订阅（不禁用系统 HID 栈），勿走独占抢占路线。
    5. **互证数据点**：RC003 返回/音量±/电源键在钩子/键盘 Raw Input/Consumer Raw Input 全通道不可见（HID GATT 0x1812 特征 AccessDenied；厂商服务 8a7a0001-… 的 Notify 无按键事件；Frida 旁路 WUDFHost 监听 IOCTL 无捕获）→ 其结论"硬件能力缺失给诊断、不宣称映射成功"，与本仓库 RC003 返回/音量±格子禁用同构；WeType 配方 Ctrl+Win/toggle/80ms、语音键=F5、"重连后扫描码偶变→持久语音映射保持权威"均与本仓库实测一致或互补。
    6. **工程细节参考**：RawKeyboardEdgeTracker（keysDown 集合按扫描码身份 add/remove，防钩子/Raw Input/驱动多源双触发）；长按 650ms、连发起始 420ms/间隔 80ms；TV=Win+Tab 任务视图且"方向键仅任务视图激活期间执行映射动作"（拥抱原生效果而非对抗）；动作执行回执（真实 SendInput 结果而非排队即成功）。
- `mwlt/Voice_VibeCoding`，提交 `c89410aed3b274fee5e571128b82c9c6e6689715`：Rust/Tauri 模块划分、windows-rs API、音频生命周期和托盘窗口工程经验；其语音键按住注入的 Hold 语义（按下先快捷键 DOWN、松手统一释放、SendInput 互斥降级）是本仓库按住说话快捷键注入时序的参考。Round 1 拆解补充其 **LL 钩子吞键工程细节**（本仓库吞键层设计的参考，非逐行复用）：时序窗吞键（音量 recent 200ms、back/home/menu/tv/power 250ms、方向/OK 200ms 或 tap_ready+自定义位图）、钩子链头 bump（重叠安装：先挂新钩再卸旧钩，消除 LL 吞键空窗）、F5 语音键状态机（sticky/correlate 120ms/tail 3s；DOWN 漏进 OS 则 UP 必放行，防粘键）、音量防双格（Tap 转发 + SendInput VK_VOLUME_* + 200ms 吞固件残留）、Alt 和弦用 SendMessageTimeoutW 直发前台避免系统菜单、自家注入放行（EXTRA_INFO 标记或 INJECTED→CallNextHookEx），及 bump 空窗/sticky 粘键/60ms 去抖门等已踩坑清单。本仓库只使用 SendInput 公共 API，不引入其 WinUHid 虚拟键盘驱动。
- `cgutman/WinUHid`（MIT 许可）：用户态 UMDF 虚拟 HID 键盘/鼠标驱动框架（C++/Win32），无预编译 Release，需自建并签名后使用；ADR 0002 增强轨驱动来源的第一候选（须先审计）。签名成本调研结论（**已闭合，2026-09-04**：UMDF 分发不需硬件计划/EV，OV 级 catalog 签名为最低门槛——三层官方原文支撑，`docs\investigations\evidence\g\signing-policy.md`；残余含混=无单句官方原文直书此结论，装机实测 deferred（调查护栏限制））记录于 `docs\investigations\2026-09-04-avoid-driver-signing-input-paths.md`。未经审计的 WinUHid 二进制不进入仓库。
- `QL-4/RemoteMapper`，main 提交 `25ca0c13cf2ff2caf7caae3d9690f9629b7c0df0`（另有 `driverless-keymap` 分支）：小米蓝牙遥控器 → 微信输入法（WeType）语音录入的完整端到端先例——按住语音键唤起 WeType 录入并送入音频，松开结束录入并恢复系统原默认麦克风。可借鉴结论：(1) **双分支分层**：`main` 含 KMDF HID lower filter（需 TESTSIGNING），`driverless-keymap` 无驱动直接交付、但缺返回/音量±三个键（被 kbdhid.sys 丢弃）且 LL 钩子映射会误吞物理键盘同名键——印证本仓库"基础路径免驱动 + 增强轨驱动"分层与"LL 钩子无来源设备 ID"的既有判断；(2) **MiRemoteHidFilter 驱动做法**：extension INF 精确绑定 VID 0x2717 / PID 0x32B8（不匹配其他键盘），修复 kbdhid.sys 丢弃的 usage 0x80/0x81/0xF1，并把普通键改写为 F13–F19、语音键 HID F5 改写为 F20，从源头规避误吞物理键盘同名键；实现为转发 `IRP_MJ_READ`、下层完成后原地等长改写 Report ID 0x01 的 `report[3]`（实测报告格式 `01 00 00 <usage> 00 ...`，report[1]=modifiers、report[2]=reserved），不改 Report Descriptor / Report ID / 报告长度；HVCI 开启下 Windows 11 x64 八键验收通过（KMDF 1.15 + WDK 10.0.26100，过 PREfast/InfVerif/ApiValidator/Inf2Cat）；其"KMDF 正式发布需 Hardware Dev Center attestation/WHCP、UMDF 2 迁移未实现"的结论与本仓库 ADR 0002 签名成本结论互证；(3) **VB-Cable 音频路径**：遥控器音频经 CABLE Input/Output 转发、临时切换系统默认录音设备喂给 WeType——依赖第三方虚拟声卡驱动和默认设备切换，违反本仓库基础路径边界，仅作增强轨/目标 App 适配参考。排除项：其语音键支持单击/双击/长按配置，违反本仓库"语音键只支持按下开始、释放结束"规则，语音键不借鉴；`keymap.json` + 托盘双击映射面板（单击/双击/长按可配、保存即生效、旧 `keymap.txt` 自动迁移）可作普通键映射产品化参考。
- `wasapi-rs` 0.24.0：MIT 许可的 Windows Core Audio 安全封装，用于端点枚举、共享模式渲染与 padding 查询。
- **CABLE Input 端点静音自愈（2026-09-07）**：依据 Microsoft Core Audio `IAudioEndpointVolume` / Endpoint Volume Controls 公共 API（`learn.microsoft.com/windows/win32/api/endpointvolume/nn-endpointvolume-iaudioendpointvolume`、`learn.microsoft.com/windows/win32/coreaudio/endpoint-volume-controls`），共享模式端点的主静音属于端点级状态，不是应用 WASAPI 写入成功即可证明可听。本仓库仅对名称确认的 VB-CABLE 渲染端点在打开时及每次语音会话开始前调用 `GetMute` → 必要时 `SetMute(FALSE)` → `GetMute` 读回确认；不修改物理输出设备，也不覆盖用户音量标量。调用结果、检查点和耗时写入结构化 GATT 诊断日志。
- **SayAll 会话静音自愈（2026-09-07）**：用户现场观察到音量合成器左侧 CABLE Input 端点未静音，但右侧“无线麦 SayAll”应用会话在开始推流后很快重新静音。依据 Microsoft `IAudioClient::Initialize` 文档，渲染会话默认会跨应用重启持久化音量与静音状态；依据 `ISimpleAudioVolume::GetMute/SetMute`，应用会话静音独立于端点主静音。实现使用 `IAudioSessionManager2::GetSessionEnumerator` + `IAudioSessionControl2::GetProcessId`，只锁定当前 SayAll 进程在用户已选 CABLE 端点上的会话；初始化、语音会话开始、`IAudioClient::Start` 后读回，并在推流期间每 100ms 低频检查，发现静音才解除，不修改会话音量、不碰系统声音或其他进程。初始化时另以 `IAudioSessionControl2::SetDuckingPreference(TRUE)` 让 SayAll 会话退出 Windows 默认通信自动压低机制；该预防措施不作为外部静音来源已经归因的证据。官方依据：`learn.microsoft.com/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize`、`learn.microsoft.com/windows/win32/api/audioclient/nf-audioclient-isimpleaudiovolume-setmute`、`learn.microsoft.com/windows/win32/api/audiopolicy/nf-audiopolicy-iaudiosessionmanager2-getsessionenumerator`、`learn.microsoft.com/windows/win32/api/audiopolicy/nf-audiopolicy-iaudiosessioncontrol2-getprocessid`、`learn.microsoft.com/windows/win32/api/audiopolicy/nf-audiopolicy-iaudiosessioncontrol2-setduckingpreference`。

## 延迟调研来源（2026-09-05，语音键按下→电平图出现优化专项）

按仓库规则（实现前先调研），本专项调研结论与边界记录如下；对应实测见 `docs/investigations/evidence/p/FINDINGS.md`（端点预热对照实验）：

- **业界 PTT"按下→开麦"模式**：可查证的主流实现均为"音频链路常驻 + 按键只做门控"（Mumble 持续采集+传输模式门控 `mumble.info/documentation/user/audio-settings/`；Zoom 会议内按住空格解除静音 `support.zoom.com` KB0063250；Discord PTT Release Delay 滑杆，页面被反爬，引自搜索摘要）。本仓库渲染端点常驻打开（`audio.rs` SelectEndpoint 打开后跨会话复用）与此一致。
- **WASAPI 冷启动与端点电源**：微软 PortCls 文档——音频设备空闲（示例 1s）进入 D3，恢复 D0 规格要求 ≤35ms/≤300ms（`learn.microsoft.com/windows-hardware/design/device-experiences/audio-subsystem-power-management-for-modern-standby-platforms`）；JUCE 论坛实测 WASAPI 设备冷创建 2-3s、Initialize 数百 ms（`forum.juce.com/t/wasapi-2-3s-delays-on-creating-audio-devices/54971`）；StackOverflow `IAudioClient::Start` 通常 5-6ms（被 Cloudflare 拦截，引自摘要）。**"跨进程保温端点让第三方 Initialize 更快"无公开量化先例**——本仓库已用持锁对照实验自行量化：对 WeType 开麦延迟无效（冷/热中位数差 0.3ms，evidence/p，2026-09-05），该方向就此关闭。
- **WeType/微信输入法语音快捷键形态**：默认按住 Ctrl+Win（微信电脑版 4.1.7+ 同款，可于微信"设置→快捷键"自定义；新浪财经/光明网/callmysoft 报道）；社区帖（linux.do/t/topic/2409202，2026-06-15，早于 2.1.3，引自搜索摘要）称 WeType 语音快捷键"必须以 Ctrl/Alt/Shift 开头，不能设独立单键"——**待 2.1.3 真机复核**；ghxi 评论区提到"单击 Ctrl 触发"模式（懒加载未复核）。讯飞输入法 PC 版默认 F6 单键+长按说话（pconline/3DM/ghxi 教程）——竞品基线，未实测其延迟。
- **竞品/社区对"面板出现延迟"的讨论**：未找到任何量化"按下→微信电平图出现"的公开评测（横评均测识别速度/准确率）；游戏侧有 PTT 激活延迟 1s-5s 的社区案例（Overwatch 官方论坛、Valorant Reddit），第三方全局钩子（如 Razer Synapse）可使 PTT 延迟 3-5s——排查本机钩子干扰的依据。
- **本专项实测结论（evidence/p，2026-09-05）**：注入→WeType 开麦（ConsentStore 精确 FILETIME 判据）稳定 ~163ms（13 试验 ±5ms），端点预热无效；两型号遥控器实际均直接 0x04 开始推流（历史 GATT 日志 0x08 计数为 0，无可并行的开麦往返）；0x04 通知早于 HID F5 键盘事件 60-90ms 到达（evidence/p 2026-09-04 取证），当前"0x04 到达即注入"已是链路最早合法触发点。剩余 ~215-245ms = BLE/固件（~30-60ms）+ 和弦间隔（20ms）+ WeType 内部处理（~163ms，外部不可合法压缩）。
- **macOS 版输入目的地/输入源设计（HD838A/remote-mic-app，本机 clone `Documents\Codex\remote-mic-app`）**：`VoiceInputDestinationCoordinator.swift`——语音触发由"聚焦目的地就绪"门控（AX 系统级聚焦快照：role ∈ {AXTextArea/AXTextField/AXComboBox} + enabled + editable + 非保护内容 + 语义文本不含 password/search/设置 等敏感词；不就绪 UI 提示等待/不可用，5s 超时不注入）；`PreferredInputSourceMonitor`——保证配置的语音工具是活动输入源。**Windows 版 IME 专项（2026-09-05）借鉴其输入源职责**：实测 WeType 语音热键仅在自身为会话活动输入法时生效（微软拼音活跃 2/2 不触发、切回 2/2 恢复、激活后零延迟注入 3/3 触发，evidence/p），已实现 `ime.rs`（TSF `ActivateProfile` + `TF_IPPMF_FORSESSION` 会话级激活，公开 API，失败不阻断）。macOS 的 AX 聚焦目的地门控在 Windows 未采用——焦点实验证明 WeType 开麦不依赖文本焦点（6/6，桌面/资源管理器照常触发），聚焦门控留给未来 UIA 版本按需评估。

## BLE 僵死链路自动恢复调研来源（2026-09-05，重连健壮性专项）

场景：应用被强杀（未走正常关闭）后 Windows 侧残留僵死 GATT/HID 链路或服务缓存，普通重试永不恢复（本机真机取证：CCCD 订阅写入 E_ABORT、HID 接口从系统消失；examples\radio_probe 与 examples\gatt_snoop 探针复现）。已实现 `bluetooth_radio.rs` 自动恢复：重连连续失败达阈值时关开蓝牙无线电；每窗口最多 2 次并在 60 秒冷却后重开窗口，避免无限普通重连；应用启动时预取 Radio 对象与权限，避免故障发生后 WinRT 枚举自身也返回 `0x80070008`。2026-09-13 依据微软 `Close` 所有权边界与 MS Q&A 99038 的 service/device 成对释放结论，进一步补齐连接构建中途失败的全量显式清理；此前仅订阅后期分支清理，会让普通重试自身可能累积 WinRT BLE 资源。真机验证：系统栈健康时无线电开关周期后重连循环立即成功（Testing\investigation\sayall-gatt-20260905-live.log T/C 能力交换取证）；预热缓存和失败路径清理版僵死态仍待安装包复验。关键参考：

- **微软官方 GATT 客户端文档**（Dispose 后系统"小超时"自动断开、重建设备对象按需重连；BluetoothLEDevice.Close 仅当本应用是唯一持有者才关连接；GATT 连接/发现可能因系统队列等待数分钟且当前不能取消）：`learn.microsoft.com/windows/apps/develop/devices-sensors/gatt-client`、`learn.microsoft.com/uwp/api/windows.devices.bluetooth.bluetoothledevice.close`
- **微软 BluetoothLEDevice 构造入口文档**：`FromIdAsync` 明确要求从 UI 线程调用（可能触发访问授权）；`FromBluetoothAddressAsync` 无此线程要求，并支持从已进入系统缓存的配对设备地址重建设备对象。2026-09-12 现场的 MTA `FromIdAsync` 先返回 Windows 资源错误，后续日志时序显示下一次请求占住 BLE 工作线程（阶段日志缺失，属结合代码的推断），故改用配对 AssociationEndpoint ID 内的对端地址调用后者；不记录真实地址。官方依据：`learn.microsoft.com/uwp/api/windows.devices.bluetooth.bluetoothledevice.fromidasync`、`learn.microsoft.com/uwp/api/windows.devices.bluetooth.bluetoothledevice.frombluetoothaddressasync`。
- **MS Q&A 99038**（只 Dispose 设备不 Dispose 服务则无法重连）、**MS Q&A 2280559**（RPA 解析滞后导致进程重启后首次 GetGattServicesAsync 必 Unreachable，官方建议 3 次重试 ×1s + Uncached）、**MS Q&A 1685221**（FromBluetoothAddressAsync 返回 null 僵死 bug，Win11 2024.01D 已修；MaintainConnection 遇 bond 丢失会重连循环）
- **Qt 论坛 156281**（实测：OS 侧服务缓存僵死，重启应用无效，**关开蓝牙是唯一有效修复**——与本机取证一致，是本仓库选择无线电恢复的直接依据）：`forum.qt.io/topic/156281`
- **ZSTDJan/windows-remote-mic-app**，提交 `af54fd8e85a70f5b8f19cd4fa5bf11fe7fe530d6`，`apps/windows/rc003/src/ovb_rc003/ble_transport_winrt.py`（2026-09-14 复核）：参考实现从已配对 BLE selector 取得设备 ID 后调用 `FromIdAsync`，以 Uncached 发现服务；关闭时先取消写入/停止工作线程，再关闭 CCCD、退订事件并依次 Close service/device，且保留关闭失败的所有者供后续再次释放。本仓库据此修正 `BleSession::close` 首次 Close 失败后只回放旧错误、没有真正重试的缺陷。没有照搬其连接入口：同一僵死现场实测该配对 ID 路径返回 `0x80004004`，直接 GATT selector 返回 `0x80070008`，证明换构造入口不能恢复已经失效的系统栈。
- **Windows PnP 自动恢复公开接口**（2026-09-14）：微软 PnPUtil 文档提供 `/restart-device <instance ID>`，设备节点变更需要管理员权限；`ShellExecuteExW` 的 `runas` verb 用于显示系统 UAC 并启动提权操作；SetupAPI `SetupDiGetClassDevsW`/设备属性用于只选择当前存在、服务为 `BTHUSB` 的唯一蓝牙适配器。实现不记录实例 ID、不接受外部命令或路径，并在工具退出后独立用 WinRT Radio 枚举验证，而不信任单独的进程退出码。官方依据：`learn.microsoft.com/windows-hardware/drivers/devtest/pnputil-command-syntax`、`learn.microsoft.com/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw`、`learn.microsoft.com/windows/win32/api/setupapi/nf-setupapi-setupdigetclassdevsw`。

### 2026-09-16 A/B 对照：无线电 Off/On 在僵死态无可观测收益

上节 Qt 156281「关开蓝牙是唯一有效修复」的适用边界已用现场日志划定（证据
`artifacts/ev_stream_raw.txt`，0.2.6，僵死现场，2983 条 `ble_connect` 记录）：

| 组 | 样本 | 恢复 | 恢复率 |
| --- | --- | --- | --- |
| 实验组：Off/On 之后首次重连 | 488 | 3 | **0.61%** |
| 对照组：同事件内普通重试（`attempt>=1`） | 2406 | 15 | **0.62%** |
| （参考）进程冷启动 `attempt=0` | 89 | 26 | 29.21% |
| （参考）Off/On 自身报 `failed` | 345 | 0 | 0.00% |
| （参考）Off/On 自身报 `passed` | 143 | 3 | 2.10% |

两组相差 **-0.01 个百分点**（判定阈值 ±10），双比例 z 检验 z=-0.022 / p=0.982，
95% Wilson 置信区间实验组 0.21%–1.79%、对照组 0.38%–1.03%（大幅重叠）。
即：开关与不开关在统计上不可区分。旁证：Off/On 自身报告成功 143 次，其后也只恢复
3 次——**"WinRT 说开关成功"不等于"碰到蓝牙栈"**，原因是启动预热缓存的 Radio 对象
让 Off/On 命中缓存而非真实栈。这解释了 Qt 156281 的结论只在**系统栈健康**时成立，
僵死态不成立。

引用措辞边界：只能说"无可观测收益，不值得保留一条会打断链路的路径"，
**不能**说成"零效果"（实验组 CI 上限 1.79%）。

据此改为**按错误码分流**（`bluetooth_radio::is_stack_exhausted`）：命中
`windows_resource_exhausted` / `winrt_operation_aborted` 时跳过 Off/On 与 PnP 重启，只留普通
重连，日志落 `ble_recovery_decision action=skip_recovery reason=stack_exhausted_proven_ineffective`；
非僵死码仍走原 Off/On 路径保留兜底。复算脚本 `scripts/analyze-radio-recovery-ab.py`，
操作与判读标准见 `Testing/WindowsBleResourceRecovery.md`。

### 2026-09-10 重连窗口 F5 泄漏补充

- **微软 `RegisterRawInputDevices` 文档**：同一进程、同一 Raw Input 设备类只能
  有一个接收窗口，最后一次注册覆盖前者；文档因此明确警告库内注册会干扰宿主
  自己的 Raw Input 处理。该约束解释了旧版 `key_suppressor.rs` 的键盘注册被
  `raw_input_windows.rs` 覆盖、断线期 F5 设备归因失效：
  `learn.microsoft.com/windows/win32/api/winuser/nf-winuser-registerrawinputdevices`。
- **参考实现复核**：本机 `reference-repos/vibe-flow` 提交
  `b47f7cdce8b753fade0c64c97332bebe80f17d2d` 的 `VoxDeckInputBridge.cs` 对语音
  F5 使用 LL 钩子兜底，并在重连扫描码变化时仍以持久语音映射为准；它接受实体
  键盘 F5 冲突。本仓库采用边界更窄的做法：主 Raw Input 窗口统一归因，只有
  Connecting/Discovering/AwaitingCapabilities/Reconnecting 建链窗口临时兜底，
  稳定状态继续保留实体键盘 F5。
- **记事本行为旁证**：Microsoft Q&A 的 Windows/Notepad 条目确认 F5 会插入当前
  日期时间。2026-09-10 本机现象格式与系统区域格式一致，结合诊断日志
  `seen=74 swallowed=0 leaked=74`，可排除 ASR 把语音识别成日期的解释。
- **Bleak winrt client 源码**（Unreachable 重试 10×1s；断开全量清理序列 CCCD=None→退订→逐服务 Close 带 0.1s 防挂起延迟）、**btleplug winrtble**（Uncached 触发连接、特征发现 5s 超时回退 Cached——#325：部分驱动 Uncached 请求无限挂起，本仓库 connect 尚无该超时，列为后续加固项）、**微软官方 BluetoothLE 示例 Scenario2_Client**（FromIdAsync→RequestAccessAsync→Uncached 发现→清理序列）
- **Windows.Devices.Radios.Radio**（微软 `RequestAccessAsync` / `SetStateAsync`
  文档）：改变无线电前先请求权限并检查 `RadioAccessStatus::Allowed`；
  `SetStateAsync` 返回只表示请求是否获准，实际状态异步转换，应观察
  `StateChanged` 或复读 `State` 确认。2026-09-12 统一包实测旧实现 0-5ms
  即误判两轮恢复失败，据此改为进程内缓存 Allowed、Off/On 有界复读确认。
  微软还说明 `RequestAccessAsync` 可能触发授权，应从可交互 UI 上下文调用；
  Radio 可由 `GetRadiosAsync` 枚举，也可从已知 ID 创建。2026-09-13 现场证明
  系统资源耗尽后这三种取对象入口均返回 `0x80070008`，因此改为 Tauri setup
  阶段先取得并缓存对象，恢复线程只复用缓存；若启动预热失败但 BLE 后续恢复，
  则立即补建缓存。该设计只使用公开 Radio API，不引入提权或驱动。
  官方依据：`learn.microsoft.com/uwp/api/windows.devices.radios.radio.requestaccessasync`、
  `learn.microsoft.com/uwp/api/windows.devices.radios.radio.getradiosasync`、
  `learn.microsoft.com/uwp/api/windows.devices.radios.radio.setstateasync`。
- **Radio 设备查询兜底**（微软 `Radio.GetDeviceSelector` / `Radio.FromIdAsync`
  文档）：官方允许以 AQS + `DeviceInformation.FindAllAsync` 枚举后通过 ID
  重建 Radio，并说明硬件异常/移除场景下它比 `GetRadiosAsync` 更可靠。
  2026-09-12 现场两条路径均返回 `0x80070008`，据此把“公开 API 已穷尽”的
  人工提示边界固定下来。官方依据：
  `learn.microsoft.com/uwp/api/windows.devices.radios.radio.getdeviceselector`、
  `learn.microsoft.com/uwp/api/windows.devices.radios.radio.fromidasync`。

外部实现只作为带来源的参考。第三方应用进程注入、私有配置读取和来源不明二进制不进入稳定主路径。

## Windows 系统快捷键录入与锁屏动作（2026-09-10）

- **执行端**：微软 `SendInput` 文档说明它把事件串行插入输入流、受 UIPI 与当前键态影响；`LockWorkStation` 是交互桌面进程可调用的公开锁屏 API，成功返回只表示异步锁屏请求已发起。Hooks 文档说明全局钩子事件局限于调用线程所在桌面。按键映射中的精确 `Win+L` 因而先等待实体键释放、由门控成对处理 DOWN/UP，再调用 `LockWorkStation`；其他快捷键仍走既有 `SendInput` 并保持按下即响应。官方依据：`learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput`、`learn.microsoft.com/windows/win32/api/winuser/nf-winuser-lockworkstation`、`learn.microsoft.com/windows/win32/winmsg/hooks`。
- **录入端**：微软 `LowLevelKeyboardProc` 文档明确低级键盘钩子在按键消息进入目标线程队列前运行，处理后返回非零可阻止继续传递，并要求回调快速把工作移交后台线程。本仓库复用常驻 `WH_KEYBOARD_LL` 门控：录入期间先吞物理 DOWN，所有对应重复 DOWN/UP 即使录入已经结束仍按同一次按住吞完；录入开始前已经按住的键则全程放行，避免不对称边沿。钩子只 `try_send`，Tauri 事件由独立线程发出。官方依据：`learn.microsoft.com/windows/win32/winmsg/lowlevelkeyboardproc`。
- **边界**：`Ctrl+Alt+Del` 等安全注意序列不属于普通快捷键录入能力；Win+L 在当前
  Windows 主机上即使低级钩子返回吞下仍会锁屏。因此自定义录入默认保留直接模式，
  并提供用户显式开启的“界面选择修饰键 + 物理键盘只按主键”安全模式；安全模式
  不在输入流中生成系统组合。
- **钩子链顺序补充（第二轮现场复验）**：微软 Hooks Overview 说明钩子按链调用，
  已处理事件可停止继续传给后续钩子/目标；`LowLevelKeyboardProc` 也明确非零返回
  阻止后续传递。现场观察到本钩子吞下 Win/L 后仍被系统锁屏；链首重挂又导致边沿
  完全丢失，实证 failed 并回退。产品路径不再依赖钩子链顺序屏蔽系统保留组合。
  官方依据：`learn.microsoft.com/windows/win32/winmsg/about-hooks`、
  `learn.microsoft.com/windows/win32/winmsg/lowlevelkeyboardproc`。
- **TV→锁屏的协议选择器兜底（2026-09-12）**：微软 Raw Input 文档明确
  `RIDEV_NOLEGACY` 只适用于鼠标/键盘，不能据此阻止消费控制 HID 的独立 Shell
  动作；`SetWinEventHook` 提供跨进程、out-of-context 的对象事件观察，
  `EVENT_OBJECT_CREATE` 早于 SHOW。现场证明 Windows 会在 SayAll 锁屏约 4 秒后
  由系统服务创建 `OpenWith.exe`；SHOW 阶段隐藏仍偶发闪帧，CREATE 阶段终止精确
  helper 连续四轮无可见弹窗。产品路径只在“已观察 TV→下一次 SayAll 锁屏”的
  15 秒窗口启用，并只处理 Windows `System32` 下映像名精确为 `OpenWith.exe`
  的进程。官方依据：
  `learn.microsoft.com/windows/win32/api/winuser/ns-winuser-rawinputdevice`、
  `learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwineventhook`、
  `learn.microsoft.com/windows/win32/winauto/event-constants`、
  `learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess`。

## WeType 热键休眠自动恢复调研来源（2026-09-05，热键休眠专项 v2）

场景：WeType 2.1.3.18 后台约 40 分钟后"TSF 存活但全局键盘钩子休眠"——和弦注入 LWin 穿透、无 0xFC、ConsentStore 时间戳不动；打开 WeType 任意自身界面立即复活（kb-live 会话 23-26 真机取证）。跨进程 `SetProcessInformation(ProcessPowerThrottling)` 解除节流**真机证伪**（对其他进程 E_INVALIDARG 0x80070057，wetype_service 打开即 0x80070005，15:04 live12 取证），该路线已从 `wetype_revive.rs` 移除。v2 已实现（`ble.rs` + `ime.rs`）：检测（注入后 700ms ConsentStore 时间戳未动）→ TSF 配置切换唤醒（`cycle_wetype_profile`：激活微软拼音 80ms 后切回，公开 API）→ 300ms 后经 `WorkerMessage::RetryVoiceChord` 在工作线程释放旧和弦并重注入 → 二次检测未响应才提示人工。关键参考与实测：

- **TSF profile 管理 API**（`ITfInputProcessorProfileMgr::ActivateProfile`/`EnumProfiles`，`TF_IPPMF_FORSESSION` 会话级激活）：微软官方文档 `learn.microsoft.com/windows/win32/api/msctf/nf-msctf-itfinputprocessorprofilemgr-activateprofile`。选择理由：切换配置会向所有 TSF 感知进程广播激活事件，是唯一能从外部触达 WeType 的公开 API 路径。
- **会话 47 真机实测（live13 + kb-live.log 全解码，2026-09-05 15:21）**：休眠中按键 → 检测未响应 → 配置切换真实完成（STA 线程，切微软拼音 clsid 9D2B2E2B 再切回）→ **346ms 后重注入的和弦同样未开麦**（LL 钩子日志见注入的 5B 边沿泄漏可见、无 0xFC 标记）。
- **16:44-17:35 七次休眠发作实测（kb-live.log，2026-09-05 晚间复盘）**：用户大量使用语音键期间钩子反复休眠/复活，七个发作簇全部同构：首和弦失败（5B 泄漏）→ v2 自动重试（cycle+300ms 时序精确吻合）**7/7 失败**→ 用户在 cycle 后 **1.24/1.28/1.68/1.85/1.9/2.28s** 的再按全部成功（FC 标记 + ConsentStore 开麦交叉验证）。**结论：配置切换确实能复活休眠钩子，复活延迟实测 ∈ (300ms, ~2.3s]（一次疑似 ≤6s）**；+300ms 重注入恒过早。注意：该时段应用为并行会话部署的无日志实例（pid 11692，含共享分支上的 v2 代码），cycle 隐形运行——与 kb-live 时序吻合。据此实现重试阶梯（WETYPE_RETRY_SETTLE_MS=[2000,3000]，两轮 cycle+重注入，最后才提示人工）。
- **LL 钩子日志判据（2026-09-05 新增，kb-live.log 全天 128 和弦窗口解码）**：WeType 钩子存活时**消费注入的和弦 LWin 边沿并注入自己的 0xFC 标记对**（每边沿一对瞬时 D/U）；钩子休眠时 5B 边沿泄漏可见、无 0xFC。此判据与 ConsentStore 开麦时间戳 100% 交叉验证一致，成为"钩子死活"的即时观测手段（无需开麦）。全天时间线（毫秒时间戳锚定）：休眠形成于 15:08:21（最后一次成功会话结束）→15:16:10（首次失败）之间的**方向键-only 活动窗口（≤8 分钟）**；此前 14:13、15:04 等休眠段落与手动复活（打开 WeType 界面）全部对齐。**休眠形成是偶发的**：15:21:34 复活后钩子存活 ≥80 分钟（跨两次探针开麦会话、用户离开/打字交替），未再休眠——"40 分钟规律"不成立，形成条件未定。
- **健全性实测（2026-09-05 16:41 持锁）**：cycle profile（STA）后 1s 注入和弦照常开麦——**配置切换不破坏活钩子的和弦触发**，v2 复活路径前提成立。剩余验证：重试阶梯版待下一次自然休眠发作做端到端确认（一次按住内自动恢复）。
- **首按失败根因终局定论与验证（2026-09-05 21:34-21:38，commit 1b55cca 部署后）**：真正的根因是 **F5 泄漏三键拒绝**——遥控器闲置后应用自身被后台节流，0x04→抑制器武装的链路（经工作线程队列）拖 ~120ms，F5 的 60ms 有界等待超时泄漏 → 和弦变成 F5+Ctrl+Win 被微信输入法拒绝；断连重连变体中首个 F5 在 0x04 前泄漏、UP 丢失致 OS 键态粘 F5。修复（三重防线）：GATT 回调线程直接武装 + 和弦前 F5 解粘 UP + 抑制器决策计数日志。**验证结果：4/4 会话首按成功**（含一次 25 分钟闲置后首按），suppressor_stats leaked=0（135 个 F5 全部吞下，其中 1 个冷启动 F5 由 GATT 回调武装+有界等待兜住），kb-live 零 F5 D 泄漏、全部和弦带 FC 成功标记，解粘 UP 按设计仅在需要时进入 OS。"WeType 钩子休眠"理论正式退役：全部证据与 F5 泄漏 + 20ms 间隔冷态拒绝两个机制一致；重试阶梯保留为无害安全网。
- **`SetProcessInformation` 权限边界**：微软文档明确 ProcessPowerThrottling 仅作用于调用进程自身；对其他进程返回 E_INVALIDARG。真机取证一致（live12）。
- **WeType 进程布局（本机取证）**：开麦方为 `wetype_update.exe`（ConsentStore 条目，拥有顶层窗口 StatusBarWnd）；另有 wetype_service/wetype_server/wetype_renderer。休眠的是钩子所在后台进程，TSF DLL 运行于前台应用进程内不受影响——这解释了为何 TSF 路径（中文输入）存活而全局钩子休眠。

## 应用内更新（tauri-plugin-updater + GitHub Releases）调研来源（2026-09-05）

场景：Windows 应用内"检查更新 + 下载安装"（GitHub-only、零自建服务器）。关键行为均以插件源码/官方文档原文核对，非推测：

- **官方 updater 插件文档**（`v2.tauri.app/plugin/updater/`，免费开源，MIT/Apache-2.0）：静态 JSON 端点模式官方示例即 `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`（GitHub 302 到最新**稳定** Release 的资产，草稿/prerelease 不参与 latest）；`latest.json` 必需字段 `version`/`platforms.<target>.url`/`platforms.<target>.signature`，`signature` 为 `.sig` 文件**内容**（非路径）；签名强制不可关闭，私钥丢失即无法再向存量用户推送更新。
- **tauri-plugin-updater 源码**（plugins-workspace v2，`updater.rs`/`config.rs`，按 2.11.0 核对）：`check()` 对 204 返回"无更新"、200 解析 JSON 后按 SemVer `release.version > current` 判定；平台键按 `windows-x86_64-nsis` → `windows-x86_64` 顺序回退查找（latest.json 只需提供 `windows-x86_64`，dev 与 NSIS 安装态通用）；`Update.timeout` 默认 None（下载不限时），builder 的 timeout 只作用于 check 请求；下载完成后**先验签**再安装。**Windows 安装时序**：`install_inner` = 解包 → `on_before_exit` 回调 → `ShellExecuteW` 启动安装器（NSIS 参数 `/P`（passive）+ `/UPDATE` + `/R`（装完自动重启应用））→ `std::process::exit(0)`——**Drop 清理不会执行**，必须把 BLE 断开等成对清理放进 `on_before_exit`（本仓库 2026-09-05"部署不得强杀/强杀残留"教训的更新路径版）；`config.rs::validate_endpoints`：debug 构建 http 端点仅警告放行，release 构建强制 https（本仓库本地 E2E 用 `SAYALL_UPDATER_ENDPOINT` 覆盖端点 + dev 构建走 http，正式配置不含任何 dangerous 开关）。
- **tauri-cli/bundler 构建约束**（tauri issues #13259、#15638 + 组织讨论 #6013 佐证，并在本机复验）：`tauri.conf.json` 配置 `plugins.updater.pubkey`（及 `createUpdaterArtifacts`）后，构建时缺 `TAURI_SIGNING_PRIVATE_KEY` 环境变量会直接失败——现有 Windows CI 的无签名预览构建必须配套处理（无 Secret 时生成一次性临时密钥保 CI 绿灯；正式 Release workflow 缺 Secret 直接失败，防止发布不可用更新包）。
- **tauri-action**（官方构建+发布 Action，自动生成 latest.json）：评估未采用——它整体接管 build+release，无法嵌入本仓库既有的 verify-windows-bundle、安装生命周期矩阵等既有验收步骤；改为保留既有构建流程 + 自写 `generate-updater-manifest.ps1`（latest.json 生成逻辑对齐 tauri-action 的字段来源：version←tauri.conf.json、signature←`.sig` 文件内容、url←Release 资产直链）。
- **发布资产命名**：NSIS 产物名含中文与空格（`无线麦 SayAll_*.exe`），GitHub 资产直链需 percent-encoding；为消除编码风险，Release 资产在 CI 中复制为纯 ASCII 名（`SayAll-Windows-<version>-x64-setup.exe`）后上传，本地构建产物名不变（CI 全部脚本按 `*-setup.exe` 过滤定位，实测不受新增 `.sig` 影响）。
- **NSIS 与既有安装器门禁的相互作用**：updater 以 `/P`（passive）+ `/UPDATE` 运行，既有 installer-hooks.nsh 的 PREINSTALL SemVer 降级门禁照常生效（升级路径不受影响）；POSTINSTALL 的 VB-CABLE 提示在 passive（非 Silent）模式下仍会弹出——仅影响未装 VB-CABLE 的用户，与首装行为一致，保留。
- **预览版通道（2026-09-08 增补）**：Tauri 官方 Runtime Configuration 文档明确支持通过 `UpdaterBuilder::endpoints` 在运行时选择 stable/beta 等独立通道；本仓库据此保持默认稳定端点不变，仅在用户显式开启“检查预览版更新”后覆盖端点。GitHub Releases 页面公开提供标准 Atom feed（`releases.atom`），包含已发布的正式版与 Pre-release、排除 Draft；实现从本仓库 feed 的 `alternate` 链接读取 SemVer tag，选择最高版本并自行构造本仓库 `https://github.com/GetSayAll/remote-mic-app-windows/releases/download/<tag>/latest.json`，避开匿名 REST API 每 IP 60 次/小时限流。最终安装包仍由 Tauri minisign 强制验签。

## 2026-09-18 本地 Codex 定制版

- 基于 GetSayAll/remote-mic-app-windows，完整提交 `451e5f9ced0deecd31eb0147c0a578a22924492d`。沿用 GPL-3.0-only 代码及原版权声明。Windows 协议、音频及输入底层来自此上游，不宣称是本次从零实现。
- Codex 预设沿用已有版本化按键配置和上游手势生命周期；仅使用普通 Windows 按键及公开 AppsFolder 应用身份。语音键不参与预设，不改变按住/释放时序。
- Codex 已运行实例的身份确认参考 Microsoft GetApplicationUserModelId（https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-getapplicationusermodelid）、SetForegroundWindow（https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow）及 WM_NULL 异步前台确认说明（https://devblogs.microsoft.com/oldnewthing/20161118-00/?p=94745）。参考行为，无外部代码复制；未读取第三方私有配置。
- 本地版独立标识为 local.remote-coding.windows，执行文件为 remote-coding.exe。Codex 是目标应用名称，不表示 OpenAI 或 SayAll 官方发布/认可。
- public/app-logo.png、public/favicon.png 和 src-tauri/icons 的品牌资产已全部替换为原创几何图标，新资产按 GPL-3.0-only 提供，不沿用上游保留版权 Logo。

## 2026-09-18 本地 Codex 听写预设

- OpenAI 官方命令文档：https://learn.chatgpt.com/docs/reference/commands 。Windows 的 Start dictation 为 Ctrl + Shift + D；2026-09-18 用户确认本机为按住说话、松开结束。
- 沿用本仓库既有按住快捷键的成对 DOWN/UP 和异常释放机制，不更改已验证的注入间隔。WeType 专属输入法激活、麦克风观察与重试仅用于 Ctrl+Win；Codex 不复用这些专属副作用。
- 听写快捷键与音频传输是两个环节。遥控器麦克风仍通过 VB-CABLE；未声称调用私有 Codex API 或省去声卡驱动。


## 本地 Codex 快捷键与退格（2026-09-18）

- 快捷键事实数据：[OpenAI 官方命令参考](https://learn.chatgpt.com/docs/reference/commands)，Windows 表；查证 2026-09-18。独立整理为 `src/lib/codex-shortcuts.ts`，未复制页面实现。
- 普通删除参考 Windows 键盘行为与 [SystemParametersInfoW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow) 的 SPI_GETKEYBOARDDELAY/SPEED，读取用户设置、不改全局设置。组合键标点使用 [官方 VK_OEM 表](https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes)。保留原有 300ms 双击窗口，语音时序不变。
- 文本删除独立实现于 `text_edit.rs`，依据 [UIA TextPattern](https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-textpattern-overview)、[TextRange](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-usingtextrangeobjects)、[UIA 线程规则](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading)、[IUIAutomation2 超时](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nn-uiautomationclient-iuiautomation2)。公开 API 读取有限光标前文本、选择范围后一次 Backspace，不依赖第三方私有数据。
- 上游 RC003 调查仅作为线索，本机只读确认键盘 HID 集合；没有有实体正对照的输入事件，不能宣称 Back 不可用。参考 [HID 系统独占](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/hid-architecture)；不通过持续 GET_REPORT 轮询或未知 GATT 写入绕行。


## 本地日常默认方案与音量配置（2026-09-18）

用户要求解除音量键配置锁定并提供日常 Codex 默认方案。方案复用上节 OpenAI 官方 Windows 快捷键事实、已有公开按键注入与手势机制；没有改动输入监听、驱动、语音时序。移除上游 UI/持久化/运行时对音量键的策略禁用，沿用已有 VK/HID 解码；RC003 是否实际发送该格式仍待真机证明。音量默认不设映射，以保留系统原生音量与长按行为。

## PageUp/PageDown 扫描码修复（2026-09-18）

- [Microsoft KEYBDINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput) 说明 SCANCODE/EXTENDEDKEY 的物理按键编码语义。
- [Chromium CodeFromNative](https://chromium.googlesource.com/chromium/src/+/e357d701c9b59b4fcb17d65b17bcb8ce3d04cf08/ui/events/win/events_win.cc) 从 Windows 消息扫描码生成 DOM code。仅参考行为，未复制外部实现。
- 独立原生窗口实测旧 VK 注入的 PageUp/Down 消息 scan=00，Ctrl 状态正确；物理对照49/51。沿用已有扫描码发送路径补齐这两个键，不调整注入时序。证据与验证边界见 `Bugs/2026-09-18-page-navigation-scan-code.md`。

## 首击提前退格与双击补偿可行性（2026-09-20，仅调研）

用户提出“单击先普通退格、双击补偿后保留标点删除”。依据 Microsoft [TextPattern 读写与跨进程调用边界](https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-textpattern-overview)、[GetActiveComposition](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtexteditpattern-getactivecomposition) 和 [KEYBDINPUT Unicode 输入](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput) 核查：受控 UIA 方案可研究，但需要首删前快照、组合态判断和实际恢复验证；未找到本仓库来源矩阵中已验证的乐观补偿先例，未复制实现。当前保留既有 300ms 窗口，提前退格尚未实现，冷/热/闲置首用延迟与 Unicode 恢复尚未验证，不宣称零延迟或通用 Ctrl+Z 安全恢复。既有双击按标点删除的实际测试与新提案分开记录，详见 [可行性调查](docs/investigations/2026-09-20-optimistic-backspace-feasibility.md)。

## WebView 焦点所属窗口校验（2026-09-20）

本机 RC003 双击已进入文字删除模块，但测试框的 UIA 元素进程与 Tauri 主窗口进程不同，旧版严格 PID 相等检查拒绝执行。依据 Microsoft [RawViewWalker](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation-get_rawviewwalker)、[GetParentElement](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtreewalker-getparentelement) 与 [GetAncestor](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getancestor)，跨进程焦点改为证明最近原生宿主 HWND 的实际所属进程与 UIA 宿主进程一致，且其 GA_ROOT 精确等于当前前台窗口；不沿 owner 关系放行，不移除焦点、密码、取消和选区校验。自家测试框的公开 UIA 祖先链已验证符合该条件；实际删除结果单独记录于 [Bug 与验收证据](Bugs/2026-09-20-punctuation-webview-focus.md)。未复制外部代码，未改 300ms 双击窗口。

## WebView 文本范围限定（2026-09-20）

依据 Microsoft [DocumentRange](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextpattern-get_documentrange)、[MoveEndpointByUnit](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextrange-moveendpointbyunit)、[MoveEndpointByRange](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextrange-moveendpointbyrange) 和 [FindText](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextrange-findtext) 的公开范围与端点语义独立实现。另只读参考 [Chromium 官方 TextRange provider](https://raw.githubusercontent.com/chromium/chromium/main/ui/accessibility/platform/ax_platform_node_textrangeprovider_win.cc) 的 `MoveEndpointByUnitHelper` 与 `FindText`：前者沿 AX 文本边界移动，后者把共同祖先中的文本偏移转换为叶节点位置；未复制外部实现，也不把 Chromium 当前主干视为本机 WebView2 的精确版本。

本机自家固定测试框的只读实验发现：其 `DocumentRange` 长度为 12，原先从末尾向前移动 2048 个字符，实际得到长度 829、起点早于该控件文档的范围，随后标点查找与删除范围均为空。仅将起点限制到自身 `DocumentRange.Start` 后，前缀长度 12、标点范围长度 1、待删范围长度 6，全部与固定预期精确一致，文本和选区保持不变。原始失败及对照元数据保留在本地忽略目录 `target/local-launch/rc003-integration/own-boundary-clamped-observation.json`，输出仅布尔与数字。

修复先验证空光标位于当前 TextPattern 自有文档内，再在读取文本之前限制克隆范围起点；重新检查范围两端、顺序及末端仍为原光标，保留精确文本、选区、焦点、取消与密码检查。日志记录是否限制起点、实际移动单位和拒绝原因，不记录文本。上述只读实验单独只证明范围构造修正；后续结合下节选区修复的真实产品函数已实际删除，证据分轮记录。最终安装版实体遥控器及跨应用兼容性仍须分别验收；此修复不改变双击等待策略。

## UIA 选区异步确认与取消清理（2026-09-20）

Microsoft [TextRange.Select](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextrange-select) 定义选择范围操作。[Chromium 官方 provider 的 Select 实现](https://raw.githubusercontent.com/chromium/chromium/main/ui/accessibility/platform/ax_platform_node_textrangeprovider_win.cc) 将 `kSetSelection` 交给 delegate 后返回，未在该函数内等待 GetSelection 确认；仅参考行为，未复制实现，也未声称该主干与本机运行时精确一致。

本机产品函数实证：从 Select 调用开始到第一次读取结束约 257 微秒（Select 自身约 145 微秒），仍得到原空光标；上下文保持 12 个 UTF-16 单位不变，约 3 毫秒后目标选区生效。后续只读比较证明目标选区与重建范围的 Compare 和两端 CompareEndpoints 一致，未采用放宽范围比较的替代方案。修复只在原空光标、精确目标选区两态之间有界观察，失焦、文本改变或第三种用户选区立即拒绝；使用既有操作预算、实际 UIA 查询及线程让出，不以固定休眠代替确认。取消清理独立限时，等已提交的选择请求落实后仅恢复自己的光标，并实际观察恢复；超时或 API 失败时向调用者明确报告恢复未确认，不把 S_OK 记为清理通过。

新增等待、拒绝第三态、取消后迟到选择与恢复、预算边界测试；定向 `text_edit` 共 17 项 passed。自家 WebView 产品函数自动验证共 6 项 passed：普通后缀 12→6（264ms），末尾标点 6→6（138ms），普通退格 12→11（0ms，提交耗时），后缀重复 12→6（204ms、199ms），固定框闲置 152.542 秒后 12→6（探针 245ms，产品内部 244ms）。每项除 API 结果外均核对了最终实际文本与固定预期完全相等。探针调用前有自己的 UIA 保护性检查，可能预热 provider；闲置项不等于未经预热的冷态首按验证。成功选择观察均为 `pending_count=0`，不宣称此次成功运行直接覆盖了等待中间态。

这些数值不能当作跨应用保证或遥控器端到端延迟；最终安装版实体 RC003、取消真实窗口、新提前退格/补偿和其他应用覆盖分别待验收。补偿候选只以精确可验证的纯文本为范围，字符恢复不证明富文本格式恢复。版本化布尔、长度及耗时见 [软件 UIA 执行证据](Testing/evidence/punctuation-webview-uia-execution-20260920.json)；完整诊断保留于本地忽略目录 `target/local-launch/rc003-integration`，版本化证据不包含文本、设备身份、进程/窗口标识或个人路径。
