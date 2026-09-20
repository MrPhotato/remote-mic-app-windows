param(
    [string]$PackageRoot,
    [string]$WdkVersion = '10.0.26100.6584',
    [string]$SdkVersion = '10.0.26100.1'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $PackageRoot) { $PackageRoot = Join-Path $repoRoot 'target/local-launch/driver-toolchain/packages' }
$PackageRoot = [IO.Path]::GetFullPath($PackageRoot)
$outRoot = Join-Path $repoRoot 'target/sayall-hid-filter/x64/Release'
New-Item -ItemType Directory -Path $outRoot -Force | Out-Null
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsRoot) { throw 'Visual Studio C++ Build Tools are required.' }
& (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null

# Official WDK NuGet integration pattern; no global WDK install or implicit restore.
# See ATTRIBUTION.md. Pin all package versions in the local build receipt.
$imports = @(
    "Microsoft.Windows.WDK.x64.$WdkVersion/build/native/Microsoft.Windows.WDK.x64.props",
    "Microsoft.Windows.SDK.CPP.x64.$SdkVersion/build/native/Microsoft.Windows.SDK.cpp.x64.props",
    "Microsoft.Windows.SDK.CPP.$SdkVersion/build/native/Microsoft.Windows.SDK.cpp.props"
)
$props = '<Project>'
foreach ($relative in $imports) {
    $import = Join-Path $PackageRoot $relative
    if (-not (Test-Path -LiteralPath $import -PathType Leaf)) { throw "Missing pinned package: $relative. See Testing/WindowsRc003Filter.md." }
    $props += '<Import Project="' + [Security.SecurityElement]::Escape($import) + '" />'
}
$props += '</Project>'
$propsPath = Join-Path $outRoot 'Directory.Build.props'
[IO.File]::WriteAllText($propsPath, $props)

function Invoke-Stage([string]$Name, [scriptblock]$Action) {
    $stageLog = Join-Path $outRoot "$Name.log"
    $stageWatch = [Diagnostics.Stopwatch]::StartNew()
    & $Action *> $stageLog
    $stageExit = $LASTEXITCODE
    if ($stageExit -ne 0) {
        Get-Content -LiteralPath $stageLog -Tail 35
        throw "stage=$Name result=failed exit_code=$stageExit"
    }
    Write-Host "stage=$Name result=passed elapsed_ms=$($stageWatch.ElapsedMilliseconds)"
}

$sourceDir = Join-Path $repoRoot 'drivers/sayall-hid-filter'
$wdkBin = Join-Path $PackageRoot "Microsoft.Windows.WDK.x64.$WdkVersion/c/bin/10.0.26100.0/x64"
Push-Location $outRoot
try {
    Invoke-Stage 'host-build' {
        & cl.exe /nologo /W4 /WX /TC /I (Join-Path $sourceDir 'tests') (Join-Path $sourceDir 'remap.c') (Join-Path $sourceDir 'tests/remap_test.c') /Fe:remap-test.exe
    }
    Invoke-Stage 'host-tests' { & (Join-Path $outRoot 'remap-test.exe') }
    Invoke-Stage 'driver-build' {
        & (Join-Path $vsRoot 'MSBuild/Current/Bin/amd64/MSBuild.exe') (Join-Path $sourceDir 'SayAllHidFilter.vcxproj') /m:2 /nologo /t:Build '/p:Configuration=Release' '/p:Platform=x64' "/p:DirectoryBuildPropsPath=$propsPath" "/p:InfToolPath=$wdkBin" /p:InfToolArchitecture=Native64Bit /p:PreferredToolArchitecture=x64 /p:RunCodeAnalysis=true /p:SignMode=Off /p:SpectreMitigation=Spectre /v:minimal
    }
    $packageDir = Join-Path $outRoot 'SayAllHidFilter'
    $wdkRoot = Join-Path $PackageRoot "Microsoft.Windows.WDK.x64.$WdkVersion/c"
    Invoke-Stage 'infverif' {
        & (Join-Path $wdkRoot 'tools/10.0.26100.0/x64/infverif.exe') /w /v (Join-Path $packageDir 'SayAllHidFilter.inf')
    }
    Invoke-Stage 'apivalidator' {
        $apiXml = Join-Path $wdkRoot 'build/10.0.26100.0/universalDDIs/x64/UniversalDDIs.xml'
        & (Join-Path $wdkBin 'apivalidator.exe') "-DriverPackagePath:$packageDir" "-SupportedApiXmlFiles:$apiXml" "-ApiExtractorExePath:$wdkBin"
    }
    $artifacts = @(Get-ChildItem -LiteralPath $packageDir -File | Where-Object { $_.Extension -in '.sys', '.inf', '.cat' })
    if ($artifacts.Count -ne 3) { throw 'Expected exactly SYS, INF, CAT in the unsigned driver package.' }
    Copy-Item -LiteralPath (Join-Path $sourceDir 'LICENSE') -Destination (Join-Path $outRoot 'SayAllHidFilter/LICENSE')
    $receipt = [ordered]@{
        schema = 1
        built_at_utc = [DateTime]::UtcNow.ToString('o')
        source_revision = (& git -C $repoRoot rev-parse HEAD).Trim()
        source_dirty = [bool](& git -C $repoRoot status --porcelain)
        source_files = @(Get-ChildItem -LiteralPath $sourceDir -File | ForEach-Object { @{ name = $_.Name; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash } })
        wdk_version = $WdkVersion
        sdk_version = $SdkVersion
        configuration = 'Release|x64'
        signing = 'unsigned'
        hardware_validation = 'deferred'
        artifacts = @($artifacts | ForEach-Object { @{ name = $_.Name; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash; bytes = $_.Length } })
    }
    $receipt | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $outRoot 'build-receipt.json') -Encoding UTF8
    Write-Host 'driver_package=prepared_unsigned installed=false hardware_validation=deferred'
} finally { Pop-Location }
