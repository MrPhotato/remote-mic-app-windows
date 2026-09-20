# 按标点删除在 WebView 焦点检查阶段被拒绝

- 发现日期：2026-09-20。
- 状态：旧规则安装版已有后续 12 次首删及 7 次双击实体通过，保留首个焦点变化取消与严格闲置 deferred；当前短句提案已由用户取消、未交付。最新即时退格＋双击 Ctrl+Z 预设另待实际验证。
- 影响范围：SayAll 0.2.6 本地三键增强安装包，Windows 11 x64（build 26200），本机 RC003；受测目标为应用内 WebView 测试输入框。其它应用及 RC001 未由本轮证明。
- 功能点：返回键单击普通退格、可选双击删到上一个标点。
- 现象：用户反馈单击与双击均无响应；实测双击动作已派发但被焦点检查拒绝，普通退格在同一观测窗口有实际删除，不能把两条路径合并为一个“输入未收到”结论。
- 复现条件：返回单击配置 `normal_backspace`，双击配置 `delete_to_punctuation`；准备初始长度 12、预期按标点删除后长度 6 的固定测试框，聚焦后实体双击返回。
- 原始复现版本的预期：短双击执行一次按标点删除并保留标点；孤立单击在双击窗口结束后普通退格；按住仍连续退格，失败不能误删其它目标。后续提前首删和当前短句新语义另节记录，不回写为原版行为。
- 证据：[脱敏失败与普通退格对照](../Testing/evidence/punctuation-webview-focus-20260920.json)，原始观测在 ignored `target/local-launch/rc003-integration/punctuation-double-observation.json` 与 `installed-app-final.log`。

## 观测窗口与失败事实

按 fixture 的 `preparedAt=2026-09-20T13:02:38.277Z` 过滤累计数组，截止结果采样 `13:07:47.291Z`，不把此前通过的按键试验混入本轮。共 48 个语义边沿：返回 19 对、左 2 对、右 3 对。

- failed：8 次 Back/Double 均进入 `map_fire ... action=delete_to_punctuation`，随后 7 次以 `foreground_process_mismatch` 失败，1 次以 `focus_changed` 失败；对应 `map_text_edit result=declined`，没有一次按标点删除成功。
- 首个失败为 `13:06:10.353Z` 的 `focus_changed`；其余 PID 检查拒绝分布在 `13:06:12.603Z` 至 `13:07:18.640Z`。每次动作请求均有失败终态，不是 Helper 或手势入口无事件。
- 观测到 10 次普通 Single、10 对浏览器可信 Backspace DOWN/UP、10 次一字符删除，长度 12→2。最终长度不等于期望 6；途中恰好经过 6 来自普通连删，不能视为标点动作通过。
- DOM 焦点在两组按键期间分别保持于 `13:06:06.590Z` 至 `13:06:19.726Z`、`13:07:10.083Z` 至 `13:07:20.883Z`。它不能独立证明 Win32/UIA 焦点身份，却足以说明不能无证据归咎用户未聚焦输入框。

## 普通单击和长按的独立判断

`button_gestures.rs` 的 `DOUBLE_CLICK_WINDOW` 为 300 ms。配置双击后，`GestureRecognizer::release` 将单击放到释放后的窗口末；`advance` 到期派发 Single。第二次短按释放会派发 Double，而不会先执行 Single；因此一次失败的双击不会自动补发普通退格。

本轮有一个明确的孤立单击：`13:06:17.404Z` DOWN、`.532Z` UP，`.845Z` 收到 Single、`.853Z` 文本从 12→11。释放到手势为 313 ms，到文本变化为 321 ms，符合 300 ms 判定窗口加调度的行为，不是该次单击完全失效。快速连续按键进入双击分支后被拒绝，可能让连续尝试看起来都无响应；不能据此断言用户每次按键意图或所有报障场景已被解释。

随后 `13:07:17.054Z` 的第二击按住至 `.886Z`（832 ms），`.558Z` 开始转成普通 Single 和重复退格，合计 9 次删除，长度 11→2。`GestureRecognizer::advance` 对按住第二击保留两次普通按压并继续连发；这些事件不是按标点删除成功，也不证明双击配置下的普通退格对所有目标都已验收。

