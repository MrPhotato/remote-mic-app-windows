#Requires -Version 5.1
<#
.SYNOPSIS
Read-only RC003 filter preflight. Emits JSON without device instance paths.
.DESCRIPTION
Queries only present-device hardware IDs, boot/security state and two driver
service registrations. Does not install drivers, change settings or open HID
input handles. An exact hardware ID identifies a product family, not a model.
#>
[CmdletBinding()]
param(
    [string]$ExpectedHardwareId = 'HID\{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&012717_PID&32b8_REV&00a4'
)

$ErrorActionPreference = 'Stop'
$upstreamHardwareId = 'HID\{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&012717_PID&32b8_REV&00a4'
$familyPattern = '^HID\\\{00001812-0000-1000-8000-00805f9b34fb\}_Dev_VID&012717_PID&32b8(?:_[^\\]*)?$'
$errors = New-Object 'System.Collections.Generic.List[object]'

function Add-ReadError {
    param([string]$Area, [System.Exception]$Exception)
    # Exception messages can contain local paths; only retain a numeric code.
    $errors.Add([ordered]@{ area = $Area; hresult = $Exception.HResult })
}

function Read-ServiceRegistration {
    param([string]$Name)
    try {
        $key = Get-Item -LiteralPath ('HKLM:\SYSTEM\CurrentControlSet\Services\' + $Name) -ErrorAction Stop
        return [ordered]@{ registered = $true; state = 'registered_not_load_verified' }
    } catch [System.Management.Automation.ItemNotFoundException] {
        return [ordered]@{ registered = $false; state = 'not_registered' }
    } catch {
        Add-ReadError -Area ('service_' + $Name) -Exception $_.Exception
        return [ordered]@{ registered = $null; state = 'unknown' }
    }
}

$elevated = $null
try {
    $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        $principal = New-Object System.Security.Principal.WindowsPrincipal($identity)
        $elevated = $principal.IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)
    } finally {
        $identity.Dispose()
    }
} catch {
    Add-ReadError -Area 'elevation' -Exception $_.Exception
}

