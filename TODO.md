# TODO

## v1 决策（2026-09-04）

- 第一版只做微信输入法听写：按住说话快捷键默认 左Ctrl+左Win（适配微信输入法默认语音热键）；语音可用 = 按住语音键 → 注入快捷键 → ATVV 音频经 CABLE Input → 微信输入法麦克风（CABLE Output）→ 云端识别 → 文字上屏。端到端链路音频段已本机实证（调查报告 evidence/n）；注入段配方约束已本机实证（evidence/p：WeType 拒绝单批零间隔和弦，须逐事件注入；间隔 20/40/60ms 均 4/4 触发、零间隔 0/2，~~默认取 20ms 压低按键延迟~~（2026-09-05 更正：20ms 为热态验证结论，冷/节流态必失败已实证并回退 80ms，见下方"性能已知项"与提交 86e5314——延迟优化必须以成功率保证为前提），Bugs\2026-09-04-wetype-zero-gap-injection.md）；**延迟账目已量化（2026-09-05，evidence/p 端点预热对照实验）**：注入→WeType 开麦固定 ~163ms（冷/热端点中位数差 0.3ms，预热无效；WeType 内部处理，第三方边界不可干预），两型号实际均直接 0x04 推流（无开麦往返可并行），0x04 早于 HID F5 60-90ms（触发点已最早），全链 ≈215-245ms 其中应用侧仅和弦 20ms 可控——**应用侧延迟优化到此收敛**；RC001 遥控器端到端真机 passed（2026-09-04，用户确认文字上屏；用户侧前提=输出端点选 CABLE Input + 系统默认录音设备切 CABLE Output，应用不修改系统默认设备）；RC003 真机待验。豆包（注入判死四层闭环）与 WinUHid 增强轨延后，见 ADR 0002 与 docs/investigations/2026-09-04-avoid-driver-signing-input-paths-final.md。

## 后续产品功能（开发前对照 Mac App）

以下功能均以 Mac App 的产品流程、页面结构、状态反馈、文案语义和异常处理为实现参考；开发前先完成对照调研并记录结论。Windows 侧仍须遵守本仓库架构边界，只使用公开 API、公开协议、全局快捷键和用户可见的辅助功能界面；因平台能力产生的差异须在 `ATTRIBUTION.md`、路线图或对应调查文档中说明，不直接移植 macOS 平台代码。

- [ ] 增加首次使用 Onboarding 流程页面：参考 Mac App 的步骤顺序和完成条件，覆盖遥控器连接、语音输出设备、目标输入法选择、按住说话验证与失败恢复；支持中断后继续、完成后重新进入，并为关键状态、分支和外部调用补齐结构化日志。
- [ ] 支持 Typeless：参考 Mac App 的选择、配置、触发、状态反馈和恢复流程，调研 Windows 公开能力后实现按住说话生命周期、音频路由与失败关闭；分别完成 RC001/RC003、冷态首用、快速连按、断连和睡眠恢复真机验收。
- [ ] 支持豆包输入法：参考 Mac App 的产品行为与配置引导，在不读取或修改豆包私有配置、内部数据库、内存或私有协议的前提下设计 Windows 支持路径；基础能力不得依赖进程注入，若必须使用提权 Helper 或虚拟 HID，须保持独立、显式启用且不影响现有语音主路径，并分别完成 RC001/RC003 真机验收。
- [ ] 完善聚焦输入框处理：参考 Mac App 对目标输入框的识别、焦点保持、恢复和无可编辑目标时的用户提示；Windows 仅使用公开的焦点与辅助功能 API，避免静默把语音结果送入错误窗口，并覆盖焦点切换、窗口关闭、应用切换、Onboarding/设置窗口前后台切换及语音会话中焦点变化。

## Windows RC001 / RC003

- [x] 参考 macOS `SMAppService.mainApp` 实现 Windows 当前用户登录自启动：关于页可开关，使用 `HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run`，启动时同步并记录结构化日志；不需要管理员权限。**2026-09-15 本机 Windows 真机 passed**：release 安装版 0.2.6 注销重登后自动启动，进程父进程为 `explorer`（由登录 shell 拉起，非手动启动），启动链 `document_load finished → vue_mount(80ms) → initial_ipc_ready(337ms)` 完整，日志 `startup feature=launch_at_login action=sync terminal_result=passed enabled=true`。验证要点：`Win+L` 锁屏再解锁**不会**触发 `Run` 项（用户会话未结束），必须注销（`shutdown /l`）或重启才能验证。

