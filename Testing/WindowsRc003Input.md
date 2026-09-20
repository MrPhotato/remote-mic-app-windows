# RC003 可选三键增强应用集成验证

日期：2026-09-20。范围见 [ADR 0003](../docs/decisions/0003-rc003-optional-input-helper.md)。
独立探针的三键可见性结果见 [WindowsRc003Frida.md](WindowsRc003Frida.md)，
不能替代本页的安装后高亮、映射和生命周期验证。

## 自动化结果

- passed：`cargo check --workspace --locked`。
- passed：Windows 库回归，157 passed、0 failed、6 ignored；包括三键来源合并、
  generation/sequence 拒绝、真实中性状态门禁、重复来源去重、断开及退出取消。
- passed：监督线程及启动路径定向测试先有 5 passed；后续清理退出状态日志改进后为 6 passed、0 failed。
  停止令牌与换代/转发共用短锁；阻塞读写在锁外。
- passed：前端全量 17 文件、145 tests；随后连接/监听前置提示定向 7 tests passed；修正未设置音量键的动作摘要后，ButtonsPage 定向 27 tests passed。
- passed（历史候选，用户已取消且未交付）：当前短句及连续尾符删除的文本编辑定向 19 tests、事务 31 tests；当时的 Coding 预设 24 tests，动作摘要与编辑页 39 tests。日志为 `backspace-text-edit-tests.log`、`backspace-transaction-tests.log`、`coding-preset-final-tests.log`、`clause-label-tests.log`。不能将这些结果作为后来即时退格＋Ctrl+Z 方案的引擎或实体验证。
- passed（历史撤销预设的前端定向）：返回双击 Ctrl+Z 的 `coding-profile` 与 `CodingPage` 共 24 tests，日志 `coding-preset-undo-tests.log`；不发送真实键，不证明即时退格引擎、撤销分组或实体效果。用户已取消此默认绑定，最终方案见文末。
- passed：本轮已实测安装包的 Helper 21 项 Python 测试及 JS observer 测试。包括父进程失联、
  租约超时、旧代/非法帧、初始按住、中性状态、源切换和发布租约/创建 capture 交错。
- passed：本轮已实测安装包的 Helper 96 个文件的 SHA-256、根 manifest 副本与对应源码一致性；
  x64/windowed/asInvoker 打包，不要求用户自行安装 Python。
- 已安装并实测的 manifest SHA-256：`65de7e42cb2a2845e14e8a790f2047a1fa37a751581657bba533c809f9678ae2`。
- 后续仅改进清理日志的候选 Helper：25 项 Python 模拟测试 passed，组件已构建，manifest 为
  `9057f5de09a13799452ccc8a1da59c0067d82b4f4e9eec059338fa35747d8441`。
  该候选完整包已于 `2026-09-20T13:22:21Z` 完成正常退出后的安装及全部 96 项文件校验，
  主程序与构建产物仅有 NSIS 标记三字节差异。候选同时包含 WebView 焦点修复和来源失效时取消文字编辑；
  已重新以普通权限启动，增强 Helper 已认证并加载，等待真实中性状态及实体复验。
  后续正常退出时新清理日志已实际确认 exit 0 / error mask 0，见本文升级节；不能替换下述旧包的按键/语音/停止验证证据。
  候选安装摘要及删除回归边界见 [WebView 焦点证据](evidence/punctuation-webview-focus-20260920.json)。

完整编译和测试日志保存在 ignored `target/local-launch/rc003-integration/`；
Helper 构建日志位于 ignored `target/rc003-helper/logs/`。
启动阶段脱敏证据见 [rc003-input-startup-20260920.txt](evidence/rc003-input-startup-20260920.txt)。
本轮实体按键及 UI 高亮脱敏证据见 [rc003-input-keys-20260920.json](evidence/rc003-input-keys-20260920.json)。
实际退格、长按与按住停止增强的脱敏证据见 [rc003-input-hold-20260920.json](evidence/rc003-input-hold-20260920.json)。
遥控器到虚拟声卡输出的语音脱敏证据见 [rc003-input-voice-20260920.json](evidence/rc003-input-voice-20260920.json)。
严格就绪闲置首键的脱敏证据见 [rc003-input-cold-first-20260920.json](evidence/rc003-input-cold-first-20260920.json)。

## Windows 实机验收

