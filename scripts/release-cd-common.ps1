$ErrorActionPreference = 'Stop'

function Read-ReleaseApi([string]$Path, [switch]$AllowMissing) {
    if (!$env:GH_TOKEN) { throw 'GH_TOKEN is required' }
    $headers = @{Authorization="Bearer $env:GH_TOKEN"; Accept='application/vnd.github+json'; 'X-GitHub-Api-Version'='2022-11-28'}
    try { Invoke-RestMethod "https://api.github.com/repos/$env:GITHUB_REPOSITORY/$Path" -Headers $headers }
    catch {
        if ($AllowMissing -and [int]$_.Exception.Response.StatusCode -eq 404) { return $null }
        throw
    }
}

function Get-ReleaseTagSource([string]$Tag) {
    $ref = Read-ReleaseApi "git/ref/tags/$Tag" -AllowMissing
    if (!$ref) { return $null }
    if ($ref.object.type -eq 'commit') { return $ref.object.sha }
    if ($ref.object.type -eq 'tag') {
        $tagObject = Read-ReleaseApi "git/tags/$($ref.object.sha)"
        if ($tagObject.object.type -eq 'commit') { return $tagObject.object.sha }
    }
    throw 'Unexpected tag object type'
}

function Get-ReleaseAssetNames([string]$Version) {
    @("SayAll-Windows-$Version-x64-setup.exe", "SayAll-Windows-$Version-x64-setup.exe.sig",
      'latest.json', 'release-signing.pub', 'release-metadata.json', 'SHA256SUMS.txt')
}

function Assert-ReleaseEnvelope($Release, [string]$Tag, [string]$Notes) {
    $names = Get-ReleaseAssetNames $Tag.Substring(1)
    if (!$Release.id -or $Release.tag_name -cne $Tag -or $Release.prerelease -ne $true -or
        @($Release.assets).Count -ne 6 -or
        (Compare-Object ($names | Sort-Object) (@($Release.assets.name) | Sort-Object))) {
        throw 'Release identity or six-asset set mismatch'
    }
    if (($Release.body -replace "`r`n", "`n").Trim() -cne ($Notes -replace "`r`n", "`n").Trim()) {
        throw 'Release notes differ from committed notes'
    }
    foreach ($asset in $Release.assets) {
        if (!$asset.id -or $asset.size -le 0 -or $asset.state -ne 'uploaded' -or $asset.digest -cnotmatch '^sha256:[a-f0-9]{64}$') {
            throw 'Release asset identity, upload state or digest unavailable'
        }
    }
}

function Assert-ReleaseSnapshot($Current, $Verified, [string]$Tag, [string]$Notes) {
    Assert-ReleaseEnvelope $Current $Tag $Notes
    if ($Current.id -ne $Verified.id -or $Current.draft -ne $Verified.draft) { throw 'Release changed during verification' }
    foreach ($asset in $Current.assets) {
        if (@($Verified.assets | Where-Object {
            $_.id -eq $asset.id -and $_.name -ceq $asset.name -and $_.size -eq $asset.size -and $_.digest -ceq $asset.digest
        }).Count -ne 1) { throw 'Asset changed during verification' }
    }
}

function Assert-DownloadedRelease($Release, [string]$Directory, [string]$Tag, [string]$Source,
                                  [string]$Repository, $Config, [string]$AuthenticodeStatus) {
    $version = $Tag.Substring(1)
    $names = Get-ReleaseAssetNames $version
    if ($Config.version -ne $version -or @($Config.plugins.updater.endpoints).Count) { throw 'Committed version or updater policy mismatch' }
    foreach ($asset in $Release.assets) {
        $path = Join-Path $Directory $asset.name
        if ($asset.name -cnotin $names -or (Get-Item -LiteralPath $path).Length -ne $asset.size -or
            ('sha256:' + (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()) -cne $asset.digest) {
            throw 'Downloaded asset size or GitHub digest mismatch'
        }
    }
    $checked = @()
    foreach ($line in (Get-Content -LiteralPath (Join-Path $Directory 'SHA256SUMS.txt'))) {
        if ($line -cnotmatch '^([a-f0-9]{64})  (.+)$') { throw 'Invalid checksum record' }
        $hash = $Matches[1]; $name = $Matches[2]
        if ($name -cnotin $names -or $name -eq 'SHA256SUMS.txt' -or $name -in $checked) { throw 'Invalid checksum file list' }
        if ((Get-FileHash -LiteralPath (Join-Path $Directory $name)).Hash.ToLowerInvariant() -cne $hash) { throw 'Checksum mismatch' }
        $checked += $name
    }
    if ($checked.Count -ne 5) { throw 'Incomplete checksum list' }
    $key = (Get-Content -LiteralPath (Join-Path $Directory 'release-signing.pub') -Raw).Trim()
    if ($key -cne $Config.plugins.updater.pubkey.Trim()) { throw 'Public key differs from committed configuration' }
    $metadata = Get-Content -LiteralPath (Join-Path $Directory 'release-metadata.json') -Raw | ConvertFrom-Json
    $manifest = Get-Content -LiteralPath (Join-Path $Directory 'latest.json') -Raw | ConvertFrom-Json
    $installerName = "SayAll-Windows-$version-x64-setup.exe"
    $installer = Join-Path $Directory $installerName
    $signature = (Get-Content -LiteralPath "$installer.sig" -Raw).Trim()
    if ($metadata.schema_version -ne 1 -or $metadata.source_revision -cne $Source -or $metadata.repository -cne $Repository -or
        $metadata.version -ne $version -or $metadata.release_tag -cne $Tag -or $metadata.build_channel -ne 'preview' -or
        $metadata.architecture -ne 'x64' -or $metadata.installer -cne $installerName -or $metadata.updater_signature -cne "$installerName.sig" -or
        $metadata.automatic_updates_enabled -ne $false -or $metadata.helper_file_count -lt 1 -or
        $metadata.helper_manifest_sha256 -cnotmatch '^[a-f0-9]{64}$' -or
        $metadata.installer_sha256 -cne (Get-FileHash -LiteralPath $installer).Hash.ToLowerInvariant() -or
        $metadata.authenticode_status -ne $AuthenticodeStatus -or $AuthenticodeStatus -notin @('NotSigned','Valid')) {
        throw 'Release metadata or installer identity mismatch'
    }
    if ($manifest.version -ne $version -or
        $manifest.platforms.'windows-x86_64'.url -cne "https://github.com/$Repository/releases/download/$Tag/$installerName" -or
        $manifest.platforms.'windows-x86_64'.signature -cne $signature) { throw 'Updater manifest mismatch' }
    return $metadata
}