- [ ] 鼠标动作映射：支持左/右/中键单击、左键双击、每次 1–100 格滚轮及每次 1–2000 物理像素指针移动；不修改默认绑定。RC001/RC003 实体按键及闲置首按回归仍需分别验收，见 `Testing/WindowsMouseActions.md`。
- [ ] Windows 注册应用扫描与应用库：支持搜索、多选/全选、配置保存及导入导出；扫描或添加应用不会自动启动或绑定。RC001/RC003 实体按键回归仍需分别验收，见 `Testing/WindowsRegisteredApps.md`。
- [ ] 遥控器电量显示：按所选 BLE 对端读取 Windows 缓存电量，断连、睡眠或缺失时显示未知，不额外进行 GATT 操作。持续更新、断连、睡眠及 RC001/RC003 实机验收见 `Testing/WindowsBattery.md`。

- [x] 建立独立 Rust + Tauri 2 + Vue 3 工程结构。
- [x] 建立 Mac 原版风格设置界面骨架。
- [x] 建立 ATVV、ADPCM 和语音会话纯 Rust 核心。
- [x] 建立 Windows CI 和真机测试手册。
- [x] 实现 WinRT 已配对设备扫描、GATT 连接/释放、ATVV 通知和 PCM 解码代码路径；Windows 与 RC001/RC003 运行验收仍待完成。
- [x] 补充 RC001/RC003 设备名称与标准 GATT Model Number（2A24）识别，在连接快照和界面中传递型号；无法识别时保持 `unknown` 且不阻断 ATVV。Windows 双型号真机验收仍待完成。
- [x] 将 RC001 短语音场景作为 JSON 夹具回放，覆盖 40 + 80 字节拆包、20 次极速空会话、20 次完整会话和中断后首个新会话恢复；该回放不代表真实 Windows/RC001 固件验收。
- [x] 将真实连接阶段、能力与解码采样计数接入 Tauri IPC 和连接页面。
- [x] 使用同一 JSON 契约夹具验证 Rust 序列化与 TypeScript 接口的 `PlatformSnapshot`、`PairedRemote`、camelCase 字段及 RC001/RC003/unknown 枚举值；Windows WebView 运行时 IPC 仍待验收。
- [x] 实现显式 WASAPI 输出端点枚举、选择、16 kHz PCM 写入、有界队列和真实 padding 排空代码路径。
- [x] 实现用户显式配置的语音键按住说话快捷键（连接页预设：关闭、右 Alt、F5、Win+H、左 Ctrl+左 Win）：按下语音键先注入 DOWN 再开始音频会话，释放统一注入 UP，断连/睡眠/中止/退出强制释放；注入时序参考 ZSTDJan 按住说话快捷键与 Voice_VibeCoding 的 Hold 语义，仅使用 SendInput 公共 API（见 ATTRIBUTION.md）。2026-09-04 修复一：和弦改为逐事件提交、事件间 80ms 间隔（WeType 拒绝单批零间隔，evidence/p）。修复二：F5 抑制器会话武装信号误接未启动的旧模块 voice_key_suppressor（ble.rs），遥控器 F5 泄漏进和弦致 WeType "额外按键"拒绝——改接 key_suppressor 并删除旧模块（Bugs\2026-09-04-wetype-zero-gap-injection.md）。2026-09-10 再修复：两套同类 Raw Input 注册互相覆盖导致重连期设备归因失效，改为主监听器统一转发，并在建链四相位临时保护 F5，防止记事本收到 F5 插入时间戳；新增逐阶段 BLE 重连日志（Bugs\2026-09-10-voice-f5-timestamp-during-reconnect.md）。加固：钩子链头 bump（会话开始 + 10s 定时）+ Raw Input 单一注册。**RC001 真机端到端 passed（2026-09-04，用户确认文字上屏；前提=输出端点 CABLE Input + 系统默认录音 CABLE Output）**；RC003 本修复真机待验。
- [ ] 使用真实 RC001/RC003 和第三方语音程序（微信输入法、Win+H 等）验证按住说话快捷键：DOWN/UP 严格成对、无粘键、无重复音频，且断连和睡眠恢复后不残留按住的快捷键。RC001 基本链路与加固版回归均已 passed（2026-09-04，型号经应用 2A24 显示双证）；RC003 基本链路 passed（连接/触发/MIC_EXTEND 续期正常），音频送达率经**重配对后复测 passed**（55%→98.7%，与 RC001 基准持平，文字"一二三四五六七八九十"全对——初次配对的连接参数带宽不足，重配对即修复，已列为标准处置；Bugs\2026-09-04-rc003-voice-quality.md）。剩余待验：快速连按成对性、断连/睡眠恢复残留复验。
- [x] 实现 RC001/RC003 选择持久化、意外断连指数退避重连和 Windows 睡眠/恢复通知代码路径；2026-09-12 修复 BLE MTA 线程误用 UI-thread-only `FromIdAsync` 导致 Windows 资源错误/工作线程卡死，改由配对 ID 的对端地址调用 `FromBluetoothAddressAsync`，并补齐连接阶段、退避与无线电恢复结构化日志；2026-09-13 针对 `0x80070008` 补充启动期 Radio 预热缓存，把“两次恢复后永久耗尽”改为 60 秒冷却后自动重开恢复窗口，并补齐设备创建后所有连接失败路径的事件退订、CCCD、连接参数、GATT service 和设备显式释放，避免重试自身持续泄漏 WinRT BLE 资源；2026-09-14 增加系统 BLE 栈已经连 Radio 枚举都失败时的 BTHUSB 设备节点自动重启兜底（系统 UAC 明示授权、精确选择唯一适配器、独立 WinRT 读回验证），并修复会话 Close 首次失败后不再真正重试 service/device 释放的问题。现场 `0x80070008` 经新兜底恢复 passed；本地包与 RC001/RC003 各自端到端验收见 `Testing/WindowsBleResourceRecovery.md`。
- [x] 在 Windows 主机编译 Tauri NSIS Preview 安装包；Windows CI 已生成并复验绑定精确来源 Commit、SHA-256 和未签名状态的 artifact，安装、升级、卸载与正式签名仍待完成。
- [x] 提供去标识化运行诊断摘要和页面内复制入口；自动化已证明不导出设备身份、路径、端点名称或错误原文，Windows WebView 剪贴板仍待运行验收。2026-09-16 诊断入口从权限页迁到关于页（权限页只保留蓝牙/按键/音频三项状态，仿真新增断言防止入口回流），并在关于页增加"打开日志目录"：目录由 Rust 从日志初始化的实际落盘路径推导、前端不传路径（维持 capabilities 最小权限），打开复用 ShellExecuteW 链路；Windows CI 仿真只验证 WebView → Tauri IPC → Rust → shell 的往返与终态消息，真实桌面资源管理器打开仍 deferred。
- [x] 持久化并展示仅保存在本机的每日按键次数、完整语音会话次数和语音采样时长；Windows/RC001/RC003 真实事件计数与升级保留仍待真机验收。
- [x] 对低于 Windows 10 1809（build 17763）的系统增加 NSIS 安装与应用启动双层拒绝门禁；Windows 10 1809 / Windows 11 提示和安装行为仍待真机验收。
- [x] 在 Windows CI 对 NSIS Preview 执行 `/S` 当前用户安装、启动存活、`/S` 卸载及设置保留边界验证；该自动化不代替可见安装界面、SmartScreen、Windows 10 1809 或真实用户环境验收。
- [x] 在 Windows CI 使用仅测试构建可启用的平台仿真，验证真实 WebView JavaScript → Tauri IPC → Rust command、五页导航、RC001/RC003 扫描、首次 RC001 语音、音频端点、Raw Input、映射、诊断和资源释放闭环；生产 NSIS 已验证不含仿真入口，该结果不代表真实 Windows API 或硬件通过。
- [ ] 使用真实 RC001 验证型号识别、BLE 配对、连接、断开、重连和首次语音。
- [ ] 使用真实 RC003 验证型号识别、BLE 配对、连接、断开、重连和首次语音。
- [ ] 验证 `STREAM_START → AUDIO → STREAM_STOP` 首次会话完整可用。
- [ ] 在 Windows 真机验证 WASAPI 端点初始化、VB-CABLE 回环、欠载恢复与完整尾音。
- [x] CABLE Input 双层静音自愈：端点主静音使用 `IAudioEndpointVolume`；音量合成器里的 SayAll 应用会话静音使用 `ISimpleAudioVolume`，只匹配当前进程在已选 CABLE 端点上的会话。初始化、会话开始、流启动后检查，推流期间每 100ms 低频检查，必要时解除静音并读回；两层均不修改音量。2026-09-07 Windows 真实 CABLE Input 受控复现 passed：端点 open/begin 两检查点，以及应用会话 begin/after_start/stream_watch 三检查点均成功从 muted 恢复到 unmuted；随后真实 RC001 连续 9 次语音均复现“流启动约半秒后会话被外部重新静音”，监视路径 9/9 捕获并恢复为 unmuted（2765 个音频包），9 次开始/停止与快捷键按下/释放均成对；修复版 NSIS 本地包由用户复测确认 RC001 语音功能 passed。边界：完整 RC003 → CABLE → 输入法语音链仍按上一项真机验收。
- [x] 生产诊断日志默认持久化到 LocalAppData：覆盖进程/Tauri/前端/Vue/首次 IPC 启动链，音频端点枚举与 `virtual_cable|bluetooth|other` 脱敏分类、WASAPI 打开/自动转换/推流/排空/中止/失败，以及按键映射和按住说话快捷键的加载、保存、重置；禁止原始音频包、端点名称/ID、自定义应用路径和异常正文进入生产日志。2026-09-08 自动化验证 passed；真实蓝牙耳机故障复现与安装包白屏现场日志验收 deferred。
- [x] 在可见 NSIS 安装完成后检测 VB-CABLE 服务，未安装时说明第三方来源、管理员权限和重启要求并打开官方下载页；应用首次启动复检唯一 CABLE Input 并在无既有选择时自动配置。静默安装不打开网页，真实安装/重启仍待真机验收。
- [ ] 如未来需要捆绑或自动执行 VB-CABLE 驱动包，先取得与 Pack45 内附许可一致的作者书面授权，并实现来源校验、显式 UAC、结果检测和重启流程。
- [x] 持久化用户选择的输出端点，并在端点消失或更名时失败关闭；Windows 运行时恢复仍待真机验收。
- [x] 实现设备路径 fail-closed、隐藏消息窗口、Keyboard/HID 双来源合并和停止释放的 Raw Input 代码路径；Windows 与 RC001/RC003 真机按键验收仍待完成。
- [ ] 实现按键映射保存、热加载和 SendInput：独立映射文件、显式热加载、批量 SendInput、部分提交回滚和界面测试已完成；2026-09-10 修复 Win+L：锁定动作改走公开 `LockWorkStation` API（RC003 电源单击现场 passed）。精确 Win+L 先等待实体 UP、门控完成边沿配对后再锁屏；2026-09-12 进一步实证 `microsoft-edge:` 弹窗并非迟到边沿，而是 TV 原生 Shell 协议动作在后续锁屏时由系统服务创建 `OpenWith.exe`，新增仅在 TV→SayAll 锁屏周期启用的 CREATE 阶段精准拦截（原型四轮现场 passed，产品化安装包待验）。当前主机实证物理 Win+L 无法由普通用户态钩子可靠阻止，链首刷新方案又造成事件丢失，已回退；录入默认保留直接模式，并增加默认关闭的安全模式开关（界面选修饰键、键盘只按主键），两种模式自动化 passed、安全模式安装现场复验 deferred。真实 Raw Input 边沿自动执行仍须分别等待 Windows/RC001/RC003 确认 Keyboard/HID 事件形态，避免重复输入。
- [x] 按键映射页增加“保存配置 / 导入配置 / 导出配置”：沿用保存即热加载，导出版本化且稳定排序的 JSON；导入先做 1 MiB 上限、格式版本、动作与快捷键完整校验，落盘成功后才一次性替换运行态，取消选择不报错。Rust/Vue 自动化与 Windows COM 对话框代码路径 passed；可见文件选择器、跨机器迁移及 RC001/RC003 导入后实体按键回归 deferred。
- [ ] 完成 Windows 10 1809 / Windows 11 安装、升级和卸载验证。
- [x] 在 Windows CI 构建较低版本 NSIS 候选，验证当前用户安装、升级后单一安装身份、设置/映射/统计逐字节保留、降级不替换当前版本和最终卸载保留用户数据；该矩阵不代表真实历史二进制、可见安装界面或 Windows 10 1809 / Windows 11 真机验收。Tauri 2.11.1 静默页不会可靠设置内置降级检查所依赖的版本比较结果，已在既有 preinstall hook 中增加独立 SemVer 门禁；Run 33637195089 通过并确认 predecessor `/S` 返回 1638、当前 0.1.0 与用户数据保持不变。
- [ ] 建立自签 Authenticode、证书指纹和 SHA-256 发布流程。
- [ ] 单独评估返回键、音量键等完整 HID 实验能力。
- [ ] 按 ADR 0002 立项可选 Helper（虚拟键盘驱动 + 按设备吞键）：物理按键对照已完成（2026-09-04，豆包/微信物理可唤起、注入不可，见 Bugs/2026-09-04）；待完成 RC001/RC003 按键形态真机确认；驱动来源初查完成（cgutman/WinUHid，MIT，源码小可审计，无预编译 Release 需自构建签名），详见路线图阶段 E；不得进入基础路径，不阻塞 Preview。