本轮已实测本地包已完成 96 个组件逐项散列检查，并在应用预先正常退出后完成主程序替换。
已验证新主程序以普通权限运行，显式启用增强后建立 IPC，并完成 capture attach/load。
应用仍运行时的首次覆盖未替换主程序，不能记为升级通过。
随后实体三键边沿与高亮、返回键实际退格、按住连续删除及按住停止增强已通过；
基础语音到虚拟声卡输出及严格闲置首键也已通过；语音识别到文字及其它未覆盖生命周期行为仍待验。

| 项目 | 状态 |
| --- | --- |
| 完整本地包安装、组件完整性与普通权限主程序启动 | passed（应用预先正常退出后；主程序及 96 个组件已核验） |
| 显式启用至 Helper 建立 IPC | passed（修复后新主程序；此前包退出码 21 为 failed） |
| 当前选中 RC003 来源的输入与真实中性状态初始化 | passed（generation 1；真实中性帧 sequence 1；其它设备负对照另计） |
| 返回、音量加、音量减高亮的成对 DOWN/UP | passed（各 3 对，UI 高亮逐一对应；上键前后共 2 对正对照） |
| 返回映射派发与单击手势 | passed；首轮只有派发证据，随后文本框试验确认实际退格 |
| 实际退格效果与按住连续删除 | passed（单击 200→199，按住 199→183；17 对可信键盘事件与文本变化对应） |
| 基础普通退格已就绪后闲置首用 | passed（旧动作配置：ready 后 225.659 秒首个实体键为返回，183→182；没有方向键预热）；即时退格＋双击撤销配置另计 |
| 提前首删与旧规则双击，安装版 `8e7b68a` | 后续 12 首删、7 双击 passed；首个事务 focus_changed 取消；该轮严格闲置首按 deferred |
| 删除当前短句及连续尾符提案 | 用户已取消，未交付；保留历史测试，不作为待验交付项 |
| 即时退格＋双击 Ctrl+Z | 历史 `e8718f1` 方案；软件实际输入及安装回读 passed，用户实体试用后因快速删除触发撤销而取消默认绑定，可选能力保留 |
| 最终普通返回（Double/Long disabled，按住重复） | 当前安装版关闭返回双击后，用户确认快速连按与长按 passed；严格闲置首键、其它应用与 RC001 deferred |
| 最终 Coding 预设的实际 Codex 效果 | 预设/页面定向 24 tests passed；TV 改动/侧边栏/撤销及完整新配置的安装和实体效果 deferred |
| 图例上方的增强开关原生启停 | 组件及页面定向 57 tests passed；新入口安装后显式授权、初始化、状态与正常停止释放仍 deferred |
| Helper 停止时仍按住的键清理，不触发取消后的动作 | passed（合成 UP 1 次，停止后 4 秒无继续删除） |
| 主程序正常退出与升级 | 正常退出 passed；运行中覆盖首次 failed；活动 Helper 正常退出并确认 exit 0 后安装 `8e7b68a` passed |
| 活动 Helper 退出与宿主存活 | Helper 退出 passed；此前 13 个宿主 PID 均仍存在，创建时间/句柄身份未核验 |
| 基础语音回归：遥控器到虚拟声卡输出 | passed（16 kHz 单声道，提交 72,480 样本，finish 后 queued=0） |
| 第三方语音识别到文字端到端 | 未验证（接收应用输入选择尚未确认，无识别文字证据） |
| RC001、另一台 RC003、多目标选择、共享宿主另一设备负对照 | deferred |
| 睡眠唤醒与真实宿主异常退出 | deferred |

### 首轮实体三键与高亮

- passed：`2026-09-20T12:43:05Z` 至 `12:43:14Z`，用户实体按键并确认界面有响应。
  返回、音量加、音量减各观测到 3 对完整 DOWN/UP，上键前后共 2 对；
  22 个语义边沿与 22 次 UI 高亮开关逐一对应，最终无残留高亮。
- passed：第一下上键期间接收真实三键中性帧，`12:43:05.047Z` 接受
  generation 1 / sequence 1 并进入 ready。窗口末 Helper 状态为 ready、无错误，
  累计 19 份状态报告、18 个三键边沿；此计数不包含普通上键正对照。
- 动作边界：日志记录 3 次 `map_fire button=Back trigger=Single action=normal_backspace`，
  前端同时收到 3 次 Back/single 手势；这只证明派发。`inputLengths` 仅有初始长度 200，
  没有按键后长度变化记录，因此本轮不能证明真实退格生效，也不能宣称动作验收通过。
