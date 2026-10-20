# 无线麦 SayAll Windows 发布流程

本流程将参考仓库的发布不变量改写为 Windows 版本，适用于公开 Preview、Stable 及应用内更新资产。

## 发布不变量

- 默认交付物是本地测试包。先在本机生成测试安装器，报告其路径和校验值，并完成与改动风险相称的本地安装、升级、启动和功能验证。
- 本地验证通过后必须停在“可发布”状态；不得自行创建发布 Tag、GitHub Release、发布草稿或上传发布资产。只有用户在收到验证结果后明确要求“发布预览版”，才获得本次发布授权。“继续”“做完”“合入 main”等指令本身不构成发布授权。
- 发布源必须是已合入远端 `main` 的精确 SHA；开始前 `git fetch origin main`，发布 worktree 必须干净且与该 SHA 一致。
- `main` 不直接开发或 push，版本、Release Notes、脚本和文档均通过普通 PR 合入。
- Preview 的 CI 未签名 NSIS artifact 只能用于受限验收，不能宣称为公开可信安装包；公开分发需同时满足 Authenticode（若流程已启用）、updater minisign 签名、SHA-256 和来源元数据要求。
- Tag、Release Notes、资产和 `latest.json` 建立后视为不可变。内容变化回到普通 PR，使用新版本/Build；不得覆盖旧 Tag 或资产。

## Preview CD：一次提交发布请求

2026-10-15 起，明确授权发布的版本 PR 使用 Preview CD 和 Windows Release。
完成本地验证、更新版本号和 `releases/vX.Y.Z.md` 后，推送功能分支并建立 PR。一次执行：

```powershell
./scripts/request-preview-release.ps1 -PullRequest <PR编号> -ExpectedHead <完整head SHA>
```

该入口使用已登录的 GitHub CLI，为同仓库的非草稿 main PR 加上 `release:preview` 标签，并提交
一次 Preview CD 请求。CI 未完成时登记等待；`workflow_run` 完成事件自动接续，不轮询。
只有带发布标签的 PR 可由此流程合并；普通 PR、外部 fork 和草稿不会自动合并。
新增提交会重新跑 CI；标签代表此版本 PR 的发布意图。取消发布时移除标签。
不修改仓库全局 auto-merge、审批或主分支保护设置，不需要新增 PAT、后台驻留进程或 Codex 定时盯守。

后续自动执行：CI 成功 → 合入 main → 核对最新 PR CI 和源码 → 创建不可变 Tag → 构建签名完整包
→ 创建草稿 → 重新下载全部六个附件 → 校验来源、digest、SHA256、minisign → 公开 Preview。
普通 main 合并没有发布标签时不会发版。手动推送版本 Tag 同样视为明确发布入口；不能仅为测试创建发布 Tag。
Preview CD 只执行 main 上的控制脚本，并检查最新精确 head/attempt 成功、CI 实际测试的合并树及 main 父提交。
用精确 head 调用合并 API，合并后再次核对文件树，失配不发布。`GITHUB_TOKEN` 合并不会触发 push 工作流，
因此显式 dispatch Windows Release；手工合入带标签的 PR 则由 main push 接续。
CD 内创建 Tag 不会再次触发第二次构建。首次把 CD 接入 main 时需人工完成一次已验证 PR 的合并。

运行结果和校验收据见 Actions 的 Windows CI / Windows Release；GitHub 原生完成通知受账号通知设置控制。
失败停在具体步骤并保留现场。已有完整草稿时，可在 Windows Release 的 Run workflow 选择 main、填写同一 Tag，
或重跑失败的运行；流程跳过重新编译及签名，只重做远端字节验证和公开。已公开且身份一致时只校验，不重复发布。
缺失附件、资产或 notes 被改动、Tag/当前 main 不一致时停止，不自动删除、覆盖或回退门禁。
CI 后 main 已变化时，更新 PR 后重跑 CI；其他仓库写入者若恰好在检查与合并间改动 main，合并后树核对会阻止发布，
需人工检查该合并。此流程不能代替仓库级分支保护，也不会绕过已有的分支规则。
本 CD 只处理 GitHub Preview，不自动操作用户电脑。应用启动时只检查发布信息；
用户点击「更新并重启」后才下载、验签、正常退出和安装，新版由安装器重新打开。

## 本地验证与发布准备

1. 先完成本地测试包的构建和验证，并向用户报告结果；没有用户随后给出的明确预览发布指令时，到此停止。
2. 获得明确发布授权后，从最新 `origin/main` 建立发布分支，确认版本、Build 和说明已冻结。
3. 运行 `scripts/ci-preflight.ps1`；需要深度检查时运行 `-Full`。CI 必须记录 source SHA、构建通道和 artifact digest。
4. 运行 `scripts/verify-windows-bundle.ps1`，确认 NSIS、应用和元数据状态；未签名候选明确标记为 `unsigned-ci-preview-not-for-public-release`。
5. 在 Windows 主机按 [Testing/WindowsReleaseBranchLifecycle.md](Testing/WindowsReleaseBranchLifecycle.md) 完成安装、升级、卸载、启动和设置保留验证；按 [Testing/WindowsRC003Preview.md](Testing/WindowsRC003Preview.md) 执行硬件与第三方语音边界。
6. PR 描述分别列出自动化、安装器、真实 RC001/RC003、VB-CABLE 和第三方输入法结果；不可把 Mac 构建或模拟器结果写成 Windows 真机通过。