### 2026-09-05 附加

- **应用内更新（tauri-plugin-updater + GitHub Releases，新增）**：关于页"检查更新"手动入口 + 启动静默检查（失败完全无声）+ 下载进度 + passive 安装自动重启；默认稳定通道使用 `releases/latest/download/latest.json`，用户可显式开启“检查预览版更新”，经 GitHub Releases Atom feed 选择最高 SemVer 的已发布版本（包含 Pre-release）；开关默认关闭并持久化，两个通道均由 minisign 强制验签。安装器启动前经 `on_before_exit` 显式断开 BLE 链路（插件在 Windows 上 `std::process::exit(0)` 不走 Drop 清理）。待完成边界：① 已安装 0.2.1 不含预览通道开关，无法自行发现 Pre-release，0.2.2 首次引导需单独处理；② GitHub Secret `TAURI_SIGNING_PRIVATE_KEY` 未配置时 CI 用一次性密钥兜底、正式 Release workflow 直接失败；③ Authenticode 代码签名仍待建立（updater minisign 验签独立于 Authenticode）；④ 大陆访问 GitHub 的网络可用性未量化（插件支持多端点兜底与系统代理，已留扩展位）。参考与源码核对记录见 ATTRIBUTION.md 更新调研节。
- **BLE 僵死链路自动恢复（bluetooth_radio.rs，新增）**：应用被强杀后 OS 侧 GATT/HID 链路或服务缓存可能僵死，普通重试永不恢复。重连循环连续失败 5 次后自动执行 Off→2s→On；每窗口最多 2 次，之后冷却 60 秒并自动开启下一窗口，既防抖又不永久停止自愈。2026-09-12 按微软文档补 `RequestAccessAsync` + Allowed 检查 + Off/On 有界状态确认；2026-09-13 再补 Tauri UI setup 阶段预先取得权限并缓存 Radio 对象，使系统稍后进入 `0x80070008` 时无需重新枚举即可恢复，连接恢复后也会补建缓存；同时以 `PendingBleConnection` 保证服务/特征发现和订阅任一步失败都显式回滚已取得的 WinRT BLE 资源，防止自愈重试反过来扩大资源耗尽。2026-09-14 现场进一步证明参考实现 `FromIdAsync`、直接 GATT selector 和 Win32 GATT 均无法穿透已经僵死的内核蓝牙栈；新增仅在 `0x80070008`/`0x80004004` 且 Radio 路径失败时触发的 PnP 兜底：SetupAPI 精确定位唯一 `BTHUSB` 设备节点，使用系统 `pnputil /restart-device` 请求 UAC 后重启，并以 WinRT Radio 重新枚举作为成功判据。现场恢复测试 1.95s passed，随后设备对象创建恢复；会话 Close 的 service/device 失败也改为后续调用真正重试。完整本地包启动验证待本次交付，RC001/RC003 各自制造僵死后的自动连接仍 deferred。详见 Bugs/2026-09-07-ble-unreachable-both-remotes.md、Testing/WindowsBleResourceRecovery.md 与 ATTRIBUTION.md BLE 恢复调研来源。
- 语音键 F5 抑制器补防粘键配对（VVC 同款"DOWN 漏进 OS 则 UP 必放行"）：按下沿 60ms 有界等待超时泄漏时，释放沿放行，杜绝"F5 粘住→和弦全部被拒"的整机失效模式。

