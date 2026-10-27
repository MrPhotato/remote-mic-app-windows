# Windows 默认麦克风选择与恢复

- 日期：2026-10-19。
- 状态：连接触发版本的自动化、浏览器布局与真实接口释放／重选往返 passed；安装版实体连接、断开与重连 deferred。下方保留 0.3.10 的历史验收，不将旧启动触发结果当作新生命周期通过。
- 授权：仅默认录音设备切换与恢复，见 [ADR 0004](../docs/decisions/0004-default-microphone-session.md)。

## 2026-10-20 固定虚拟目标、普通麦立即使用

用户反馈旧实现将普通麦保存为遥控器目标，CABLE 被挤入其余列表。现在按 Capture 类型固定分组：
虚拟麦在目标区，普通麦在下方；当前实际使用项采用既有柔和靛紫底色／边框，无绿色底色、额外徽标或外发光。
正常成功反馈仍沿用 320ms 轻微回落，但只作用于刚选中的行；减少动态效果时不位移。

普通麦按钮改走独立 `use_microphone_now`：立即切换，保留虚拟目标，成功后持久化结束恢复所有权，
不在断连或退出时反向覆盖这次手选。显式重新应用 CABLE 才恢复当前进程的连接接管。
旧物理目标仅在唯一已验证虚拟线路存在时迁移；所有自动应用和离线保存路径都拒绝普通 Capture，
包含旧设备启动时缺失、之后重新出现的情况。没有虚拟设备时展示缺失状态，不把物理目标误报为 CABLE 已就绪。

| 验证 | 结果 | 边界 |
| --- | --- | --- |
| 前端组件与类型 | passed | 最终 55 项 ConnectionPage；推送前完整前端 314 项 passed，类型检查及生产构建通过 |
| 宿主与 Windows 平台状态机 | passed | 宿主含 runtime-simulation 35 项，平台 27 项；包含手选提交后最终读回被外部更改、首次连接也不抢回。真实接口 1 项默认 ignored，执行结果下列独立记录 |
| 实际 Edge 深浅主题 × 正常／减少动画 | passed | 1030×734 四组合，无页面错误／溢出；动效仅选择行，初始／轮询不动，减少动画为零；最终源码摘要一致 |
| Windows 真实默认麦往返与手选提交 | passed | 已安装应用先正常退出。独立 COM 读三种 Capture／Render；CABLE 临时选择→手选回原物理→释放后保留→新 CABLE 会话恢复新基线→退出，全部实际读回一致，三个 Render 始终不变 |
| 本地新版完整升级及原生页面 | passed | 静默安装返回 0，主程序仅 NSIS 标记差异，96 个 Helper 完整。启动将旧物理目标迁移到已验证 CABLE；原生按钮普通麦 A→B→A 后重应用 CABLE，CABLE 全程留在上方，高亮随实际当前端点变化 |
| 实体遥控器连接／断连、RC001／RC003 听写 | deferred | 当前遥控器未就绪；不以接口往返、浏览器或离线安装结果替代 |

本地证据在 `target/microphone-fixed-target/`：`browser-check.json`、20 张浏览器截图、
`live-probe.json`、`live-round-trip.log` 与独立音频读回；宿主／平台日志在
`target/microphone-selection-motion/{host-target-tests,platform-target-tests}.log`。组件最终日志为
`target/microphone-connection-ui/pinned-tests.log`。详细文件被 Git 忽略，不提交设备身份。

本机安装构建源为 `95009fa292d9b7abedcbf62c2b80c1d070b77e81`，仍为 0.3.12 本地测试包。
安装包 `target/microphone-fixed-target/SayAll-Windows-0.3.12-local-fixed-target.exe`，SHA-256：
`66C4B1F7AEA40B894B3BDB8385789D0A850478CE940EB74CE6BEFF171DACBBA9`。
安装阶段原三份配置哈希不变；启动修正旧物理目标时仅改变 `target_microphone_id` 与
`target_microphone_name`。两次普通麦点选及重应用 CABLE 后，虚拟目标始终不变，其余设置、按键映射与语音快捷键配置不变。

三次原生点击均有 `microphone_selection_feedback` 请求及 settled 完成，约 326–336ms。
每步经独立只读 Core Audio 核对：普通麦确实立即生效，最后三个 Capture 恢复安装前原麦，
三个 Render 全程不变。CABLE 离线重应用只保存目标并恢复等待连接，不假装实际使用 CABLE。
原生截图与辅助功能树目视确认柔和靛紫高亮、按钮“使用中”／“已选择”、无绿色选中底色或额外徽标。
物理设备可因枚举的当前优先排序在下方组内换序；本次固定的是虚拟／物理分组，不声称系统枚举顺序恒定。
新增证据 `installed-highlight.json`、`native-ui-check.json`、`after-array.json`、`after-original.json`、
`after-cable.json` 及生产诊断日志均在上述本地目录。上述安装验收阶段未推送、创建 Tag 或发布 Release。

