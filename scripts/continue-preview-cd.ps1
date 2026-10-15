# This script is executed only from main by Preview CD, without signing secrets.
. (Join-Path $PSScriptRoot 'release-cd-common.ps1')
. (Join-Path $PSScriptRoot 'release-source-gates.ps1')
$event = Get-Content -LiteralPath $env:GITHUB_EVENT_PATH -Raw | ConvertFrom-Json
if ($env:GITHUB_EVENT_NAME -eq 'workflow_dispatch') {
    if ($env:GITHUB_REF -ne 'refs/heads/main' -or $env:REQUESTED_PR -notmatch '^[1-9][0-9]*$' -or
        $env:EXPECTED_HEAD -notmatch '^[a-f0-9]{40}$') { throw 'Manual CD requires main, PR number and exact head' }
    $number = [long]$env:REQUESTED_PR
    $head = $env:EXPECTED_HEAD
} else {
    $run = $event.workflow_run
    if ($run.event -ne 'pull_request' -or $run.conclusion -ne 'success' -or $run.head_repository.full_name -ne $env:GITHUB_REPOSITORY) {
        throw 'Expected successful same-repository PR CI'
    }
    $head = $run.head_sha
    $candidates = @($run.pull_requests | Where-Object {$_.base.ref -eq 'main'})
    if ($candidates.Count -ne 1) {
        $candidates = @(Read-ReleaseApi "commits/$head/pulls" | Where-Object {
            $_.state -eq 'open' -and $_.base.ref -eq 'main' -and $_.head.sha -eq $head
        })
    }
    if ($candidates.Count -ne 1) {
        'No single open PR association; no merge requested.' >> $env:GITHUB_STEP_SUMMARY
        exit 0
    }
    $number = [long]$candidates[0].number
}
$pr = Read-ReleaseApi "pulls/$number"
if ('release:preview' -notin $pr.labels.name) {
    'PR 未标记 release:preview，不自动合并或发布。' >> $env:GITHUB_STEP_SUMMARY
    exit 0
}
if ($pr.head.sha -cne $head -or $pr.head.repo.full_name -ne $env:GITHUB_REPOSITORY -or $pr.base.ref -ne 'main' -or $pr.draft) {
    throw 'Authorized PR identity changed'
}
if ($pr.state -ne 'open' -and !$pr.merged) { throw 'PR was closed without merge' }
$runs = Read-ReleaseApi "actions/workflows/windows-ci.yml/runs?event=pull_request&head_sha=$head&per_page=100"
if (!@($runs.workflow_runs).Count) {
    '发布请求已登记。等待 Windows CI 注册并完成，无需轮询。' >> $env:GITHUB_STEP_SUMMARY
    exit 0
}
$latest = Select-LatestPrCiRun $runs $head $number
$verified = Read-ReleaseApi "actions/runs/$($latest.id)"
if ($verified.status -ne 'completed') {
    '发布请求已登记。等待 Windows CI 完成事件，无需轮询。' >> $env:GITHUB_STEP_SUMMARY
    exit 0
}
Assert-LatestPrCiPassed $verified $latest $head $number
# Always prove the actual tested merge tree, including its current main parent.
$log = (gh run view $verified.id --repo $env:GITHUB_REPOSITORY --attempt $verified.run_attempt --log) -join "`n"
if ($LASTEXITCODE -ne 0) { throw 'Cannot read CI checkout evidence' }
$testedSha = Get-TestedMergeSha $log $number
$tested = Read-ReleaseApi "git/commits/$testedSha"
$main = (Read-ReleaseApi 'git/ref/heads/main').object.sha
Assert-TestedMergeTree $tested $testedSha $head $tested.tree.sha
if (!$pr.merged -and @($tested.parents | Where-Object {$_.sha -eq $main}).Count -ne 1) {
    throw 'main changed since CI; update this PR and let CI run again'
}
$configFile = Read-ReleaseApi "contents/src-tauri/tauri.conf.json?ref=$head"
$config = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($configFile.content)) | ConvertFrom-Json
$tag = 'v' + $config.version
if ($tag -notmatch '^v\d+\.\d+\.\d+$') { throw 'Invalid release version' }
$null = Read-ReleaseApi "contents/releases/$tag.md?ref=$head"
if (!$pr.merged -and (Get-ReleaseTagSource $tag)) { throw 'Version already tagged; do not merge another release with this version' }
$confirmed = Read-ReleaseApi "actions/runs/$($verified.id)"
if ($confirmed.run_attempt -ne $verified.run_attempt) { throw 'CI was rerun during verification' }
Assert-LatestPrCiPassed $confirmed $latest $head $number
$currentPr = Read-ReleaseApi "pulls/$number"
if ($currentPr.head.sha -cne $head -or 'release:preview' -notin $currentPr.labels.name) { throw 'Release intent changed' }
if (!$pr.merged) {
    gh api --method PUT "repos/$env:GITHUB_REPOSITORY/pulls/$number/merge" -f "sha=$head" -f merge_method=merge --silent
    if ($LASTEXITCODE -ne 0) { throw 'Merge failed or uncertain; inspect PR before retry' }
}
$merged = Read-ReleaseApi "pulls/$number"
if (!$merged.merged -or $merged.head.sha -cne $head) { throw 'Merge readback failed' }
$source = $merged.merge_commit_sha
$commit = Read-ReleaseApi "git/commits/$source"
Assert-TestedMergeTree $commit $source $head $tested.tree.sha
if ((Read-ReleaseApi 'git/ref/heads/main').object.sha -cne $source) { throw 'Merged source is no longer current main' }
# Bot merges do not trigger push workflows. Explicit dispatch is supported with GITHUB_TOKEN.
gh workflow run windows-release.yml --repo $env:GITHUB_REPOSITORY --ref main -f "tag=$tag"
if ($LASTEXITCODE -ne 0) { throw 'Release dispatch uncertain; inspect Windows Release before retry' }
"PR #$number 已合并，$tag 签名与发布已交给 Windows Release。`n`nSource: $source" >> $env:GITHUB_STEP_SUMMARY
