# Read-only operations; push permission is needed to include release drafts.
# Ordinary main merges are a no-op.
param([string]$RequestedTag = '')
. (Join-Path $PSScriptRoot 'release-cd-common.ps1')
$source = (git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $source -notmatch '^[a-f0-9]{40}$') { throw 'Exact source required' }
if ($env:GITHUB_EVENT_NAME -eq 'push' -and $env:GITHUB_REF -eq 'refs/heads/main') {
    $prs = @(Read-ReleaseApi "commits/$source/pulls")
    $matched = @($prs | Where-Object { $_.merged_at -and $_.base.ref -eq 'main' -and $_.merge_commit_sha -eq $source })
    if ($matched.Count -ne 1 -or 'release:preview' -notin $matched[0].labels.name) {
        'ready=false' >> $env:GITHUB_OUTPUT
        '普通 main 合并：未标记 release:preview，不发布。' >> $env:GITHUB_STEP_SUMMARY
        exit 0
    }
} elseif ($env:GITHUB_EVENT_NAME -eq 'push' -and $env:GITHUB_REF -like 'refs/tags/v*') {
    $RequestedTag = $env:GITHUB_REF_NAME
} elseif ($env:GITHUB_EVENT_NAME -ne 'workflow_dispatch' -or $env:GITHUB_REF -ne 'refs/heads/main') {
    throw 'Manual release must run from main'
}
$config = Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json
$tag = 'v' + $config.version
if ($tag -notmatch '^v\d+\.\d+\.\d+$' -or ($RequestedTag -and $RequestedTag -cne $tag)) { throw 'Release tag/version mismatch' }
if ((Get-Content package.json -Raw | ConvertFrom-Json).version -ne $config.version) { throw 'Frontend version mismatch' }
if (!(Test-Path -LiteralPath "releases/$tag.md")) { throw 'Committed release notes required' }
$tagSource = Get-ReleaseTagSource $tag
if ($tagSource -and $tagSource -cne $source) { throw 'Existing tag points to another commit; never overwrite it' }
$release = Find-PreviewRelease $tag
if ($release -and !$tagSource) { throw 'Existing release has no verifiable tag' }
if ($release) { Assert-ReleaseEnvelope $release $tag (Get-Content -LiteralPath "releases/$tag.md" -Raw) }
"ready=true`ntag=$tag`nsource=$source`nexisting=$([bool]$release)".ToLowerInvariant() >> $env:GITHUB_OUTPUT
"Preview CD: $tag / $source；已有资产：$([bool]$release)。" >> $env:GITHUB_STEP_SUMMARY