2026-10-20 推送前复核：使用项目指定的 pnpm 10.15.0 执行 `scripts/ci-preflight.ps1`，六步全部 passed；
前端 314 项、Rust workspace 432 项 passed，9 项 ignored 不计为通过；格式检查及前后端构建检查通过。
应用源码与上述本机安装构建源一致，之后只补充验证记录和当前选择行为说明。
完整检查日志保留在本地忽略目录 `target/push-0312-20261020/preflight-pinned.log`。

## 2026-10-20 按钮状态与选择反馈（历史，目标可移动行为已由上节取代）

用户回验后取消额外勾号徽标。当前系统麦克风只用行高亮和按钮“使用中”表达；离线保存的目标
显示“已选择”，其他设备仍可“选择”。当前设备位于其余列表时仍能设为目标；仅其他 Capture 角色
外部改变时，目标保留“重新应用”，不会因 Console 设备相同而禁用修复入口。

成功确认后目标行按既有 `--motion-slide`（320ms）和 `--ease-out-quint` 轻微回落一次，只动画 transform；
色彩沿用 200ms 状态渐变。成功反馈与绿色高亮相互独立，保存偏好成功不暗示系统已切换。
失败、首次读取和轮询不播放成功反馈；新选择取消前一次动画，卸载清理。减少动态效果时直接更新。
结构化事件 `microphone_selection_feedback` 记录请求、完成、取消及减少动态效果分支，不含设备身份。

- 48 项 ConnectionPage 测试、类型检查、前端生产构建与格式检查 passed。
- 深浅主题 × 正常／减少动画四组浏览器逐帧检查 passed：正常动效 320ms 后稳定、连选取消前次、减少动画零位移，首次／轮询不播放成功反馈。1030×734 布局无溢出或裁切，截图目视通过，收据记录源码摘要。
- 修订版本机静默升级、实际按钮与动效回验 passed：正常退出后安装返回 0，主程序字节仅 NSIS 标记不同、96 个 Helper 文件完整；原生页面截图和辅助功能树确认无额外徽标、按钮“使用中”／“已选择”正确。
- 本机实际点击当前物理麦设为目标，再选回 CABLE：两次选择保存成功，日志均记录反馈 requested→settled（各约 336ms），页面状态对应实际结果。测试后原三份配置哈希完全一致，三个 Capture 与三个 Render 默认值全部与安装前相同。实体连接／断连及硬件听写仍沿用下方 deferred 边界。

本次测试日志在 `target/microphone-connection-ui/motion-tests.log`、`motion-build.log`；
浏览器证据放在 `target/microphone-selection-motion`，本机包及安装证据放在 `target/microphone-button-feedback`。
沿用下方本地构建／验证命令；未变化的设备生命周期和 Helper 复用已有证据。

修订构建源 `6f60df75f8163205d3372661acdf82534e5f6a22`；仍为 0.3.12 本地包，未推送、发布或上传。
安装包 SHA-256：`40915FA71B87D3A8335A6A1731F88159CF5D003444AF99139F8CA430118F2DBF`。
本机证据：`installed-highlight.json`、`installed-highlight.log`、`native-ui-check.json`、`interaction-preservation.json`，
均位于 `target/microphone-button-feedback`，不提交个人设备身份及配置。

## 2026-10-20 首版高亮的历史验收

按宿主实时快照的当前系统默认 Capture / Console 端点显示绿色底色、边框与“✓ 当前生效”，
目标区和其余列表采用同一规则。保存目标、等待连接和其他录音角色的状态不替代当前默认端点；
读取失败、未知或正在选择时不猜测。只改变展示，不执行设备切换或录音；不代表每个应用独立选择的输入。

