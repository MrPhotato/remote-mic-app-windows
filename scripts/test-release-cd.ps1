$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'release-cd-common.ps1')
$directory = Join-Path $PSScriptRoot ("../target/release-cd-tests/" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$tag = 'v1.2.3'; $source = 'a' * 40; $repository = 'example/sayall'
$config = @{version='1.2.3';plugins=@{updater=@{pubkey='fixture-public-key';endpoints=@()}}}
$installerName = 'SayAll-Windows-1.2.3-x64-setup.exe'
$script:passed = 0
function Assert-Case([string]$Name, [bool]$Accept, [scriptblock]$Action) {
    $accepted = $false
    try { & $Action | Out-Null; $accepted = $true } catch { $reason = $_.Exception.Message }
    if ($accepted -ne $Accept) { throw "CD fixture failed: $Name (accepted=$accepted; $reason)" }
    $script:passed++
}
function Write-Fixture($Change = $null) {
    Set-Content -LiteralPath (Join-Path $directory $installerName) -Value 'installer fixture' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $directory "$installerName.sig") -Value 'signature fixture' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $directory 'release-signing.pub') -Value 'fixture-public-key' -Encoding utf8
    $metadata = @{schema_version=1;source_revision=$source;repository=$repository;version='1.2.3';release_tag=$tag;
        build_channel='preview';architecture='x64';installer=$installerName;updater_signature="$installerName.sig";
        installer_sha256=(Get-FileHash -LiteralPath (Join-Path $directory $installerName)).Hash.ToLowerInvariant();
        authenticode_status='NotSigned';automatic_updates_enabled=$false;helper_file_count=98;helper_manifest_sha256=('b'*64)}
    $manifest = @{version='1.2.3';platforms=@{'windows-x86_64'=@{url="https://github.com/$repository/releases/download/$tag/$installerName";signature='signature fixture'}}}
    if ($Change) { & $Change $metadata $manifest }
    $metadata | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $directory 'release-metadata.json') -Encoding utf8
    $manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $directory 'latest.json') -Encoding utf8
    $names = Get-ReleaseAssetNames '1.2.3'
    $sums = @($names | Where-Object {$_ -ne 'SHA256SUMS.txt'} | ForEach-Object {
        (Get-FileHash -LiteralPath (Join-Path $directory $_)).Hash.ToLowerInvariant() + '  ' + $_
    })
    $sums | Set-Content -LiteralPath (Join-Path $directory 'SHA256SUMS.txt') -Encoding utf8
    $id = 0
    $assets = @($names | ForEach-Object { $id++; $file=Get-Item -LiteralPath (Join-Path $directory $_)
        @{id=$id;name=$_;size=$file.Length;state='uploaded';digest=('sha256:'+(Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant())}
    })
    return (@{id=100;tag_name=$tag;prerelease=$true;draft=$true;body='Release notes';assets=$assets} | ConvertTo-Json -Depth 10 | ConvertFrom-Json)
}
$good = Write-Fixture
Assert-Case 'valid preview and all downloaded bytes' $true {
    Assert-ReleaseEnvelope $good $tag 'Release notes'
    Assert-DownloadedRelease $good $directory $tag $source $repository $config 'NotSigned'
}
$badEnvelopes = @(
    {param($r) $r.tag_name='v1.2.4'}, {param($r) $r.prerelease=$false},
    {param($r) $r.body='changed'}, {param($r) $r.assets=$r.assets[0..4]},
    {param($r) $r.assets[0].name='../escape.exe'}, {param($r) $r.assets[0].digest=$null},
    {param($r) $r.assets[0].state='starter'}, {param($r) $r.assets[0].size=0}
)
foreach ($change in $badEnvelopes) {
    $bad=$good | ConvertTo-Json -Depth 10 | ConvertFrom-Json; & $change $bad
    Assert-Case 'reject malformed remote envelope' $false { Assert-ReleaseEnvelope $bad $tag 'Release notes' }
}
foreach ($change in @(
    {param($m,$u) $m.source_revision='c'*40}, {param($m,$u) $m.repository='other/repo'},
    {param($m,$u) $m.automatic_updates_enabled=$true}, {param($m,$u) $m.Remove('automatic_updates_enabled')},
    {param($m,$u) $m.installer_sha256='e'*64}, {param($m,$u) $m.build_channel='stable'},
    {param($m,$u) $m.helper_file_count=0}, {param($m,$u) $m.helper_manifest_sha256='invalid'},
    {param($m,$u) $m.authenticode_status='Valid'},
    {param($m,$u) $u.platforms.'windows-x86_64'.url='https://example.com/other.exe'},
    {param($m,$u) $u.platforms.'windows-x86_64'.signature='different signature'}
)) {
    $bad=Write-Fixture $change
    Assert-Case 'reject self-consistent bytes with wrong metadata' $false {
        Assert-DownloadedRelease $bad $directory $tag $source $repository $config 'NotSigned'
    }
}
$good=Write-Fixture
Add-Content -LiteralPath (Join-Path $directory $installerName) -Value 'tampering'
Assert-Case 'reject altered download' $false { Assert-DownloadedRelease $good $directory $tag $source $repository $config 'NotSigned' }
$good=Write-Fixture
foreach ($change in @(
    {param($r) $r.id=101}, {param($r) $r.draft=$false},
    {param($r) $r.assets[0].id=99}, {param($r) $r.assets[0].digest='sha256:'+('d'*64)}
)) {
    $bad=$good | ConvertTo-Json -Depth 10 | ConvertFrom-Json; & $change $bad
    Assert-Case 'reject remote change before publication' $false { Assert-ReleaseSnapshot $bad $good $tag 'Release notes' }
}
Assert-Case 'unchanged snapshot accepted' $true { Assert-ReleaseSnapshot $good $good $tag 'Release notes' }

