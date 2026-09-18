# Remote Coding for Windows

把小米蓝牙遥控器用于 Windows 语音输入、文字修改和 Codex 快捷键操作。

这是独立维护的 Codex 定制版本，基于 [SayAll Windows](https://github.com/GetSayAll/remote-mic-app-windows) 的 `451e5f9ced0deecd31eb0147c0a578a22924492d` 开发；产品与协议参考源自 [无线麦 SayAll](https://github.com/HD838A/remote-mic-app)。本项目不是 SayAll 或 OpenAI 的官方发行版。

## 功能

- Codex 操作页：连接状态、语音配置入口和一键日常按键方案。
- Codex 快捷键栏目：73 个 Windows 默认动作，可分类、搜索和自定义绑定。
- 普通退格：单按删除，按住按 Windows 键盘重复设置连续删除。
- 可选双击按标点删除：保留光标前最近的标点，默认关闭。
- 可选择 Codex 按住听写快捷键，沿用遥控器按下开始、松开结束的语音流程。
- 每次应用方案前备份当前按键配置，同时保留首次备份；可分别恢复。
- 独立应用身份、配置和图标；不接收上游官方版的自动更新。

底层 BLE、ATVV 音频解码、虚拟声卡输出和按键映射来自 Windows 上游。开发使用 Vue 3、Tauri 2 和 Rust。

## 使用前准备

- Windows x64、WebView2，以及已在 Windows 中配对的小米蓝牙遥控器。
- 支持目标为小米蓝牙遥控器 2 / RC001 和 2 Pro / RC003，实际兼容性与固件、系统输入上报有关。
- 使用遥控器麦克风需要另行安装 [VB-CABLE](https://vb-audio.com/Cable/)；只用普通按键不需要它。
- 安装 Codex，并确认所需快捷键在当前版本可用。

启动 `remote-coding.exe` 后，在“连接与语音”连接遥控器，再到“Codex 遥控”查看并应用日常方案。快捷键发送给当前前台窗口；先用主页键打开 Codex，再点击输入框。关闭主窗口会收进托盘，在托盘菜单选择“退出”才会结束程序。

## 日常按键方案

| 按键 | 单击 | 双击 | 长按 |
| --- | --- | --- | --- |
| 主页 | 打开或恢复 Codex | 新建任务 | 打开设置 |
| 上下左右 | 光标移动或界面导航 | 不另设 | 连续移动 |
| 确认 | Enter：发送或确认 | 不另设 | 不另设 |
| 返回 | Backspace | 默认关闭，可另选按标点删除 | 连续删除 |
| 菜单 | 下一个任务或标签页 | 命令菜单 | 上一个任务或标签页 |
| TV | 改动审查面板 | 打开或收起终端 | 搜索文件 |
| 电源 | Esc：关闭弹层或退出当前操作 | 不另设 | 不另设 |
| 音量＋/－ | 系统原始音量行为 | 保持原样 | 保持原样 |
| 语音 | 按住开始，松开结束；单独配置听写工具 | — | — |

主页、菜单和 TV 配有双击，单击松开后等待约 0.3 秒；独立长按动作约 0.55 秒触发一次。确认和默认退格没有双击等待。Enter 在审批框中可能批准请求，在输入框中可能发送文字，请留意当前焦点。

音量键允许自行配置；没有配置时不拦截或模拟系统音量。按键页的“恢复基础配置”只恢复返回退格、其余保持原样，与这里的日常方案不同。

快捷键依据 [OpenAI 官方命令参考](https://learn.chatgpt.com/docs/reference/commands) 整理。Codex 版本、自定义快捷键、键盘布局和当前界面都可能影响结果。

## Codex 听写

1. 在“连接与语音”选择“Codex 听写 · Ctrl + Shift + D”。
2. 将程序音频输出设置为 VB-CABLE 的 `CABLE Input`，让 Codex 从 `CABLE Output` 麦克风收音。
3. 切到 Codex 并点击输入框，按住遥控器语音键说话，松开结束。
4. 检查文字后，再按确认键发送。

切换听写快捷键不会改变音频来源；遥控器不会因此变成 Windows 原生麦克风。程序不捆绑第三方声卡驱动，也不自动更改系统默认音频设备。更多音频配置见 [安装与配置说明](docs/installation-and-configuration.md)。

## 可选的按标点删除

双击返回可以设置为“删到上一个标点（保留标点）”：`你好，今天天气很好｜` → `你好，｜`。当前段落没有标点时删到段首，光标紧跟标点时不删除。开启后，普通单击需等待约 0.3 秒以区分双击；按住仍会连续退格。

此功能使用公开的 Windows UI Automation 文本范围，不使用剪贴板。已有选区、密码或只读输入框、焦点变化、超时，以及不支持文本范围读取的输入框都会拒绝操作。它默认关闭，目标编辑器兼容性仍需实际验证。

## 已验证范围与限制

- 已完成 Windows x64 构建、前端与 Rust 自动测试，以及原生界面和配置保存检查。
- RC003 的 BLE 连接与 ATVV 握手已验证；这不等于语音识别和所有按键均已验证。
- RC003 实体返回、音量的输入上报，完整默认方案的实体操作，以及遥控器麦克风到 Codex 文字的完整流程仍待验证。可编辑并保存映射不代表硬件一定会发送该键。
- 按标点删除通过了独立测试输入框的多种边界验证，但曾出现闲置后首次操作未响应；Codex 输入框的兼容性和稳定性仍待验证。
- Windows 可能拒绝后台程序切换前台窗口；应用发现成功不等于一定能够夺取焦点。
- 遥控器连接且配置主页、TV 映射时，现有输入门控会同时接管物理键盘的 Home、反引号键。取消这两个映射或断开遥控器后恢复原样。

验证记录位于 [Testing](Testing)。历史记录中的设备和构建结论仅适用于其列出的环境，不代表所有 Windows 版本或遥控器型号均已通过。

## 从源码构建

需要 Node.js、Rust MSVC 工具链、Microsoft C++ Build Tools、Windows SDK 和 WebView2。

```powershell
git clone https://github.com/MrPhotato/remote-mic-app-windows.git
cd remote-mic-app-windows
./scripts/build-local.ps1
```

脚本安装锁定的前端依赖，执行前端编译、自动测试和 Rust 格式检查，再构建 `target/release/remote-coding.exe`。已有验证结果、只需重新构建时可加 `-SkipTests`；生成 NSIS 安装包可加 `-Installer`。使用便携工具链时，可通过 `-ToolchainScript '你的工具链目录/activate.ps1'` 加载其环境。

构建完成后可运行 `Start Remote Coding.cmd`。构建脚本不安装声卡驱动、不更改系统默认音频，也不发布 GitHub Release。

## 许可证与来源

本项目作为 SayAll Windows 的派生作品，继续使用 **GPL-3.0-only**，见 [LICENSE](LICENSE)。保留上游版权和来源记录，见 [ATTRIBUTION.md](ATTRIBUTION.md)。

当前应用图标为独立绘制的几何图形，按 GPL-3.0-only 提供；没有使用上游保留版权的品牌 Logo。上游品牌资产的历史许可说明见 [LOGO-LICENSE.md](LOGO-LICENSE.md)。Codex、SayAll 等名称仅用于准确说明兼容目标和项目来源，不表示官方认可。
