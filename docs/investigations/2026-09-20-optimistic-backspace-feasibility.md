# 首击提前退格、双击保留标点删除：可行性调查

日期：2026-09-20。范围：只读核对现有实现和 Microsoft 公开文档；没有修改输入行为、执行文本编辑实验或采集用户文本。

## 结论与当前状态

用户期望返回键单击及时执行普通 Backspace；若随后形成双击，则补偿首击，执行“删到最近标点，保留标点”。这个方向可在满足条件的文本控件中研究实现，不能据此承诺通用应用零延迟或无条件安全还原。

**当前保留既有 300ms 双击判定窗口以维护删除边界正确性。** 提前退格方案尚未实现；删除前 UIA 快照的冷态、闲置首用和热态耗时、输入法组合态识别、Unicode 恢复及目标输入框兼容性均未完成验证，不据此调整等待常量。

既有双击动作是 `delete_to_punctuation`，不是 Ctrl+Backspace，也不是 Ctrl+Z。归档时，本地已启用该动作，实际输入框的 `alpha, bravo` → `alpha,` 实体双击用例仍待观察；这只是**当前实现的验收目标**，不是提前退格方案的实验结果。后续实际验收另记 [RC003 集成验收](../../Testing/WindowsRc003Input.md)，不能用三键监听或普通退格已通过代替本项通过。

## 已有实现与来源

- [text_edit.rs](../../crates/sayall-windows/src/text_edit.rs)：读取有上限的光标前文本，定位并保留最近标点或段落边界；拒绝已有选区、受保护/只读内容和不支持的范围。复核焦点、光标、文本后，通过 UIA 选择确切范围，再发送一次 Backspace。没有文本插入接口。
- [button_gestures.rs](../../crates/sayall-windows/src/button_gestures.rs)：配置双击时，首击释放后等待 300ms；窗口内第二击释放触发双击。已有测试明确要求双击之前不先删掉字符或标点，并覆盖按住重复、迟到事件和取消。
- [button_mapping.rs](../../crates/sayall-windows/src/button_mapping.rs)：普通退格直接提交键盘动作；标点删除在独立工作线程执行，具有忙碌和取消 generation。提前退格不能直接套成两次互不关联的动作。
- [send_input_windows.rs](../../crates/sayall-windows/src/send_input_windows.rs)：已有成对键盘注入，但本次核查未见 `KEYEVENTF_UNICODE` 文本恢复路径。
- [ATTRIBUTION.md](../../ATTRIBUTION.md) 的既有手势来源采用按配置启用双击、保留判定窗口的语义；没有记录已验证的“首删后补回”实现。本调查的补偿方案是待验证设计推论，不冒称已有成熟参考。

Microsoft 明确 [TextPattern 不提供文字插入/修改接口](https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-textpattern-overview)，大部分操作依赖跨进程调用，文本内容也没有其他模式那样的缓存机制。恢复文字需另用公开输入接口；[KEYBDINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput) 的 `KEYEVENTF_UNICODE` 可以提交 Unicode 输入，但不保证目标应用最终接受，也不恢复原有富文本格式。

## 为什么必须有首删前快照

例如光标位于末尾：

| 原文 | 首击后 | 双击目标 | 推论 |
|---|---|---|---|
| `你好，世界` | `你好，世` | `你好，` | 若确证只发生该次退格，直接删除剩余后缀即可，不必先恢复“界” |
| `你好，` | `你好` | `你好，` | 首击已删掉必须保留的逗号，需要恢复逗号，并停止继续向前删除 |

仅在首击后读取 `你好`，无法区分原本是 `你好，` 还是本来就没有标点。保存一个 UIA range 引用也不等于保存删除前的不可变文本；需要实际读取并保存有限文本及对应焦点、选区和光标条件，只在内存保留，不写入日志。

删除前查询**不需要固定等待 300ms 双击窗口**，但仍必须先等查询完成再发送首删。现有 UIA 每次连接/事务超时为 300ms、操作预算为 2s；这些是上限，不是实测延迟。后台预读缓存可以研究，但不能假定公开 UIA 事件及时性或缓存新鲜度能够提供原子保证。UIA 校验与 SendInput 之间始终存在非原子的时间窗口。

## 最小待验证实现范围

1. 仅针对“单击普通退格 + 双击按标点删除”建立短期编辑事务，由独立 MTA worker 串行处理，不阻塞钩子、BLE 或 UI 线程。
2. 首删前确认同一前台输入框、空选区、可写、非密码及可确认无输入法组合态；读取有限上下文，再次检查焦点与取消状态，然后提交普通退格。
3. 首删后只接受同一位置发生可解释的删除；双击时再次核对上下文和光标。若被删内容属于目标后缀，仅删除剩余后缀；跨过需要保留的边界时，才考虑精确恢复。恢复后必须验证实际结果，不能把 SendInput 返回成功当作文字已恢复。
4. 手势状态记录“首击已执行”，避免窗口到期、迟到第二击或按住转连删时重复执行首击。失焦、取消、断连、配置变化或新文本编辑均作废事务，不在新焦点补字，也不撤销已完成的普通首删。
5. 不使用 Ctrl+Z、剪贴板或整框 ValuePattern.SetValue 恢复；不读取私有配置、进程内存或私有编辑器协议。

## 必须验证或拒绝的边界

| 场景 | 当前设计要求 |
|---|---|
| 已有选区 | 首击可能删除整个选区，不能按一个字符恢复；初版禁用该次补偿，保留普通退格语义 |
| IME 正在组合/转换 | Backspace 可能修改组合串而非已提交文本；不启用补偿 |
| IME 状态接口不可用 | 不能视为“没有组合态”，应拒绝该次补偿 |
| 焦点、光标、选区或文本变化 | 放弃补偿和进一步批量删除，不往新的位置插字 |
| Emoji、代理对、组合字符 | 不假定一次 Backspace 等于一个 Rust `char` 或一个 UTF-16 单元；核对实际删除片段，不确定就拒绝 |
| 段落、富文本、嵌入对象 | 补回纯文本无法保证恢复格式或段落结构；初版不支持这种补偿 |
| UIA provider 不支持、超时或保护输入框 | 保留普通退格，说明增强动作不可用；不得猜测被删字符或回退为更大范围删除 |
| 首击无实际删除、部分输入提交、恢复未生效 | 不假设成功；依据观察停止，记录固定原因而不记录文本 |

IME 可查询公开 [IUIAutomationTextEditPattern::GetActiveComposition](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtexteditpattern-getactivecomposition)，但必须先确认目标 provider 支持；接口缺失与返回“无组合”不是同一种结果。[UIA TextUnit_Character](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-uiautomationtextunits) 是语言相关的文本单位，不能把其数量直接当成 UTF-16 单元数或应用 Backspace 的删除数量。

下一步只读测量应先回答目标输入框能否提供上述快照和组合态信息，并分别记录冷态、闲置首用、热态的查询耗时与拒绝原因；日志不含文本、窗口标题或设备身份。得到证据后，再决定是否进入受限实现及 Unicode 恢复真机测试。此文不包含任何提前退格方案的 `passed` 结论。