## Stable 与 updater 资产

- 0.3.12 起，先生成原 NSIS 安装内核，再封装独立 Vue/Tauri 安装界面。只对最终外层安装器生成 updater 签名；`target/release/setup-build-receipt.json` 将版本、最终安装器、内层包、主程序及安装宿主 SHA-256 绑定。CI 和发布输出都须核对该收据，不能把内层安装器当作最终资产。原有 `/S`、`/P`、`/UPDATE`、`/R` 和 `/D` 调用继续转发给内核。

- 当前 fork 的 workflow 先创建 **Preview 草稿**，下载验证成功后按本次发布请求自动公开，不声称 Stable。Tag 必须指向当前 `origin/main`，并关联已合入的 PR；该 PR 精确 head 的最新 Windows CI 及最新重试必须完成且成功，旧绿灯不能覆盖后来失败。
- GitHub 在 PR 合并后可能将 Actions run 的 `pull_requests` 返回为空。此时不得丢弃这些运行或退回旧绿灯：仍选择最新精确 head，并从该次 attempt 的 checkout 日志提取对应 PR 的 merge SHA，再用 Git commit API 核对父提交包含 PR head、文件树等于发布源码；缺失日志、身份不符或重试变化均停止。门禁回归见 `scripts/test-release-source-gates.ps1`。
- 缺少本仓库 `TAURI_SIGNING_PRIVATE_KEY` 时 fail closed。公开 Preview 包必须通过独立 minisign 验签；当前未启用 Authenticode，因此 Windows 仍将安装器识别为 `NotSigned`，minisign 不等于 Windows 开发者证书。Stable 需要另外启用并验证 Authenticode，不能沿用 Preview 结论。
- 普通 PR CI 不读取签名私钥，也不生成临时更新密钥；发布构建显式使用 `build-local.ps1 -BuildChannel preview -ReleaseTag <tag> -CreateUpdaterArtifacts`。静态更新端点保持为空，只在发布构建覆盖开启 updater 资产生成。应用内按钮使用刚核实的本仓库具体 Release 端点；`automatic_updates_enabled=false` 表示没有静默下载安装，不表示禁用此按钮。
- Helper 必须先于 Rust 应用编译构建，随后调用 `verify-rc003-helper-bundle.ps1` 核对完整文件、SHA-256 和主程序内嵌完整清单。仅安装器存在或主程序编译通过不算完整打包。
- `scripts/generate-updater-manifest.ps1` 生成 `latest.json`、ASCII 安装器名、`.sig`、`release-signing.pub`、`release-metadata.json` 和 `SHA256SUMS.txt`。清单中的签名是 `.sig` 内容，不是路径；下载地址必须指向本 fork 的 HTTPS Release。元数据记录源码 SHA、版本、通道、Helper 清单摘要和 Authenticode 状态；不包含私钥或个人路径。
- 上传前使用 `tools/release-signature-verify` 验证安装器完整字节。暂存目录非空时拒绝重跑，保留现场供核查；不会覆盖或删除远端资产。
- `releases/<tag>.md` 必须在版本 PR 中准备并合入，草稿创建后核对全部下载字节、签名和源码身份，再按用户本次明确授权自动公开。发布前再次读取 Release、附件 ID/digest、Tag 和 main，防止验证期间内容发生变化。
- 发布前打印并核对待上传资产清单、大小和 SHA-256；删除或覆盖远端资产不属于正常重试流程。
- 应用内更新必须先完成下载与固定公钥验签，再断开 BLE、释放键态、停止音频并恢复麦克风；收尾失败不得启动安装器。NSIS 等待退出超时中止，不强杀进程。安装使用当前目录，完成后只由 NSIS `/R` 启动新版，下一次启动重新建立麦克风恢复基线。

## 失败与重试

- Runner、GitHub、网络或签名服务失败且尚无公开身份：在同一 SHA、版本、Build 和 artifact 身份上重试，不新建 rerun 分支、不升版本、不重签已成功字节。
- Release 已创建但验证失败：先只读核对 Tag、资产、digest 和 notes，只补缺失验证；发现字节或来源不一致立即停止并保留现场。
- 任何认证、权限、5xx、超时或无法判断的远端结果均 fail closed。

## 发布报告

报告 source SHA、Tag、版本/Build、workflow Run、artifact ID/digest、安装器签名状态、资产 SHA-256、安装生命周期矩阵、真实硬件/第三方工具结果及 `passed`、`failed`、`deferred` 边界。不得输出证书、私钥、密码、Token 或用户数据。