| 项目 | 状态 | 证据或边界 |
| --- | --- | --- |
| 目标／其余当前设备、连接／断连轮询、外部变化、未知值与读取失败恢复 | passed | 44 项 ConnectionPage 测试；沿用防止迟到读取覆盖新选择的机制；类型检查与前端构建通过 |
| 1030×734 深浅主题、目标／其余四组布局 | passed | 实际 Edge 渲染，截图目视复核，单一高亮、名称换行、按钮可见、卡片无裁切与页面无错误 |
| 本机安装版实际默认设备高亮 | passed | 0.3.12 本地更新后，通过原生窗口截图及辅助功能树确认：当前物理麦在其余列表绿色高亮且仅有一个“当前生效”，保存的 CABLE 目标保持中性 |
| 安装升级及配置／默认音频保留 | passed | 应用自有事件正常退出，静默安装返回 0，主程序除 NSIS 标记外完全匹配构建；96 个 Helper 文件及清单核对，三份配置哈希相同，三个 Capture 与三个 Render 默认值均与升级前相同 |

本地证据为 `target/microphone-connection-ui/highlight-tests.log`、`highlight-build.log`、
`highlight-layout.json`（含界面源文件摘要）及 `highlight-*.png`，均不提交设备身份或个人配置。
展示类别变化记录 `microphone_active_display`，只记录 target／other／not_listed／unconfirmed，不记录端点 ID 或名称。

本地构建源 `77afbffa3efc365835e1877386f66b33e133ccab`，保持 0.3.12 本地版本，未推送或发布；
原安装包已保留在忽略的证据目录。新安装包 SHA-256：
`71A45B257D244FC0F65B1F25AE5F980A34F67B15D492BF64D4C053B750B46382`。
实际安装／启动回执 `installed-highlight.json`、原生验收 `native-highlight-check.json`，
生产日志已观察到 `microphone_active_display ... system_default_other`。
实体遥控器连接触发的默认设备切换仍沿用下一节的 deferred 边界，本次没有用界面高亮替代连接／断连实测。

验证命令：`npm test -- src/pages/ConnectionPage.test.ts --maxWorkers=2 --minWorkers=1`、
`npm run build`、`cargo fmt --all -- --check`；本地完整包用
`scripts/build-local.ps1 -Installer -SkipTests -SkipHelperBuild`，未变化的 Helper 复用既有验证，
并重新执行 `scripts/verify-rc003-helper-bundle.ps1` 核对最终主程序内嵌清单和完整载荷。

## 2026-10-19 连接触发生命周期

仅在所选遥控器同一 `connection_generation` 的 `Ready`、`Streaming`、`Draining` 阶段持有默认麦克风切换；
单次语音开始、收尾不重建基线。断开、连接中、休眠或失败时释放并恢复，新的连接代次就绪后重建。
离线只保存目标，退出执行兜底恢复；已观察到的外部手动选择在重连时仍保留。

| 项目 | 状态 | 证据或边界 |
| --- | --- | --- |
| 宿主连接门控、目标偏好及恢复流程 | passed | 28 项后端定向测试；离线不切换，同一连接代次连续持有，新代次释放／重建，外部修改保护 |
| Windows 默认麦克风平台层 | passed | 23 项定向测试通过，1 项真实接口测试默认 ignored；真实执行结果单列，不把 ignored 计为通过 |
| 等待连接、离线保存、状态刷新及失败文案 | passed | 44 项 ConnectionPage 测试；类型检查通过。返回目标不一致或保存失败不显示成功，浏览器预览不伪造系统切换 |
| 1030×734 深浅主题布局 | passed | 两主题各一张截图并目视复核；卡片约 508px 高，说明、设备名与选择按钮无裁切、溢出或遮挡，无页面错误。原生最低 1029×732，不测试手机尺寸 |
| Windows 真实接口释放／重选往返 | passed | 当前物理 Capture→release_session→基线恢复，再选已验证 CABLE→释放→恢复；重复释放正常，最终 restore 关闭后拒绝新选择。每步独立 COM 读取三个 Capture，三个 Render 全程不变；scope_cleanup=true，耗时 0.47s |
| 安装版离线启动 | passed | 2026-10-20 完整安装 0.3.12 后，日志确认 remote_not_ready、等待连接；独立读取三个 Capture 与三个 Render，均与升级前基线一致 |
| 安装版目标保存、实体连接后切换 | deferred | 仍需实际遥控器就绪后验收；不以离线启动通过替代 |
| 安装版实体断开／重连、退出兜底及听写 | deferred | 须在实际安装版观察系统 Capture；不能用接口往返或组件模拟替代，RC001／RC003 分别验收 |

本次浏览器证据：`target/microphone-connection-ui/layout-check.json`（含对应源码 SHA-256）及同目录深浅截图；
真实接口证据：`target/taskbar-icon-20261019/microphone-live-round-trip.log`。这些本地证据均被 Git 忽略。
未对系统蓝牙执行开关、重配对或适配器重启，也未为本次界面测试启动生产应用。