`button_mapping.rs` 的 `NormalBackspace` 分支直接调用 `injector.tap(Backspace)`；`DeleteToPunctuation` 才调用 UIA 文本编辑路径。因此不能仅用后者的 `foreground_process_mismatch` 推断前者也在该检查处失败。用户其余“单击失效”尝试仍须保留未知范围并按实际窗口复验。

## 根因范围与最小修复方向

已确认修复前拒绝位置为 `crates/sayall-windows/src/text_edit.rs` 的 `validate_focus`：旧实现要求焦点 UIA 元素的 `CurrentProcessId` 严格等于前台 HWND 的进程 ID，否则返回 `foreground_process_mismatch`。日志明确证明该分支拒绝了 7 次动作。

`13:15:54.640Z` 的只读归属探针通过 `FromHandle` 精确定位本应用主窗口，再按本应用 fixture 的 Automation ID 查找输入框；没有把用户当时的全局焦点当作目标。`own-fixture-ancestry.json` 确认深度 0 的 Edit 元素 PID 不等于主程序，最近有原生 HWND 的深度 3 Pane 窗口有效，其 owner PID 与该 Pane 元素 PID 一致，`GA_ROOT` 又精确回到本应用主窗口。由此确认同属本应用窗口树的合法 WebView 控件会被“PID 必须与主程序相同”规则误拒绝。

探针当时实际前台不是本应用，因此该元数据证明宿主归属关系，不是修复后执行通过，也不能还原首次 `focus_changed` 的具体瞬间。修复必须保留真实前台及元素焦点检查，不能简单移除保护。

修复候选已在 `text_edit.rs` 中加入实际前台窗口/元素归属核对：跨 PID 时沿 UIA RawView 查找最近的原生 HWND，要求该 HWND owner PID 与对应祖先元素 PID 一致、`GA_ROOT` 等于原前台窗口，再复核焦点元素与前台进程。首个原生宿主不匹配即拒绝，不继续向更高祖先寻找放行条件；保留密码字段、选择及文本变化、取消和超时等保护。原始场景复验仍在进行，首个 `focus_changed` 的具体触发点及其它应用兼容性仍未知。

失败试验后曾将双击映射恢复为 disabled，界面与后端均确认一致。它恢复普通单击的即时路径，不代表按标点删除已修复；该处仅记录当时的临时处置，不描述后续验证时的配置。

## 修复候选安装与自动化证据

- passed：`2026-09-20T13:22:21.132Z` 的安装摘要确认旧主程序正常退出，再安装并启动新版。
  安装器退出码 0；主程序与构建产物只差 NSIS bundle 标记，已安装主程序 16,107,008 字节，
  SHA-256 为 `d859fa0d821c095cfa2e7537c3b1e8b6ef90cbc06cee091a91d758dbd4c74b08`。
  安装器 SHA-256 为 `7d0752c5dd9c5946e0fb2b08a7b7cb35fb210454310dbc6dc51e71e26a38fb9f`。
- passed：候选 Helper 的 96 个文件全部通过散列校验，失败数 0，manifest 与构建一致：
  `9057f5de09a13799452ccc8a1da59c0067d82b4f4e9eec059338fa35747d8441`。
  此证据证明本次候选已安装，不替换此前旧包实际按键试验的版本归属。
- passed：文本编辑定向测试 13 项、映射/RC003 取消定向测试 16 项、监督与清理退出状态测试 6 项。
  日志分别为 ignored `text-edit-foreground-ancestry-tests.log`、
  `button-mapping-edit-cancellation-tests.log`、`cleanup-exit-tests.log`，均位于本轮集成日志目录。
- deferred：原生文本探针本轮执行 `text_edit_probe --run --idle` 时未确认自身前台/焦点/文本，
  因此没有进行 UIA 编辑或删除，退出码 2。证据为 ignored `text-edit-native-ancestry-regression.log`；
  不能把编译通过或该次启动列为原生删除回归通过。
