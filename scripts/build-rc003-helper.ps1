param([string]$Python = 'python')
$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$sourceRoot = Join-Path $repoRoot 'helpers/rc003-input'
$outputRoot = Join-Path $repoRoot 'target/rc003-helper'
$venvRoot = Join-Path $outputRoot 'build-venv'
$logRoot = Join-Path $outputRoot 'logs'
New-Item -ItemType Directory -Path $outputRoot, $logRoot -Force | Out-Null

function Invoke-Logged([string]$Program, [string[]]$CommandArguments, [string]$LogName) {
    $logFile = Join-Path $logRoot $LogName
    $savedPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $Program @CommandArguments *> $logFile }
    finally { $ErrorActionPreference = $savedPreference }
    if ($LASTEXITCODE -ne 0) {
        Get-Content -LiteralPath $logFile -Tail 30
        throw "Helper build failed: $LogName"
    }
}

$pythonCheck = "import sys,struct; assert sys.platform == 'win32' and struct.calcsize('P') == 8 and sys.version_info[:2] >= (3,11), 'requires Windows x64 Python 3.11 or later'"
Invoke-Logged $Python @('-B', '-c', $pythonCheck) 'python-check.log'
$venvPython = Join-Path $venvRoot 'Scripts/python.exe'
if (-not (Test-Path -LiteralPath $venvPython)) {
    Invoke-Logged $Python @('-m', 'venv', $venvRoot) 'venv.log'
}
Invoke-Logged $venvPython @('-B', '-c', $pythonCheck) 'venv-check.log'
Invoke-Logged $venvPython @('-m', 'pip', '--disable-pip-version-check', '--require-virtualenv', 'install',
    '--require-hashes', '--only-binary=:all:', '-r', (Join-Path $sourceRoot 'requirements-build.txt')) 'dependencies.log'
Invoke-Logged $venvPython @('-B', '-m', 'unittest', 'discover', '-s', (Join-Path $sourceRoot 'tests'), '-p', 'test_*.py') 'tests.log'
Invoke-Logged 'node' @((Join-Path $sourceRoot 'tests/observer.test.cjs')) 'observer-tests.log'

$runName = [Guid]::NewGuid().ToString('N')
$stagingRoot = Join-Path $outputRoot "build-dist/$runName"
$workRoot = Join-Path $outputRoot 'pyinstaller-work'
Invoke-Logged $venvPython @('-m', 'PyInstaller', '--noconfirm', '--distpath', $stagingRoot,
    '--workpath', $workRoot, (Join-Path $sourceRoot 'SayAllKeyHelper.spec')) 'pyinstaller.log'
$stagedBundle = Join-Path $stagingRoot 'SayAllKeyHelper'
$stagedManifest = Join-Path $stagingRoot 'manifest.json'
Invoke-Logged $venvPython @('-B', (Join-Path $sourceRoot 'bundle_manifest.py'), $stagedBundle, $stagedManifest) 'manifest.log'
Copy-Item -LiteralPath $stagedManifest -Destination (Join-Path $stagedBundle 'manifest.json')

$finalBundle = [IO.Path]::GetFullPath((Join-Path $outputRoot 'SayAllKeyHelper'))
$allowedRoot = [IO.Path]::GetFullPath($outputRoot).TrimEnd('\') + '\'
if (-not $finalBundle.StartsWith($allowedRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Bundle destination escaped the intended output directory.'
}
if (-not ([IO.Path]::GetFullPath($stagedBundle)).StartsWith($allowedRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Staged bundle escaped the intended output directory.'
}
if (Test-Path -LiteralPath $finalBundle) {
    if ((Get-Item -LiteralPath $finalBundle).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw 'Existing bundle is a reparse point.'
    }
    # Preserve the previous artifact in the system recycle bin, per repository policy.
    Add-Type -AssemblyName Microsoft.VisualBasic
    [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($finalBundle,
        [Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs,
        [Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin)
}
Move-Item -LiteralPath $stagedBundle -Destination $finalBundle
Copy-Item -LiteralPath $stagedManifest -Destination (Join-Path $outputRoot 'manifest.json') -Force
Write-Output 'RC003 helper: dependency checks, simulated tests, bundle and integrity manifest passed.'
Write-Output 'Output: target/rc003-helper/SayAllKeyHelper/SayAllKeyHelper.exe'
Write-Output 'Manifest: target/rc003-helper/manifest.json'