# Verify the real plan script against API fixtures; no GitHub calls or writes.
$originalLocation = Get-Location
$savedEnv = @{}
foreach ($key in @('GH_TOKEN','GITHUB_REPOSITORY','GITHUB_EVENT_NAME','GITHUB_REF','GITHUB_REF_NAME','GITHUB_OUTPUT','GITHUB_STEP_SUMMARY')) {
    $savedEnv[$key]=[Environment]::GetEnvironmentVariable($key)
}
$planPath=Join-Path $PSScriptRoot 'prepare-preview-release.ps1'
New-Item -ItemType Directory -Path (Join-Path $directory 'src-tauri'),(Join-Path $directory 'releases') -Force | Out-Null
$config | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $directory 'src-tauri/tauri.conf.json')
'{"version":"1.2.3"}' | Set-Content (Join-Path $directory 'package.json')
'Release notes' | Set-Content (Join-Path $directory 'releases/v1.2.3.md')
$global:sayallCdFixture=@{}
function git { return ('a'*40) }
function Invoke-RestMethod {
    param([string]$Uri, $Headers)
    if ($Uri -like '*/pulls') { return $global:sayallCdFixture.pr }
    if ($Uri -like '*/git/ref/tags/*') { return $global:sayallCdFixture.tag }
    if ($Uri -match '/releases\?per_page=100&page=') { return $global:sayallCdFixture.release }
    throw 'Unexpected fixture API request'
}
try {
    Set-Location $directory
    $env:GH_TOKEN='non-secret-fixture';$env:GITHUB_REPOSITORY=$repository
    $env:GITHUB_EVENT_NAME='push';$env:GITHUB_REF='refs/heads/main'
    $env:GITHUB_OUTPUT=Join-Path $directory 'outputs.txt';$env:GITHUB_STEP_SUMMARY=Join-Path $directory 'summary.txt'
    $global:sayallCdFixture.tag=$null;$global:sayallCdFixture.release=$null
    $global:sayallCdFixture.pr=@(@{merged_at='today';base=@{ref='main'};merge_commit_sha=$source;labels=@()})
    $global:LASTEXITCODE=0
    & $planPath
    Assert-Case 'ordinary merge never releases' $true { if ((Get-Content $env:GITHUB_OUTPUT -Raw) -notmatch 'ready=false') { throw 'Unexpected release' } }
    $global:sayallCdFixture.pr[0].labels=@(@{name='release:preview'})
    & $planPath
    Assert-Case 'opt-in merge prepares exact version' $true { if ((Get-Content $env:GITHUB_OUTPUT -Raw) -notmatch 'tag=v1.2.3') { throw 'Missing release plan' } }
    $global:sayallCdFixture.tag=@{object=@{type='commit';sha=('f'*40)}}
    Assert-Case 'existing foreign tag never overwritten' $false { & $planPath }
    $global:sayallCdFixture.tag=@{object=@{type='commit';sha=$source}};$global:sayallCdFixture.release=$good
    & $planPath
    Assert-Case 'existing complete release skips rebuild' $true { if ((Get-Content $env:GITHUB_OUTPUT -Raw) -notmatch 'existing=true') { throw 'Rebuild requested' } }
    $env:GITHUB_EVENT_NAME='workflow_dispatch';$env:GITHUB_REF='refs/heads/feature'
    Assert-Case 'manual release cannot execute feature ref' $false { & $planPath -RequestedTag $tag }
    $env:GITHUB_REF='refs/heads/main'
    Assert-Case 'manual version mismatch rejected' $false { & $planPath -RequestedTag 'v1.2.4' }
} finally {
    Set-Location $originalLocation
    foreach ($key in $savedEnv.Keys) { [Environment]::SetEnvironmentVariable($key,$savedEnv[$key]) }
    Remove-Item function:git,function:Invoke-RestMethod
    Remove-Variable sayallCdFixture -Scope Global
}
Write-Host "Release CD gates: $script:passed passed"