## 0.3.10 历史验证范围

真实设备检查须保存 Console、Multimedia、Communications 三种 Capture 和 Render 默认值。
选择物理麦克风、选择已验证的虚拟麦克风，每次通过独立 Core Audio 读取确认三种 Capture 角色；
最后恢复并与原始值比较，全程核对 Render 默认值未变。仅调用选设备接口，不录音、不播放、
不操作蓝牙。独立测试使用忽略目录中的专用恢复记录，不改安装版配置。

| 项目 | 状态 | 证据或边界 |
| --- | --- | --- |
| 状态机、部分失败回滚、恢复记录、外部变化、超时取消 | passed | 18 项确定性状态机测试，含 Console/Multimedia 联动；真实接口探针独立执行 |
| Capture/Render 偏好分离、启动推断、保存失败、并发退出 | passed | 13 项 host 选择测试、2 项退出并发测试；最终 workspace 共 388 passed、9 ignored |
| 所有 Capture 可选择、失败及过期状态、浏览器模拟 | passed | 36 项 ConnectionPage 测试；完整前端 284 passed，类型检查通过 |
| 两尺寸、两主题、物理/虚拟目标共 8 组界面 | passed | 独立无头 Chrome，无横向溢出；两类目标往返选择有效，诊断默认折叠 |
| Windows 真实选择、逐角色读回、恢复 | passed | 最终实现 08:29:21 UTC，生产 runtime 普通麦→CABLE→恢复；独立 COM 枚举器三 Capture 值匹配，三个 Render 值始终不变；恢复后拒绝再次选择 |
| 升级安装与配置保留 | passed | 旧版正常退出，0.3.10 安装返回 0；三份配置 SHA-256 不变，主程序字节与 98 个增强组件核验通过 |
| 安装版启动、手动选择、托盘退出恢复、实体听写 | passed | 2026-10-19 用户完成手动验收，明确确认“选择、听写和退出恢复都正常” |

## 用户可见行为与边界

启动读取保存的目标麦克风，遥控器未就绪时不切换系统默认录音设备。首次升级没有独立 Capture 偏好时，
只从现有已验证虚拟线路推断；设备缺失或关联不唯一时不猜测。离线选择 CABLE 仅保存偏好，界面显示“等待遥控器连接”；
连接就绪后才应用，断开后恢复，新连接代次重新建立基线。普通麦克风选择则立即生效，保留已保存的虚拟目标并暂停自动切换；
后续断开或退出不还原该手动选择。选择普通麦克风不会把遥控器音频写入物理输入；应用 VB-CABLE 目标前须确认遥控器传声线路可用。

关闭窗口继续驻留托盘，不视为断开。真正退出、正常升级退出对仍持有的自动切换执行恢复兜底，并等待收尾；不强杀进程。
用户在 Windows 或其他程序中更改过的默认录音角色不覆盖，重连也不自动抢回；须用户显式重新选择目标。
目标应用须使用“系统默认麦克风”
才能跟随；固定选择了 CABLE 或其他麦克风的应用保持自身设置。

进程崩溃、断电、设备拔除与 Windows 接口失效无法保证立即恢复；保留恢复记录供后续安全处理。
未公开接口没有 Windows 兼容性保证，不把一次本机检查宣称为其他设备或系统版本通过。

本机实际观察到设置 Console 会同时改变 Multimedia；不能把后者误当成没有改动的角色。
实现先记录两者可能的变更，再读取实际结果；外部改动任一项时放弃这对角色的恢复资格。
全部恢复操作结束后再次读回，不提前清除记录。无法同时维持不同原值时保留未完成状态；
这一失败分支通过确定性联动模拟覆盖，不为了构造异常而修改用户设备。

## 证据存放

完整日志、模拟截图与独立探针收据放在被忽略的 `target` 目录；仓库只记录脱敏结论。
不提交端点标识、设备身份、语音内容、真实配置、个人路径或授权凭据。

## 0.3.10 本地升级

