# Only run after explicit authorization to publish this PR.
param(
    [Parameter(Mandatory)][int]$PullRequest,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$ExpectedHead,
    [string]$Repository = 'MrPhotato/remote-mic-app-windows'
)
$ErrorActionPreference = 'Stop'
$pr = gh api "repos/$Repository/pulls/$PullRequest" | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $pr.state -ne 'open' -or $pr.draft -or $pr.base.ref -ne 'main' -or
    $pr.head.repo.full_name -ne $Repository -or $pr.head.sha -cne $ExpectedHead) { throw 'Expected open, same-repository main PR at exact head' }
# REST labels need only repository access. `gh pr edit` also queries organization
# metadata through GraphQL and can fail with an otherwise sufficient repo token.
gh api "repos/$Repository/issues/$PullRequest/labels" --method POST -f 'labels[]=release:preview' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Cannot record release intent' }
gh workflow run preview-cd.yml --repo $Repository --ref main -f "pull_request=$PullRequest" -f "expected_head=$ExpectedHead"
if ($LASTEXITCODE -ne 0) { throw 'Cannot submit Preview CD request; inspect workflow availability and permissions' }
$confirmed = gh api "repos/$Repository/pulls/$PullRequest" | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot read back release request' }
if ($confirmed.head.sha -cne $ExpectedHead -or 'release:preview' -notin @($confirmed.labels.name)) {
    throw 'Release request readback differs from the requested head or label'
}
[pscustomobject]@{url=$confirmed.html_url;state=$confirmed.state;headRefOid=$confirmed.head.sha;labels=@($confirmed.labels.name)} | ConvertTo-Json -Compress