### 2026-09-05 IME 专项

- **语音"无法唤起"根因 = 会话活动输入法不是微信输入法**（WeType 语音热键仅在自身活跃时生效；焦点无关，桌面/资源管理器聚焦 6/6 照常开麦）。修复（ime.rs）：注入和弦前用公开 TSF API 会话级激活 WeType（TF_IPPMF_FORSESSION，零延迟 3/3 实证），失败不阻断。参考 macOS 版 PreferredInputSourceMonitor 职责设计。

### 2026-09-05 性能已知项（评估归档，供后续修复）

背景：首按失败根因修复（80ms 回退 + F5 三重防线，PR #19）验证通过后，对语音链路做整体性能评估。当前全链延迟 ~300ms（按键→开麦），其中外部因素 ~200ms。逐项账目与处置边界如下，**勿盲改**——每一条都有实测依据。

**外部边界（第三方/固件，不可干预，勿再投入）**：

- WeType 内部识别和弦→开麦固定 ~163ms（evidence/p 13 次实测 ±5ms，端点预热无效）。
- BLE/固件按键→0x04 通知 ~30-60ms（0x04 早于 HID F5 60-90ms，触发点已最早合法位置）。

**正确性取舍（"延迟优化必须保证成功率"规则项，勿回退）**：

- 和弦间隔 80ms：20ms（cef24d3）冷/节流态必失败（2026-09-05 用户实证 7 次发作），86e5314 回退。热态验证 4/4 不代表可交付。
- F5 解粘 20ms（和弦前保险 UP + 间隔）：跨应用重启的 OS 粘键状态无法便宜检测，须无条件执行。