- 安装原始摘要为 ignored `install-foreground-fix-result.json`；本记录只复制脱敏数值和散列。
  新版真实单击与双击验证已准备，当前双击已重新配置为 `delete_to_punctuation`，
  增强状态为 waiting/awaiting_neutral，尚未收到本轮实体按键。已请求用户先初始化再验证单击、双击，
  结果仍待执行确认，不声明删除通过。

## 验证与隐私

### 第二轮实体复测与文本范围根因

`13:27:44Z–13:28:23Z` 用户复测后反馈“单击很慢，双击无效果”。3 次 Single 均产生实际退格，输入长度 12→11→10→9；保留既有 300ms 双击窗口导致单击在释放后等待，尚未实现首击提前删除。5 次 Double 均未删除，其中 1 次 `focus_changed`，4 次已通过 `foreground_ancestor_verified` 后以 `boundary_mismatch` 拒绝。不能再将此轮失败归因于旧 PID 判定；该部分已实际生效。

只读实验锁定自家固定测试框，其 `TextPattern.DocumentRange` 长度为 12；从光标向左移动 2048 个 Character 却越过这个控件的 DocumentRange，得到长度 829 的页面范围。原路径的逗号查找返回空范围，后续精确校验正确拒绝了删除。将前文 START 限制在该输入框 DocumentRange START 后，前文长度为 12、边界范围长度为 1、删除范围长度为 6，所有预期文本比较及端点归属均通过，原文本与选区保持不变。

该实验证明范围构造缺陷，尚不能代替修改后实际删除。修复需在读取前验证光标位于自身 DocumentRange 内，并将移动后的前文限制于该范围；保留精确匹配、焦点、选区及取消检查。脱敏日志与原始/限定范围对照见 [第二轮证据](../Testing/evidence/punctuation-webview-boundary-20260920.json)。

- failed：原始 WebView 按标点删除 8 次均未完成。
- passed（仅所列观测）：普通孤立单击实际删除 1 字，按住第二击实际删除 9 字，可信退格键盘事件严格成对。
- passed（软件真 UIA）：下节记录的最终修复产品函数已在同一 WebView fixture 实际完成 12→6；不等于安装版遥控器双击验收。
- 待验证：最终修复安装版的 RC003 孤立单击、双击、按住、焦点切走及取消；其它应用和 RC001。尚未执行修复后实体全链路验收，不标记已修复。
- 本记录的整理仅解析现有文件并审阅代码；安装及探针结果引用实际执行方的日志，整理过程没有操作进程、改变配置、聚焦 UI 或发送按键。
- 隐私检查：只记录语义按钮、时间、错误分类、焦点布尔值和文本长度；不记录测试文本、个人路径、UIA 元素内容、设备身份、进程 ID、原始报告、语音或凭据。

## 第三轮软件真 UIA：异步选区与实际文本结果

范围限定后，直接调用产品函数的自家 WebView 探针仍 failed：第一次 135ms、诊断复现 110ms，均保持原文长度 12。后一轮日志定位为 `selection_range_mismatch`：Select 自身耗时 145 微秒，从 Select 开始至读取结束 257 微秒，GetSelection 仍为原空光标，上下文长度 12 且未变化；约 3ms 后清理路径已能读到目标选区。因此问题是提交选择后立即读取尚未生效的状态，不能用放宽 Compare 或忽略实际选区解决。旧清理日志仅表示恢复请求 submitted，不作为已观察恢复成功的证据。

修复保留精确范围和文本校验，只允许在原空光标与目标删除选区之间有界观察；出现其他选区、文本变化或失焦即拒绝。取消时用独立有限预算等待自己的选择请求落实，仅恢复仍能证明属于本次操作的光标，并确认恢复；无法确认时返回明确警示。

`text_edit_webview_probe` 使用公开 UIA 锁定本应用固定输入框，直接调用产品 `text_edit` 函数，并读取 ValuePattern 精确比较最终预期。它真实执行了 UIA 和按键输入，不是模拟；没有经过遥控器、手势或安装版事件链。最终软件回归共 6 项 passed：

