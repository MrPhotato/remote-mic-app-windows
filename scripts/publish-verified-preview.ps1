param([Parameter(Mandatory)][ValidatePattern('^v\d+\.\d+\.\d+$')][string]$Tag)
. (Join-Path $PSScriptRoot 'release-cd-common.ps1')
$source = (git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $source -notmatch '^[a-f0-9]{40}$') { throw 'Exact source required' }
$notes = Get-Content -LiteralPath "releases/$Tag.md" -Raw
$release = Find-PreviewRelease $Tag
if (!$release) { throw 'Expected exactly one existing preview release' }
Assert-ReleaseEnvelope $release $Tag $notes
if ((Get-ReleaseTagSource $Tag) -cne $source) { throw 'Tag source mismatch' }
$directory = Join-Path $PWD 'artifacts/release-download-verification'
New-Item -ItemType Directory -Path $directory -Force | Out-Null
if (@(Get-ChildItem -LiteralPath $directory -Force).Count) { throw 'Verification directory must be empty; preserve previous evidence' }
gh release download $Tag --repo $env:GITHUB_REPOSITORY --dir $directory
if ($LASTEXITCODE -ne 0) { throw 'Cannot download the published asset bytes' }
$config = Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json
$installer = Join-Path $directory "SayAll-Windows-$($config.version)-x64-setup.exe"
$authenticode = [string](Get-AuthenticodeSignature -LiteralPath $installer).Status
$metadata = Assert-DownloadedRelease $release $directory $Tag $source $env:GITHUB_REPOSITORY $config $authenticode
cargo run --release --locked --manifest-path tools/release-signature-verify/Cargo.toml -- $installer "$installer.sig" (Join-Path $directory 'release-signing.pub')
if ($LASTEXITCODE -ne 0) { throw 'Independent downloaded installer signature verification failed' }
$current = Read-ReleaseApi "releases/$($release.id)"
Assert-ReleaseSnapshot $current $release $Tag $notes
if ((Get-ReleaseTagSource $Tag) -cne $source -or (Read-ReleaseApi 'git/ref/heads/main').object.sha -cne $source) {
    throw 'Source changed during verification; preserve draft for inspection'
}
if ($current.draft) {
    # No upload, overwrite, delete or resign path. Retry only re-verifies existing bytes.
    gh api --method PATCH "repos/$env:GITHUB_REPOSITORY/releases/$($release.id)" -F draft=false -F prerelease=true --silent
    if ($LASTEXITCODE -ne 0) { throw 'Publication outcome uncertain; read release state before retry' }
}
$published = Read-ReleaseApi "releases/$($release.id)"
$expectedPublished = $release | ConvertTo-Json -Depth 20 | ConvertFrom-Json
$expectedPublished.draft = $false
Assert-ReleaseSnapshot $published $expectedPublished $Tag $notes
if (!$published.published_at) { throw 'Publication readback missing' }
$result = [ordered]@{result='passed'; release_id=$published.id; url=$published.html_url; tag=$Tag; source=$source;
    installer_sha256=$metadata.installer_sha256; authenticode_status=$authenticode;
    assets=@($published.assets | Select-Object id,name,size,digest)}
$result | ConvertTo-Json -Depth 6 | Set-Content artifacts/release-verification.json -Encoding utf8
"## Preview 已发布`n`n$($published.html_url)`n`nSource: $source`n`n安装器 SHA256: $($metadata.installer_sha256)`n`n6 个远端附件、来源、SHA256 和独立 minisign 验证 passed。客户端更新方式见版本说明。" >> $env:GITHUB_STEP_SUMMARY
Write-Host "Published and verified ${Tag}: $($published.html_url)"