- 闲置边界：capture 在 `11:10:45.982Z` 加载，至 `12:43:05.047Z` 才首次获得中性状态，
  中间约 92 分 19 秒处于 awaiting_neutral。它证明长时间等待后的首次初始化仍可完成，
  不是已就绪后的普通闲置首用测试，也不是输入延迟证据。
- 上述首轮未测试语音、长按或实际文本删除；后续长按和停止增强的独立结果见下一节。

### 实际退格、长按与按住时停止增强

- 证据仅统计 `hold-observation.json` 中 `preparedAt`（`2026-09-20T12:44:55.247Z`）
  之后的窗口，排除该文件保留的上一轮事件。用户确认本轮长按有效。
- passed：单击返回使测试框长度从 200 变为 199，随后按住返回从 199 变为 183。
  共记录 17 次 Back/single 手势、17 对浏览器 `isTrusted=true` 的退格 DOWN/UP、
  17 次一字符删除；包含单击删除 1 字及按住期间删除 16 字。仅保存长度，不保存文本内容。
- passed：测试控制器在返回仍按住时于 `12:45:50.837Z` 主动请求停止增强，
  `12:45:50.840Z` 收到一次引擎合成 UP 并清除高亮，`12:45:50.916Z` 日志确认
  `helper_exited=true`，`12:45:50.918Z` 返回 stopped。此时累计 22 份 Helper 报告、
  21 个三键边沿；物理释放在停止后未被捕获，不能将合成 UP 记作物理 UP。
- passed：停止前后测试框保持 183 字，停止完成后再观察 4.010 秒仍为 183，
  请求停止后无新增退格键盘事件或长度变化。之后右键 3 对、左键 2 对及高亮仍正常；
  窗口内停止前还有左键 1 对正对照，合计 16 个语义边沿和 16 次对应高亮变化。
- 进程边界：主程序仍运行，原 Helper 已消失；此前 13 个宿主 PID 在复核时均存在。
  创建时间为空，未持有句柄核对身份，因此这里只证明 PID 存在，不能排除 PID 复用，
  也不能把它写为宿主身份级验证通过。
- 闲置边界：上轮最后实体 UP 在 `12:43:14.866Z`，本轮首个返回 DOWN 在 `12:45:48.405Z`，
  相隔 153.539 秒；距上轮最后 Back/single 手势为 159.566 秒，实际退格成功。
  但 `12:45:46.887Z` 已先按左键，早于返回 1.518 秒；此窗口不能证明严格闲置首键，后续独立结果见下节。
- 停止是本轮控制器预设的清理验证步骤，随后用户第二次尝试无响应发生在增强已停止的状态。
  控制器已重新启用增强，后续不再自动停止；不能将这次按设计停止后的无响应归为新输入故障。
  上述按键窗口未测试语音；后续基础语音结果见下一节，活动 Helper 的正常升级仍未覆盖。

### 基础语音到虚拟声卡输出

- passed：用户完成一次语音键试验。`2026-09-20T12:49:46.134Z` 收到开始控制，
  `12:49:46.316Z` 请求音频会话，`.318Z` begin completed/passed，`.319Z` 虚拟声卡输出流启动。
  音频工作线程配置为 WASAPI render、16,000 Hz、单声道；不记录端点身份或语音内容。
- passed：`12:49:50.766Z` 收到停止控制，`.929Z` 请求结束，`.930Z` finish completed/passed，
  共提交 72,480 个样本，queued=0；按采样率换算约 4.53 秒音频，不把它当作端到端延迟。
- 增强在该语音生命周期中从 generation 2 切换到 3，`12:49:46.483Z` 完成 capture attach/load，
  `12:49:50.803Z` 收到中性状态后恢复 ready，`.804Z` 接受 sequence 1；主程序持续运行。
  这是可选增强的重绑定观察，不代表基础语音依赖 Helper。
- 未验证：接收应用的输入设备和识别文字。用户随后询问是否应选择 CABLE Output 为输入，
  因此本轮只记录遥控器到虚拟声卡输出链路 passed，不能宣称第三方语音识别或文本输入端到端通过。

### 严格就绪闲置后的首个返回键

- passed：语音释放控制在 `2026-09-20T12:49:50.766Z`，Helper 在 `.803Z` 恢复 ready，
  音频会话在 `.930Z` 完成。之后至 `12:53:36.462Z` 的第一个实体按键为返回，
  日志未出现中间按键事件，用户确认该次确为闲置后的首键，没有方向键预热。