**待修复项（按优先级）**：

- [ ] **笔记本功耗：后台节流豁免改为条件化**（bf03f0e 当前全局豁免——它是首按修复的组成部分，全局豁免代价是闲置功耗略高）。方向：仅在遥控器连接期间豁免，断连后恢复参与节流；重连可靠性已由 WakeReconnect + 无线电自愈兜底。台式机无影响；上笔记本场景前处理。
- [ ] **失败恢复加速（可选）**：wetype_check 检测判据从 ConsentStore 注册表（700ms 检查窗）换 LL 钩子 0xFC 标记观察（毫秒级，kb-live 已验证与开麦 100% 交叉一致）。仅加速失败路径的重试触发，成功路径零收益；需抑制器/钩子层新增观察通道，注意钩子线程不做 IO。
- [ ] **冷态管道 ~120ms（低优先级）**：闲置后首按应用内部链路（GATT 回调→武装→工作线程→IME 查询→和弦）实测可拖 ~120ms（不失败但慢）。节流豁免已生效仍有首次线程调度延迟；武装已内联到 GATT 回调。进一步压缩收益 ~100ms 冷态延迟，风险中（动的是刚修好的链路），无用户报障不动。

### 2026-09-07 按键映射单响应专项

- **冷首按原生残留（2026-09-08 用户调整策略）**：武装族按键（确定/方向）在闲置 >4s 后的首次按压会附带一次原生按键动作；同键映射由泄漏对冲保证净单响应，不同键映射仍会同时出现原生动作与配置动作。左键已恢复自定义，与上/下/右/确定使用相同的逐键 4s 武装机制。Home/TV 继续采用方案 C"遥控器优先"常驻抑制；零代价终局仍是 Helper 轨（ADR 0002）。
- **返回/音量±全型号禁用（2026-09-07 用户决策，真机验证时确认）**：此前 RC001 上三键以 VK 0xFF 厂商键可达且可直接归因（可正常映射），RC003 上输入栈不可见（格子禁用）——两型号行为不一致造成用户困惑（连 RC001 时格子放开）。决策：统一全型号禁用（RC001 也不开放），UI 格子禁用 + 持久化层/引擎层双重剥离；电源/菜单保持可配（直接归因且用户未要求禁用）。
- RC003 按键映射真机验收 passed（2026-09-07 01:13–01:15 用户全键测试，remote-capture 逐事件比对：同键映射×4 两路径单响应、菜单直接归因、TV/主页 open_app 生效、物理键盘无劫持）。
- RC001 按键映射真机验收 passed（2026-09-07 下午，用户真机验证 c70767d 构建确认：返回/音量±/左键三键禁用策略符合设计、Home/TV 遥控器优先严格单响应含闲置首按、语音链路无回归）——两型号按键映射验收均已通过，详见调查档案"RC001 真机验收记录"节。