- 功能提交 `65cfcad`；构建源 `b15dd97872a6b7536c1198db17264e34bb20901f`，仅额外更新版本元数据。
- 前端类型检查、生产构建、Rust release 和 NSIS 构建 passed；未改动的增强组件复用既有校验清单。
- 升级前留存诊断，核对旧版进程路径并请求应用自身退出；未强杀、启动新版、重连遥控器或操作系统蓝牙。
- 安装返回 0，安装后版本为 0.3.10；主程序与构建产物仅存在预期的 Tauri NSIS 标记替换，98 个增强组件校验 passed。
- 正常用户环境的 `button-mappings.json`、`settings.json`、`voice-hold-hotkey.json` 在升级前后哈希一致。
- 本地安装包：`target/update-0310-20261019/SayAll-Windows-0.3.10-x64-setup.exe`，50027068 字节。
- 安装器 SHA-256：`f2f0bc722e4c33a9b72272f2013b7e96d736fd65f9d129a8829ee95ddb255e01`。
- `passed`：2026-10-19 用户按安装版验收步骤测试普通麦与 CABLE 选择、实体听写及从托盘退出恢复，并明确确认“选择、听写和退出恢复都正常”。这是用户实际使用确认，与前述独立接口探针分别记录；应用运行后的合法偏好保存与升级时的哈希保留分开判断。
- `deferred`：重新启动后自动应用上次保存目标的单独实机核对、睡眠/热插拔、异常终止恢复及其他遥控器型号；不将本次确认扩大为这些场景通过。
- 未推送、创建 Tag 或发布 Release。

## 2026-10-20 缺失原麦克风回退

用户确定的行为：切换前保存原默认录音设备；退出优先恢复。原设备已不存在或明确不可用时，
保留 Windows 当前默认选择，不指定内置麦克风，不重启驱动。原本没有默认麦克风时，
不向 Windows 写入空设备标识。系统当前仍为 CABLE 也须如实保留，不能声称已恢复物理麦克风。

现场复现见 [旧恢复目标缺失](../Bugs/2026-10-20-missing-microphone-restore-target.md)。
针对三个角色的状态机、存储失败、旧记录兼容、无默认设备及只读真实端点复验单独记录；
这些检查不代替安装版启动、退出和实体听写验收。

| 项目 | 状态 | 证据或边界 |
| --- | --- | --- |
| 缺失设备回退、无默认设备、旧格式迁移及失败保护 | passed | 33 项状态机测试；包含有效与缺失角色联动、持久化失败、外部修改和写入中设备消失 |
| 页面状态、诊断去重及成功读回 | passed | ConnectionPage 45 项测试、类型检查；明确无麦克风与读取失败分开显示 |
| Rust 工作区回归及格式检查 | passed | 405 passed、0 failed、10 ignored；忽略的其他硬件实验未运行 |
| 真实旧恢复记录的只读副本验证 | passed | 生产 Owner 逻辑确认旧设备不存在，选择 ready、恢复完成且记录无 pending；系统设置写入硬禁止，实际次数为 0 |
| 现场保护 | passed | 三个 Capture 与三个 Render 默认值、测试源副本及安装版四份配置均未改变；已连接安装版继续运行 |
| 新安装版启动及麦克风选择 | passed | 用户确认“安装成功，状态和选择都正常”；生产日志显示三个旧目标安全回退、startup passed |
| 新安装版正常退出恢复及实体听写 | deferred | 尚未单独完成，不将安装、选择或单测替代实际使用 |

本轮日志存放在忽略目录 `target/microphone-recovery-0313`；只读实验收据在
`target/microphone-readonly-probe-*`，仅包含布尔结论和写入次数。

### 0.3.13 本地测试包

- 构建源：`fb48cbb5643d22bf4d7269227b32db6504dc780f`。
- passed：前端生产构建、Rust release、NSIS 和安装界面封装；最终安装器、构建原件、
  实际装入文件的绑定收据校验通过，98 个增强组件及主程序内嵌清单一致。
- 安装包：`target/release/bundle/nsis/Remote Coding_0.3.13_x64-setup.exe`，54864090 字节。
- SHA-256：`d62d5c01200b863bcea9c7627ed7b09340d1504e8edcef506fdc35d3f1110bcf`。
- passed：用户手动安装后确认状态及选择正常；实际运行版本为 0.3.13，安装文件哈希与捕获收据一致。
- passed：04:07:02 UTC 旧版经正常退出路径保留失败恢复记录；04:07:14 UTC 新版将三个已缺失原目标逐一回退，
  `restore`、`select`、`startup` 均 passed。恢复记录已迁移为 schema 2，当前无 pending 角色。
- passed：安装并启动后的三份偏好文件与安装前哈希一致；实际选择往返另以用户确认记录。
- deferred：新版正常退出恢复、持续听写、切换输入法及闲置首用验收。用户随后报告确认键长按异常，
  微信输入法实体测试暂缓。
- 没有自动启动安装器、强杀应用、操作蓝牙或发布到 GitHub。