- 距 Helper ready 为 225.659 秒，距前一次语音释放为 225.696 秒；这次是真实就绪后的闲置，
  与首轮等待首次中性帧及前一轮先按左键的情形分开记录。
- passed：返回 DOWN/UP 在 `12:53:36.462Z` / `.522Z`，高亮同期打开和清除；
  记录一次 `normal_backspace` 派发及一对可信退格 DOWN/UP，测试框在 `.465Z` 从 183 变为 182。
  Helper 保持 generation 3，报告数 24→26、边沿数 21→23，最终仍 ready、无错误。
- 范围只覆盖本机 RC003 的这一次约 225.7 秒闲置首用；睡眠、宿主崩溃、其它型号、全部闲置时长
  或识别文字端到端不能由本次结果推定。

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
- 缺陷复现、路径转换回归及新包实际提权启动至 IPC 已 passed；随后实体三键及高亮结果见上节，
  返回实际退格、按住停止、基础语音输出及严格闲置首键也已单独记录；识别文字端到端仍未验证，不能以组件构建或自动化测试替代。

多 RC003/残留设备实例目前保持唯一目标门禁；拒绝歧义不会扩大到整个宿主。
本模式仍依赖非公开 Windows UMDF 实现与管理员 Helper，不是普通权限公开 API 方案。
测试期间不修改驱动、Secure Boot 或测试签名状态；先前签名实验的证书状态另计。


## 2026-09-20 首击提前退格候选：正常升级与启动

源代码提交 `8e7b68acf1af3ab0ce09b21615416af268684c2b` 的本地 NSIS 已构建、安装并启动，未推送或发布。完整散列、安装回读与当前验收状态见 [安装证据](evidence/eager-backspace-install-20260920.json)。本轮实际核对主 EXE 与构建产物仅有 NSIS 的三字节 bundle 标记差异；Helper manifest 与构建一致，96 个文件全部通过、总文件 97（含 manifest）。不能仅凭安装器返回 0 判定安装成功。

升级前使用应用正常退出事件，主进程确认退出；同一运行轮次的日志出现 helper_exited=true、helper_process cleanup_exited exit_code=0、cleanup_result passed / helper_cleanup_completed / error_mask=0。这是 `9057f5de…7d8441` Helper 的实际正常清理结果，不是强杀或单元模拟；没有单独采集驱动宿主存活证据，不扩展为崩溃/睡眠恢复通过。

新主程序 startup connection/audio ready，voice idle，主程序 main_elevated=false；返回单击/双击映射保持 normal_backspace/delete_to_punctuation。增强通过 manifest 预检并启动，首次状态 waiting/awaiting_neutral，随后 `15:32:23.162Z` 实体上键释放完成初始化。该安装快照中的 physical_rc003_return=deferred 表示当时状态，后续实体结果见下一节；未经预热的严格闲置首按与其它应用 UIA 兼容性仍待验证。软件函数实测、取消清理及边界补回的结果见 [WebView 执行证据](evidence/optimistic-backspace-webview-20260920.json)。

## 安装版旧规则实体返回测试与后续需求边界

`8e7b68acf1af3ab0ce09b21615416af268684c2b` 在 `15:35:45.697Z–15:36:12.082Z` 接收 20 对实体返回边沿，派发 13 次 Single 和 7 次 Double。首个事务在 52ms 因 `focus_changed` 取消，没有发送首删；后续 12 次 prepared 首删提交耗时 36–55ms，7 次双击均以实际文本精确核验完成，其中 5 次删除剩余后缀、2 次按旧规则补回被首删的尾标点。测试框记录 17 对可信 Backspace DOWN/UP，补回另由 Unicode 提交与最终文本检查证明。见 [本轮实体证据](evidence/eager-backspace-physical-round1-20260920.json)。

记录中的文本长度增加包含人工重新填充，不能全归因于遥控器。该轮返回前先按了 Up、Left、Right，严格闲置首键条件未满足，记 deferred；此前基础普通退格的严格闲置通过也不自动证明新提前首删路径。首次焦点变化的具体原因未确定。

用户曾要求双击删除当前短句及连续尾符；该候选有文本编辑 19 项、事务 31 项测试 passed，但随后被用户取消，未构建安装交付。旧规则的两次尾符补回不属于短句提案通过，也不能转为后来 Ctrl+Z 方案的证据。

