# 合并后 Actions run 的 PR 关联为空导致发布误停

- 发现日期：2026-09-21。
- 状态：已复現并修复门禁逻辑；下一次 Tag 的完整自动发布另验。
- 影响范围：Windows Release 工作流，不改变应用二进制功能。
- 功能点：发布源码与 CI 来源核验。
- 现象：PR #1 的 Windows CI 已成功，合并后 v0.2.7 发布任务以 Cannot establish PR association for Windows CI runs 退出。
- 复现条件：合并前 CI run `35529960366` 有 PR #1 关联，合并后相同 run 的 `pull_requests=[]`；head `96c19e7a045cc63e0efabc0a35be8b43282cbbef`、attempt 1 与 success 不变。
- 正常预期：仍验证最新 run/attempt，使用可核查的等价来源证据，不丢弃空关联运行或回退旧绿灯。
- 证据：失败发布 run `35530959494`；成功 CI 检出 `refs/remotes/pull/1/merge`、SHA `8733bbccf93136a287322c91dcf7cd0ed4fdc51d`。其树 `444b213b5f33131ce86ad8a32fb2e36f8a18fdb4` 与发布来源 `65d38c57980c830ffd7916e62203a19ac0a04cf0` 完全一致。
- 根因：将 GitHub 可变的 PR 关联字段误当作合并后必然保留的数据；此前 JSON fixtures 只证明遇空字段会停止，未覆盖真实 API 合并后的变化。
- 修复：空关联运行参与最新排序；只有最新 attempt 成功后才取 checkout 日志，绑定 PR 编号、测试 commit 与源文件树，并复查 attempt 没有变化。无证据仍拒绝。
- 验证：生产 YAML 函数的 37 个正负 fixture passed；真实 CI 日志与 Git commit API 的等价证据重放另见本次执行记录。覆盖新失败/进行中不能被旧成功遮盖、错误 PR/步骤/父提交/文件树、缺失或歧义日志。
- 隐私检查：只记录公开提交、树、PR 和 Actions run 编号；不包含用户目录、设备或凭据。