## 2026-09-18 本地 Codex 定制版

- [x] Codex 操作页面、七键预设、首次配置备份和恢复；前端 105 项与 Rust 180 项测试 passed。
- [x] 原生应用构建、可见界面与配置保存/恢复实测通过；RC003 连接和 ATVV 握手通过。
- [ ] 本机 Codex 已运行窗口恢复的实体遥控器验证。
- [ ] 遥控器实体按键与语音到输入法闭环；配对成功不等价于这些项目通过。
- [x] 独立本地标识、原创图标和上游更新隔离已实现；具体构建/运行验证见 Testing/LocalCodexWindows.md。
- [x] 增加 Codex 听写 Ctrl+Shift+D 预设，复用按住/松开成对生命周期，隔离微信输入法专属激活与重试；298 项自动测试、发布编译与原生选项保存通过。遥控器麦克风到 Codex 文字闭环因缺 VB-CABLE 仍 deferred。


## 本地 Codex 遥控扩展（2026-09-18）

- [x] 独立 Codex Windows 快捷键栏目：73 个动作、搜索分类、上下文提示与官方来源；前端目录/键码契约验证通过。
- [x] 返回键普通退格配置、跟随系统键盘重复参数、可选双击保留标点删除；手势、迟到事件、取消、迁移与公开 UIA 实现单元测试通过。
- [ ] RC003 可选双击按标点删除完整真机验收：旧焦点/范围/异步选区失败记录保留；安装版 `8e7b68a` 的后续实体轮次已有 12 次 prepared 首删（提交 36–55ms）、7 次旧规则双击完成 passed，其中 2 次补回尾标点。首个事务因 `focus_changed` 在 52ms 取消，没有发送退格；返回前先按 Up/Left/Right，严格闲置首按 deferred。该轮不证明后来改用 Ctrl+Z 的日常预设、其它应用或全部生命周期通过，详见 [实体证据](Testing/evidence/eager-backspace-physical-round1-20260920.json) 与 [Bug 记录](Bugs/2026-09-20-punctuation-webview-focus.md)。
- 已取消、未交付：2026-09-20 的“当前短句及连续尾符删除”提案曾有文本编辑 19 项、事务 31 项测试 passed，用户随后明确取消；不作为待交付需求，不将此前旧规则实体结果记为此提案通过。
- 历史默认方案、现为可选配置：即时普通退格＋双击 Ctrl+Z 的引擎/手势测试 21/32、自家 WebView 实际 12→11→12，以及来源 `e8718f1` 的构建、正常升级、启动和配置回读 passed。用户实体试用后因快速连删会触发撤销而取消返回双击默认绑定，见 [原 Undo 方案实体记录](Testing/evidence/daily-undo-physical-20260921.json)。可选 Ctrl+Z 仍可配到任意可编辑格；其撤销范围由编辑器决定，不保证整段撤销，跨应用、RC001 和严格闲置首用不据此记为通过。
- [ ] Codex 输入框公开 UIA 文本范围兼容性及双击删除真机验收。