| 情形 | 实际长度变化 | 探针动作耗时 |
| --- | --- | --- |
| 保留标点删除后缀 | 12→6 | 264ms |
| 光标前已是标点 | 6→6 | 138ms |
| 普通成对退格对照 | 12→11 | 0ms（毫秒精度的提交耗时） |
| 重复后缀删除 1 | 12→6 | 204ms |
| 重复后缀删除 2 | 12→6 | 199ms |
| 固定框闲置 152.542 秒后 | 12→6 | 245ms（产品内部日志 244ms） |

这些耗时不含实体遥控器与手势判定。探针在产品调用前会读取自己的 UIA fixture 做保护性确认，也可能预热 provider；因此闲置一项仅证明该受控流程闲置后通过，不证明未经 UIA 预热的冷态首按。成功删除的 `selection_wait` 均为 `pending_count=0`，本轮没有直接复现成功路径中先见空光标、再见目标的轮询分支；该分支由旧真实失败及定向单元测试支撑。

本轮 `text_edit` 定向单元测试 17 passed、0 failed，覆盖焦点归属、待生效选择、第三种选区拒绝、取消后的迟到选择和恢复确认、预算边界。此时新增 `cancel_selection` 与首击提前退格/双击补偿真实窗口用例尚待执行；早期快速双击探针因未确认自身前台而 deferred，未发送产品编辑操作。用户随后聚焦后的实际结果见下一节，原证据保留当时状态。补偿候选仅考虑精确可验证的纯文本，不能从文本一致推导富文本格式已恢复。

脱敏版本化记录见 [软件 UIA 执行证据](../Testing/evidence/punctuation-webview-uia-execution-20260920.json)。此前两轮实体失败证据保持原样；本轮尚未证明最终修复安装版的 RC003 实体双击通过。

## 第四轮软件真 UIA：提前退格、补偿与取消

用户聚焦自家固定测试框后，`15:10:14Z–15:12:39Z` 直接调用产品 Runtime/文本编辑函数。共 9 项 passed、1 项 failed；逐项数值见 [提前退格执行证据](../Testing/evidence/optimistic-backspace-webview-20260920.json)。

| 情形 | 实际长度过程（UTF-16 单位） | 首次提交 / 动作结束 |
| --- | --- | --- |
| 普通后缀双击补偿 | 12→11→6 | 45ms / 545ms |
| 单次首删 | 12→11 | 28ms / 28ms |
| 英文标点恢复 | 6→5→6 | 46ms / 138ms |
| 中文标点恢复 | 3→2→3 | 35ms / 130ms |
| 单个 emoji 删除，边界已到达 | 8→6 | 38ms / 98ms |
| 无标点删空 | 5→4→0，但 API 拒绝 | 39ms / 531ms，failed |
| 首删后取消事务 | 12→11，后续 complete 被拒绝 | 48ms / 48ms |
| 选择生效时取消并恢复光标 | 12→12 | 187ms |
| 普通后缀连续复验两次 | 每次 12→11→6 | 均 30ms / 504ms、520ms |

首删计时结束于成对输入已提交回调，不是 DOM 首次变化或画面响应时刻。探针在首删提交后直接调用 Runtime.complete，不包含实体双击判定时间；第一轮重复的 stdout 被第二轮覆盖，504ms 来自同轮产品日志从 begin 到 shutdown，不能与其它 action_ms 当作完全相同计时边界。最终 ValuePattern 精确预期与产品 `exact_result_verified` 一起作为成功依据；不只检查 SendInput 返回。

`cancel_selection` 在实际非空选区出现时触发取消。日志先记录删除操作 cancelled，再记录 `cleanup_wait_caret pending_count=1 observed_ms=16` 和 `selection_cleanup ... passed original_caret_restored`；探针确认原文 12 单位不变、末尾空光标恢复，执行方同时观察到没有 input 事件。该项实际覆盖恢复选择异步生效的等待，不等于在任意第三方应用、失焦或超时时均已验收。

无标点一项虽然实际删到 0，产品最终验证以 `unsupported_text_content` 拒绝，仍记 failed。只读公开 UIA 对照确认固定空 `<input>` 的 ValuePattern 为空，但 DocumentRange.GetText 返回一个 U+FFFC，选区为空且位于文档起点。修复候选必须用独立的空 ValuePattern、可编辑属性和精确光标等条件确认空字段，不能普遍忽略嵌入对象字符。本轮仅记录实证与修复方向，修复后的通过结果另行补充。