当时的后续要求是返回单击立即普通退格、按住重复、双击发送 Ctrl+Z：首击已经删除，第二击触发编辑器撤销；具体撤销范围与分组由当前编辑器决定，不承诺整段撤销或精确恢复某次首删。其软件输入和 `e8718f1` 安装记录见下文，随后用户实体试用取消该默认绑定。独立可选按标点删除功能不因此改成 Ctrl+Z。

历史 `e8718f1` 预设保留音量＋/－ Ctrl+PageUp/PageDown，TV 当时为侧边栏/改动/终端，返回双击为 Ctrl+Z。最终 TV 与返回方案见文末；首次及最近备份保留，升级不会自动覆盖当前配置。组合键官方来源和聊天/标签页范围见 [默认方案调研](../docs/investigations/2026-09-20-codex-remote-defaults.md)，不宣称只切 Agent。


## 普通退格与撤销实际软件验证（2026-09-21）

[实际执行证据](evidence/ordinary-backspace-undo-webview-20260921.json)：自家固定 WebView 输入框由真实 SendInput 普通 Backspace 删除一次（12→11），随后 Ctrl+Z 恢复原值（11→12），最终焦点及光标 12/12 正确。探针只读 UIA 核验，不进入 backspace_transaction 或选择删除；首删观察 30ms、总输入动作 54ms，不能当作遥控端到端或冷首按延迟。手势 32 tests、引擎路由 21 tests passed，覆盖无 UIA 能力也直接首删/一次撤销、普通重复及原标点路径保留；预设页面 24、按键编辑页 27 tests passed。实体 RC003、新安装及 Codex 实际动作仍待验。


2026-09-21 配置提示修正：ButtonsPage 定向 34 tests passed，覆盖返回 disabled/Ctrl+Z/标点/其它双击的准确时序说明，以及电源单/双/长任意格配置和关闭 Ctrl+Z；测试不注入按键。新版原生页面回验另计。

## 撤销默认方案安装与配置回验（2026-09-21）

来源 e8718f1372cecf6085c1bda8b6b43e5ee39b35b6 的本地 NSIS 构建、正常升级、启动 passed。旧增强 Helper 正常退出 exit 0 / error mask 0；新主程序文件与产物仅 NSIS 三字节标记不同，96 个 Helper 文件全部核验通过。实际应用默认方案后，12 键 36 格逐项回读符合预期，当前配置精确备份；此运行的 WebView 首次备份原先不存在，已按当前配置新建，不声称恢复了不存在的旧备份。应用列表、语音热键与音频端点保持一致。原生返回编辑器实际显示首击立即退格／第二击 Ctrl+Z，旧删除说明和错误等待提示均不存在。证据见 [安装及原生回验](evidence/daily-undo-install-20260921.json)。新包实体键与严格闲置首按另验。

## 最终普通返回与 TV 三动作方案（2026-09-21）

用户在上述安装版的实体试用中发现快速连续退格会触发双击撤销，取消返回双击默认绑定；原结果保留于 [Undo 默认方案实体记录](evidence/daily-undo-physical-20260921.json)。通过可见编辑器只把返回双击改为 disabled，保留普通退格及按住重复，用户随后确认快速连按与长按正常，记 passed，见 [普通返回实体回验](evidence/plain-back-physical-20260921.json)。这属于原安装版上的配置回验，不是最新完整包的安装证明；严格闲置首键、其它应用和 RC001 仍 deferred。

最终用户批准方案已更新到源码：TV 单击 Ctrl+Alt+B 查看改动、双击 Ctrl+B 开关侧边栏、长按 Ctrl+Z 撤销；Back 单击 normal_backspace、Double/Long disabled，按住仍重复。用户不使用终端，Ctrl+Z 仍可配置到任意可编辑格或关闭；Home、Menu、方向、音量、OK、Power 和语音保持原方案。最终预设/页面定向 24 tests passed，普通返回快速重复手势定向 33 tests passed；最新包尚未构建安装，TV 三动作与完整 Codex 效果不可记 passed。

图例上方的显眼增强开关已完成组件 12、ButtonsPage 37、CodingPage 8 项定向测试。日志为 `rc003-enhancement-switch-component-tests.log` 与 `rc003-enhancement-switch-tests.log`；首轮仅新增测试的局部变量作用域错误已修复并单独重跑组件通过。开关原生启停及显式授权、初始化、断连状态和正常停止释放仍待新安装版验收，不能以旧按钮或后端既有证据代替。
