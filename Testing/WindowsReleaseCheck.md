# 手动检查更新验证

范围：0.3.6；关于页显式点击后读取 `MrPhotato/remote-mic-app-windows` 的公开 Release，包含预览版。软件不自动下载或安装，也不触发启动检查、蓝牙恢复或按键注入。

## 验证记录

- 前端全量 253 项 passed；交叉审查后增加切页期间请求和结果保留，相关 26 项再次 passed。覆盖错误重试、重复点击、发布说明纯文本及 URL 约束。
- Rust 宿主库 50 项 passed、2 项 ignored；其中新增 13 项覆盖 SemVer、预览版、草稿、最高版本资产完整性及来源、分页和 HTTP 错误。无 Rust core/Windows 音频或按键逻辑改动。
- 真实 GitHub API：passed。显式执行 ignored 的 `release_check::tests::live_public_release_check`，复用生产请求和选择路径，以 0.3.4 为已安装版本发现实际 0.3.5，`status=available prerelease=true`。
- 浏览器界面：passed。独立 headless Chromium 在 1029/1440 宽度、浅/深两主题中检查卡片边界、错误提示、切页后的结果保留，0 页面异常。新版展示使用 UI fixture；真实网络结果独立由上述 Rust 测试验证，不混称原生验收。
- 前端生产构建、Rust 格式、Windows release 构建与 NSIS 安装包：passed。本地升级至 0.3.6，三份设置文件 SHA-256 升级前后相同，主程序与构建副本仅差 Tauri 的 `__TAURI_BUNDLE_TYPE_VAR_UNK` → `NSS` 三字节安装器标记，其余字节一致；98 文件 Helper 完整性及主程序内嵌清单 passed。旧 Helper 的三个 Python 3.11 依赖移入回收站，未永久删除。
- 安装版原生界面检查：passed，2026-10-19 用户手动启动 0.3.6，在「关于 → 软件更新」点击「检查更新」，确认“正常显示检查结果”。
- 安装版打开发布页及网络失败后的重试：deferred，尚无相应现场确认；已有前端自动化覆盖不替代原生验收。

证据保存在忽略目录 `target/release-check-*.log`、`target/release-check-*.png` 与 `target/update-036-20261019/`，配置快照不提交。安装包 SHA-256：`1b134ac33cb4c84324b7369e56db1affc9d5ab12596caef124f0aed1e4a32d17`。

## 本地验证命令

```powershell
npm test -- --maxWorkers=2 --minWorkers=1
npm test -- --run src/components/ReleaseUpdateCard.test.ts src/pages/AboutPage.test.ts src/lib/github-release.test.ts --maxWorkers=2 --minWorkers=1
cargo test -p sayall-windows-app --lib --locked
cargo test -p sayall-windows-app release_check::tests::live_public_release_check --lib --locked -- --ignored --nocapture
cargo fmt --all -- --check
scripts/build-local.ps1 -Installer -SkipTests -SkipHelperBuild
```

本地构建复用已核验 Helper 和 Rust/MSVC 环境；未创建发布标签、推送、PR 或 GitHub Release。

更新检查自身不包含音频和遥控器功能，因此本次结果不宣称 RC001/RC003 新增硬件验收。