`15:16:02Z` 的独立修复复验 passed：首击提交 39ms、总动作 559ms，实际 5→4→0，API、焦点与精确预期均通过；产品记录 `empty_result_verify ... writable_empty_value_placeholder_confirmed` 后 `exact_result_verified`。新分支只接受可写 Edit 的空 ValuePattern、单个 U+FFFC 文档和起点空光标，并重复两次组合态、焦点及空字段检查；没有放宽普通嵌入对象检查。执行方另确认最终 DOM 光标为 0/0。最新事务定向测试 23 passed、0 failed。原失败与修复复验分别保留，累计为 10 项软件执行 passed、1 项历史 failed。

本轮没有实体遥控器、手势分类或新安装验证，fixture 预检可能预热 UIA provider，也没有真实闲置首用。单次首删用例随后关闭 Runtime，不能据此宣称手势窗口到期不会重删。复杂字素、活动 IME、已有选区、其它应用及富文本格式仍未通过本轮证明。

## 第五轮安装版 RC003 实体返回：旧尾标点规则

来源 `8e7b68acf1af3ab0ce09b21615416af268684c2b` 已正常升级并核验主程序/Helper，安装快照见 [安装证据](../Testing/evidence/eager-backspace-install-20260920.json)。后续 `15:35:45.697Z–15:36:12.082Z` 的 [独立实体证据](../Testing/evidence/eager-backspace-physical-round1-20260920.json) 记录 20 对返回边沿、13 次首击事务和 7 次双击。

- 首个事务在 52ms 以 `focus_changed` 取消，没有提交首删；记录保留，具体焦点变化原因未知，不将整轮写成全部首按通过。
- passed：后续 12 次 prepared 首删，提交 36–55ms；7 次 double_complete 均有 `exact_result_verified`，其中 5 次删剩余后缀、2 次恢复旧规则要求保留的尾标点。
- 测试框记录 17 对可信 Backspace DOWN/UP；文本重新增多包含人工重填，只把各事务精确核验及两次 Unicode 补回归属于对应动作。
- deferred：返回前已有 Up、Left、Right，未满足“闲置后的第一个实体键就是返回”；旧软件探针及基础普通退格的闲置证据不替代此项。

这轮证明焦点归属、DocumentRange 和异步选区修复后的安装版旧规则能够完成实际删除。它不证明后续新语义、其它应用、RC001 或所有焦点/取消/生命周期条件通过。

## 后续产品调整：短句提案取消，改为即时退格＋双击撤销

用户曾要求双击连同当前短句尾部的连续标点和非换行空白一起删除。候选曾有文本编辑定向 19 passed、事务 31 passed，随后用户明确取消该需求，未构建安装交付。第五轮的 2 次旧尾符恢复仍是历史旧规则结果，不标成短句方案通过。

最新 Coding 预设改为返回单击立即普通退格、按住连删、双击发送 Ctrl+Z。双击发生时首击已删除，撤销范围及分组由编辑器决定，不保证整段撤销，也不保证只恢复该次首删。音量＋/－仍切上一/下一任务或标签页，TV 单击仍开关侧边栏；菜单/TV 的其余动作与官方来源见 [新方案调研](../docs/investigations/2026-09-20-codex-remote-defaults.md)。首次/最近备份保留；新预设及即时首删接线的实际输入、安装、实体和冷首用均独立 deferred，不以旧标点成功或预设测试替代。

撤销版预设与 Coding 页面定向 24 tests passed（`coding-preset-undo-tests.log`），覆盖映射表、说明和原有备份恢复行为；测试没有发送真实键，不能证明 Ctrl+Z 在目标编辑器中撤销了哪些内容。

2026-09-21 普通退格＋撤销的软件执行已 passed，实际固定输入框 12→11→12，见 [独立证据](../Testing/evidence/ordinary-backspace-undo-webview-20260921.json)。这条新默认路径无需原来的 UIA 文字事务。候选短句测试的两次超时失败归档于 [已取消实验](../Testing/evidence/current-clause-guard-timeout-20260920.json)，未纳入当前实现。