$devicesKnown = $false
$candidateDevices = @()
$hardwareIds = @()
$exactMatchCount = $null
$upstreamMatchCount = $null
try {
    if ($ExpectedHardwareId -notmatch $familyPattern) {
        throw [System.ArgumentException]::new('ExpectedHardwareId must be a generic target-product HID hardware ID.')
    }
    if (-not ('SayAllRc003Preflight.Native' -as [type])) {
        # Public metadata APIs only: SPDRP_HARDWAREID is REG_MULTI_SZ.
        # No instance ID, friendly name, Bluetooth address or input report API.
        Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

namespace SayAllRc003Preflight {
    public sealed class DeviceMetadata {
        public string DeviceClass;
        public string[] HardwareIds;
    }
    public static class Native {
        [StructLayout(LayoutKind.Sequential)]
        private struct SP_DEVINFO_DATA {
            public UInt32 cbSize;
            public Guid ClassGuid;
            public UInt32 DevInst;
            public UIntPtr Reserved;
        }
        [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr SetupDiGetClassDevsW(IntPtr classGuid,
            string enumerator, IntPtr hwnd, UInt32 flags);
        [DllImport("setupapi.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetupDiEnumDeviceInfo(IntPtr set,
            UInt32 index, ref SP_DEVINFO_DATA data);
        [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetupDiGetDeviceRegistryPropertyW(IntPtr set,
            ref SP_DEVINFO_DATA data, UInt32 property, out UInt32 regType,
            byte[] buffer, UInt32 size, out UInt32 required);
        [DllImport("setupapi.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetupDiDestroyDeviceInfoList(IntPtr set);

        private static string[] ReadHardwareIds(IntPtr set, ref SP_DEVINFO_DATA data) {
            UInt32 regType, required;
            bool sized = SetupDiGetDeviceRegistryPropertyW(set, ref data, 1,
                out regType, null, 0, out required);
            int error = Marshal.GetLastWin32Error();
            if (!sized && error == 13) return new string[0]; // No property.
            if (!sized && error != 122) throw new Win32Exception(error);
            if (required == 0) return new string[0];
            if (required > 65536) throw new InvalidOperationException("Property size exceeds limit.");
            byte[] buffer = new byte[required];
            if (!SetupDiGetDeviceRegistryPropertyW(set, ref data, 1,
                out regType, buffer, (UInt32)buffer.Length, out required))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            if (regType != 7 || required > buffer.Length || (required % 2) != 0)
                throw new InvalidOperationException("Invalid hardware ID property type.");
            return Encoding.Unicode.GetString(buffer, 0, (int)required)
                .Split(new char[] { '\0' }, StringSplitOptions.RemoveEmptyEntries);
        }
        public static DeviceMetadata[] EnumeratePresentHidMetadata() {
            Guid keyboard = new Guid("4d36e96b-e325-11ce-bfc1-08002be10318");
            Guid hid = new Guid("745a17a0-74d3-11d0-b6fe-00a0c90f57da");
            IntPtr set = SetupDiGetClassDevsW(IntPtr.Zero, null, IntPtr.Zero, 6);
            if (set == new IntPtr(-1)) throw new Win32Exception(Marshal.GetLastWin32Error());
            List<DeviceMetadata> results = new List<DeviceMetadata>();
            try {
                for (UInt32 index = 0; ; index++) {
                    SP_DEVINFO_DATA data = new SP_DEVINFO_DATA();
                    data.cbSize = (UInt32)Marshal.SizeOf(typeof(SP_DEVINFO_DATA));
                    if (!SetupDiEnumDeviceInfo(set, index, ref data)) {
                        int error = Marshal.GetLastWin32Error();
                        if (error == 259) break;
                        throw new Win32Exception(error);
                    }
                    if (data.ClassGuid != keyboard && data.ClassGuid != hid) continue;
                    results.Add(new DeviceMetadata {
                        DeviceClass = data.ClassGuid == keyboard ? "Keyboard" : "HIDClass",
                        HardwareIds = ReadHardwareIds(set, ref data)
                    });
                }
            } finally {
                SetupDiDestroyDeviceInfoList(set);
            }
            return results.ToArray();
        }
    }
}
'@
    }
    $candidateDevices = @([SayAllRc003Preflight.Native]::EnumeratePresentHidMetadata() | Where-Object {
        @($_.HardwareIds | Where-Object { $_ -match $familyPattern }).Count -gt 0
    })
    # Output only verified generic product IDs: never instance IDs or paths.
    $hardwareIds = @($candidateDevices | ForEach-Object { $_.HardwareIds } |
        Where-Object { $_ -match $familyPattern } | Sort-Object -Unique)
    $exactMatchCount = @($candidateDevices | Where-Object { $_.HardwareIds -icontains $ExpectedHardwareId }).Count
    $upstreamMatchCount = @($candidateDevices | Where-Object { $_.HardwareIds -icontains $upstreamHardwareId }).Count
    $devicesKnown = $true
} catch {
    Add-ReadError -Area 'hardware_metadata' -Exception $_.Exception
}

$secureBoot = [ordered]@{ state = 'unknown'; source = 'unavailable' }
try {
    $secureBootValue = Get-ItemPropertyValue -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\SecureBoot\State' -Name UEFISecureBootEnabled
    if ($secureBootValue -eq 1 -or $secureBootValue -eq 0) {
        $secureBoot.state = if ($secureBootValue -eq 1) { 'enabled' } else { 'disabled' }
        $secureBoot.source = 'UEFISecureBootEnabled_registry'
    }
} catch {
    Add-ReadError -Area 'secure_boot' -Exception $_.Exception
}

$testSigning = [ordered]@{ state = 'unknown'; query_succeeded = $false; source = 'bcd_current'; exit_code = $null }
try {
    $previousPreference = $ErrorActionPreference
    try {
        # Capture the full command output in memory; never emit its raw text.
        $ErrorActionPreference = 'Continue'
        $bcdOutput = @(& (Join-Path $env:SystemRoot 'System32\bcdedit.exe') /enum '{current}' 2>&1)
        $bcdExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }
    $testSigning.exit_code = $bcdExitCode
    if ($bcdExitCode -eq 0) {
        $testSigning.query_succeeded = $true
        $setting = @($bcdOutput | ForEach-Object { $_.ToString() } | Where-Object { $_ -match '^\s*testsigning\s+' })
        if ($setting.Count -eq 0) {
            $testSigning.state = 'not_explicitly_set'
        } elseif ($setting.Count -eq 1) {
            $value = ($setting[0] -replace '^\s*testsigning\s+', '').Trim()
            if ($value -match '^(Yes|True|On|1)$') { $testSigning.state = 'enabled' }
            elseif ($value -match '^(No|False|Off|0)$') { $testSigning.state = 'disabled' }
        }
    }
} catch {
    Add-ReadError -Area 'test_signing' -Exception $_.Exception
}

$hvci = [ordered]@{ running = 'unknown'; configured = 'unknown'; source = 'Win32_DeviceGuard_and_registry' }
try {
    $guard = Get-CimInstance -Namespace root\Microsoft\Windows\DeviceGuard -ClassName Win32_DeviceGuard -ErrorAction Stop
    if ($null -ne $guard -and $null -ne $guard.SecurityServicesRunning) {
        $hvci.running = if (@($guard.SecurityServicesRunning) -contains 2) { 'enabled' } else { 'disabled' }
    }
} catch {
    Add-ReadError -Area 'hvci_running' -Exception $_.Exception
}
try {
    $hvciValue = Get-ItemPropertyValue -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity' -Name Enabled
    if ($hvciValue -eq 1 -or $hvciValue -eq 0) {
        $hvci.configured = if ($hvciValue -eq 1) { 'enabled' } else { 'disabled' }
    }
} catch {
    Add-ReadError -Area 'hvci_configured' -Exception $_.Exception
}

$sayAllService = Read-ServiceRegistration -Name 'SayAllHidFilter'
$upstreamService = Read-ServiceRegistration -Name 'MiRemoteHidFilter'
[ordered]@{
    schema_version = 1
    read_only = $true
    elevated = $elevated
    model_scope = 'product_family_rc003_only_validated_for_selection'
    rc001_validation = 'deferred'
    model_proven_by_hardware_id = $false
    hardware = [ordered]@{
        query_succeeded = $devicesKnown
        source = 'SetupDi_present_SPDRP_HARDWAREID'
        generic_hardware_ids = $hardwareIds
        candidate_device_count = if ($devicesKnown) { $candidateDevices.Count } else { $null }
        expected_hardware_id = $ExpectedHardwareId
        exact_inf_target_match_count = $exactMatchCount
        unique_inf_target = if ($devicesKnown) { $exactMatchCount -eq 1 } else { $null }
        upstream_hardware_id = $upstreamHardwareId
        upstream_exact_match_count = $upstreamMatchCount
    }
    secure_boot = $secureBoot
    test_signing = $testSigning
    hvci = $hvci
    services = [ordered]@{
        SayAllHidFilter = $sayAllService
        MiRemoteHidFilter = $upstreamService
        upstream_conflict = $upstreamService.registered
    }
    installation_action = 'not_performed_read_only'
    loadability_verified = $false
    errors = @($errors.ToArray())
} | ConvertTo-Json -Depth 8
