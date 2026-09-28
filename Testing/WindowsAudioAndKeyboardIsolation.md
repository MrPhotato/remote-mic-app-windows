# 遥控器麦克风界面与物理键盘隔离（2026-09-28）

## 范围

基于个人 fork `f3ed8fc1952ac4bf61a8b83b4dd6bf0244061878`；界面提交 `6fa8f3c`，键盘修复提交 `2b3ad8b`。本地包的代码来源为后者，随包文档另行记录验证结果。
本次只调整语音页呈现和 Home/TV 输入归因；不改语音时序、驱动、默认映射或用户已保存配置。

## 自动验证

- `passed`：Windows 本机前端 17 文件 / 169 tests，包括设备枚举失败、自动启用失败、手动重试及已有配置保留。
- `passed`：`cargo fmt --all -- --check` 和 `git diff --check`。
- `passed`：`cargo test -p sayall-windows home_tv --lib --offline -- --test-threads=1`，5 tests。
  - 普通 Home/OEM_3 在无设备身份解码器中不被识别；生产键盘处理函数放行修饰键组合、重复按下、松开及陈旧配对。
  - 真实遥控器的已归因输入仍可识别 Home/TV，重复来源去重并在停止、重启时释放。
  - HID 和键盘路径均不为 Home/TV 武装全局拦截器。
- 测试直接调用处理函数，未安装全局钩子、未注入真实输入、未改变系统输入设置。未执行会安装全局钩子的旧映射测试。
- 就绪状态不会影响 Home/OEM_3 的放行是解码先于 `gate_ready` 的代码路径证明；不是测试中实际切换全局就绪状态的结果。
- `passed`：复用已安装 v0.2.7 Helper 前核对 `git diff v0.2.7 -- helpers scripts/build-rc003-helper.ps1` 无差异；完整清单 96 文件 / 152655741 bytes 的哈希及主程序内嵌清单校验通过。
  清单 SHA-256：`c261e898f3031ab9e62b5d7de574092fd0898ed6b8f09615d2d2c41f833ea0a3`。

## 本地安装和界面

- `passed`：Windows x64 Release 构建、Vue 类型检查与打包、NSIS 完整安装包生成；复用已验证且源码未变的 Helper。
- `passed`：通过应用自身退出事件正常结束旧进程，安装器 exit 0；升级后、任何手动设置之前，三个配置文件与备份逐字节相同。
- `passed`：启动来源 `2b3ad8b049ecc2041ba8dce357bbcee029a6f0f1`，`build_channel=local`；运行日志明确 `shared_vk=passthrough attribution=selected_raw_input`，Home/TV 已配置。遥控器自动恢复连接，按键监听就绪。
- `passed`：原生窗口主卡片显示“遥控器麦克风”、CABLE Output 提示及扬声器说明；高级设置默认折叠、展开可见原路由，折叠后恢复主视图。
- 实机额外发现已保存的旧多声道端点标识失效：`endpoint_identity_mismatch`，界面正确保留设置并显示错误，没有假报麦克风就绪，也没有自动改选其它播放设备。经可见高级设置选择当前标准 CABLE Input 后，日志 `manual_virtual_cable`、选择成功 28ms / 前端完成 34ms，界面显示声音传送就绪。此处仅修改了应用音频路由；不代表目标应用已经选好麦克风或语音识别通过。
- `passed`：安装后完整 Helper 96 文件哈希及主程序内嵌清单验证通过。
- 安装包 SHA-256：`312a4e6e7ad35592469988fad99cdb73de13fa870173ee2e25ad4103be922bda`。
- 安装后 EXE SHA-256：`bfcb14c0f2047a502bbd91f1cc063cd24747127e572c93449cb0e4978e9f3bfe`。
- 与构建目录 EXE 的完整比较只差 Tauri 安装类型标识的 3 字节：`__TAURI_BUNDLE_TYPE_VAR_UNK` → `__TAURI_BUNDLE_TYPE_VAR_NSS`。依据本地 `tauri-utils 2.9.3` 的 `platform.rs`，NSS 对应 NSIS。仅在内存中归一化该标识后 SHA-256 完全一致；没有修改安装文件或以宽泛忽略方式绕过校验。
- 本次仅构建并安装本地测试包，没有发布 Release、Tag 或 PR。

## 实体复验

- `deferred`：保持遥控器连接，在空白文本框单按反引号、Shift＋反引号、按住反引号后松开，确认原生输入及停止重复。
- `deferred`：遥控器 TV/主页后立即重复普通键盘测试，确认不再因近期遥控器活动被误识别。
- `deferred`：遥控器 TV/主页映射效果，以及允许的原生 Home/反引号旁路；长按可能出现原生重复，不能宣称严格单响应。
- `deferred`：闲置后的第一次物理输入、RC001、睡眠恢复及历史连续反引号故障的真实复现。用户报障的历史状态未知，不能由源码假设断言其完整发生过程。

用户按实体键才能证明上述边界，软件注入因 `LLKHF_INJECTED` 放行不能替代。