## 本地日常默认方案与配置入口修正（2026-09-18）

- [x] 音量±的单击/双击/长按开放编辑，保存、导入与运行时保留配置；界面改为信号需实测提示。定向持久化与编辑保存测试通过。
- [x] 日常默认方案应用与原生窗口回验：已在新版程序应用12键方案、生成最近备份且保留最初备份；读取持久化文件核对全部动作，语音配置哈希不变。原生音量编辑入口可打开，六格均启用；实体动作验收另列。
- [ ] 2026-09-21 最终日常预设实体验收：TV 单击查看改动 Ctrl+Alt+B、双击开关侧边栏 Ctrl+B、长按撤销 Ctrl+Z；返回单击普通退格，Double/Long 均 disabled，按住仍连续退格。Home、菜单、方向、音量 Ctrl+PageUp/PageDown、OK、Esc 与语音保持现有方案。来源 `b67a397` 的完整包已构建、正常升级、启动并通过可见页面应用；36 格零差异，首次备份保留、最近备份精确，应用列表/语音/音频端点不变，见 [最终包证据](Testing/evidence/final-profile-switch-install-20260921.json)。此前关闭返回双击后，用户确认快速连按与长按正常，见 [普通返回实体回验](Testing/evidence/plain-back-physical-20260921.json)；新 TV 实体三动作、完整 Codex 前台效果、严格闲置首用和 RC001 仍 deferred。官方来源及聊天/标签页边界见 [方案调研](docs/investigations/2026-09-20-codex-remote-defaults.md)。
- [ ] RC003 音量按键上报及全部默认动作实体真机验收。
- [ ] RC003 可选三键 HID lower filter：复用 QL-4 MIT 实现，仅将返回/音量±转为设备归因后的 F15/F13/F14；本地构建、自动验证、测试签名与签名/目录成员验证 passed，安装/加载/回滚、闲置首用和语音回归尚 deferred。默认主程序不依赖驱动、不自动发布，边界见 Testing/WindowsRc003Filter.md。
- [ ] RC003 返回/音量±免驱输入：2026-09-20 应用目录 GameInput 3.5.274 实验未取得三键事件或原始报告；前后实体上键对照 passed，三键读取 failed，系统 3.3 服务保持不变。不能据此宣称所有纯软件方案不可能，也不接入产品。见 Testing/WindowsRc003GameInput.md；驱动试验暂留在签名准备阶段，Secure Boot 保持开启。
- [x] RC003 Frida 独立监听实验：2026-09-20 用户明确授权的管理员 helper + 严格设备来源绑定实测，返回/音量±各 3 对 DOWN/UP，上键前后共 4 对；正常解钩、卸载、detach 和宿主存活复核 passed。未写报告、未改驱动或启动安全设置。本次独立实验未接入产品高亮/映射；随后获授权的集成见下一项。长按/冷首用/异常恢复/语音回归须另验。来源、GPLv3 边界与证据见 Testing/WindowsRc003Frida.md。
- [x] RC003 可选三键增强核心接入（用户明确授权；仅本机 RC003）：已安装完整包的显式管理员 Helper、三键来源及高亮、实际返回退格/按住连续删除、按住停止增强与普通键保留、严格就绪闲置首键、基础语音到虚拟声卡输出均 passed。原普通键和基础语音不依赖增强。实测包 manifest 为 `65de7e42…9678ae2`，后续 `9057f5de…7d8441` 日志改进候选已核对全部文件，主程序正常退出时 Helper 清理 exit 0 / error mask 0 已 passed；最新返回键候选的实体复验另列。见 [集成验收](Testing/WindowsRc003Input.md) 及 [三键](Testing/evidence/rc003-input-keys-20260920.json)、[长按/停止](Testing/evidence/rc003-input-hold-20260920.json)、[严格闲置首键](Testing/evidence/rc003-input-cold-first-20260920.json)、[基础语音](Testing/evidence/rc003-input-voice-20260920.json) 证据；不等同于 RC001 或识别文字端到端通过。
- [x] RC003 三键增强显眼开关：已在来源 `b67a397` 的安装版确认唯一入口位于图例上方，补齐返回、音量＋/－及管理员权限说明可见，无横向溢出。原生开启、等待真实中性状态、关闭及再次开启均 passed；停止 Helper exit 0 / error mask 0，主程序普通权限，用户方向键初始化后实际显示“增强已就绪”。组件 12、ButtonsPage 37、CodingPage 8 项定向测试 passed；浏览器、失败、断连等未实测分支仍只按组件证据记录。临时输入框已移除，生产资源与新版窗口均不存在该框。见 [安装与原生开关证据](Testing/evidence/final-profile-switch-install-20260921.json)。
- [ ] RC003 可选三键增强剩余验收：睡眠/崩溃恢复、多目标与共享宿主负对照、RC001、最终普通返回的严格闲置首用、新 TV/完整 Codex 预设和第三方识别文字端到端；旧规则已有本机实体首删/双击结果，关闭返回双击后快速连按已由用户确认，正常退出、清理退出状态与本轮升级/启动已 passed，不代表崩溃清理通过；运行中安装未替换主程序的问题仍未归因，不得以安装器返回 0 代替实际文件核验。范围和边界见 [集成验收](Testing/WindowsRc003Input.md) 与 [ADR 0003](docs/decisions/0003-rc003-optional-input-helper.md)。
- [ ] 本 fork 首个完整签名 Preview：Helper 构建与完整性门禁、独立 minisign 验证、own-repo 元数据和最新 PR CI 门禁已实现并完成本地对应检查；远端全流程、版本化 Notes、Tag 与公开资产尚未执行，不把本地签名测试记为发布通过。自动更新保持关闭，Authenticode 仍未启用。
- [ ] 并发映射变更一致性：保存、重置、导入已统一写盘与热加载事务，11 项设置测试 passed（含三类操作全部 9 种交错和失败不热加载）；来源 `6e49fd5` 的 0.2.7 安装版 4 对并发保存 IPC、恢复原配置和升级后文件保留 passed，实际导入对话框／重置按钮及实体按键回归 deferred。见 [Bug 记录](Bugs/2026-09-21-button-mapping-transaction.md)。
- [ ] 语音触发三键增强重复重绑：已用日志确认旧版 12 次语音均误重绑，Helper 已改读同锁发布的真实连接代次；清理前先发布失效，避免慢清理期间仍保留旧可用状态。补修后 19 项 RC003 定向及此前 1 项 IPC 契约测试 passed；新版实体语音／三键、断连及睡眠复验 deferred。见 [Bug 与证据](Bugs/2026-09-21-rc003-voice-helper-generation.md)。
- [x] Windows 宣传片本地初稿：64 秒 1080p 中文动效与原创配乐，包含默认键位、自定义、实验性增强权限、GPL、本项目／Mac 原版／Windows 上游链接及用户指定协作署名；全片明确交互动效示意。渲染与全帧解码 passed，源脚本和发布文案见 [marketing/promo](marketing/promo/README.md)。仅本地交付，未上传视频平台。

## 本地快捷键编码修复（2026-09-18）

- [x] 修复 PageUp/Down 注入缺失扫描码；原生消息对照证明修复前 scan=00、修复后49/51且Ctrl及释放边沿正确。Windows平台库145项通过。
- [ ] Codex前台遥控切换任务与闲置首用复测；返回/音量上报独立排查，保留用户自定义配置。
