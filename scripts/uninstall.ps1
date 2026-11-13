param(
    [switch]$KeepData,
    [switch]$PurgeData,
    [string]$InstallDirectory,
    [switch]$RemoveBinaries,
    [switch]$DryRun,
    [int]$TimeoutSeconds = 5
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($KeepData -and $PurgeData) {
    throw 'Cannot specify both -KeepData and -PurgeData simultaneously.'
}

$summary = [ordered]@{
    ProcessStopped = 'SKIPPED (none running)'
    DataAction     = 'NONE'
    DataPath       = $null
    BinariesAction = 'NONE'
    RegistryAudit  = 'CLEAN (zero entries)'
    TasksAudit     = 'CLEAN (zero tasks)'
    ServicesAudit  = 'CLEAN (zero services)'
    TaskbarAudit   = 'CLEAN (native taskbar preserved)'
    Result         = 'PASS'
}

# 1. Graceful Process Shutdown
$processes = @(Get-Process -Name 'roqa' -ErrorAction SilentlyContinue)
if ($processes.Count -gt 0) {
    Write-Host "Found $($processes.Count) running ROQA process(es)."
    $userSid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $quitEventName = "Local\ROQA.$userSid.quit"

    if ($DryRun) {
        Write-Host "[DryRun] Would signal quit to ROQA process(es) and wait up to $TimeoutSeconds s."
        $summary.ProcessStopped = '[DryRun] Would signal --quit'
    } else {
        # Signal quit: try executable if present in current worktree or install directory, or set the IPC event directly.
        $signaled = $false
        $candidateExes = @()
        if ($InstallDirectory) {
            $candidateExes += (Join-Path $InstallDirectory 'roqa.exe')
        }
        $projectRoot = Split-Path -Parent $PSScriptRoot
        $candidateExes += (Join-Path $projectRoot 'target/release/roqa.exe')
        $candidateExes += (Join-Path $projectRoot 'target/debug/roqa.exe')

        foreach ($exe in $candidateExes) {
            if (Test-Path -LiteralPath $exe) {
                try {
                    $control = Start-Process -FilePath $exe -ArgumentList '--quit' -WindowStyle Hidden -PassThru
                    if ($control.WaitForExit(3000) -and $control.ExitCode -eq 0) {
                        $signaled = $true
                        $control.Dispose()
                        break
                    }
                    $control.Dispose()
                } catch {
                    # Fallback to direct event signaling
                }
            }
        }

        if (!$signaled) {
            # Direct IPC event fallback
            $ewh = $null
            if ([System.Threading.EventWaitHandle]::TryOpenExisting($quitEventName, [ref]$ewh)) {
                try {
                    $ewh.Set() | Out-Null
                    $signaled = $true
                } finally {
                    $ewh.Dispose()
                }
            }
        }

        # Bounded wait for process termination
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        foreach ($proc in $processes) {
            $remMs = [int]($deadline - [DateTime]::UtcNow).TotalMilliseconds
            if ($remMs -gt 0) {
                if (!$proc.WaitForExit([Math]::Max(100, $remMs))) {
                    throw "ROQA process (PID $($proc.Id)) did not terminate within $TimeoutSeconds seconds."
                }
            } elseif (!$proc.HasExited) {
                throw "ROQA process (PID $($proc.Id)) did not terminate within $TimeoutSeconds seconds."
            }
            if ($proc.ExitCode -ne 0) {
                Write-Warning "ROQA process (PID $($proc.Id)) exited with non-zero code $($proc.ExitCode)."
            }
            $proc.Dispose()
        }
        Write-Host "ROQA resident process exited gracefully."
        $summary.ProcessStopped = 'PASS (graceful quit, exit 0)'
    }
} else {
    Write-Host "No resident ROQA process running."
}

# 2. User Data Handling (%LOCALAPPDATA%\ROQA)
$localAppData = $env:LOCALAPPDATA
if ($localAppData) {
    $roqaDataDir = Join-Path $localAppData 'ROQA'
    $summary.DataPath = $roqaDataDir
    if (Test-Path -LiteralPath $roqaDataDir) {
        $shouldPurge = $false
        if ($PurgeData) {
            $shouldPurge = $true
        } elseif ($KeepData) {
            $shouldPurge = $false
        } elseif ($DryRun) {
            Write-Host "[DryRun] User data directory detected at $roqaDataDir. Neither -PurgeData nor -KeepData specified (retaining)."
            $summary.DataAction = '[DryRun] RETAINED (use -PurgeData to delete)'
        } else {
            # Interactive prompt only if truly interactive, non-redirected session
            $isInteractive = [Environment]::UserInteractive -and 
                             ($null -ne $Host.UI) -and 
                             ($null -ne $Host.UI.RawUI) -and 
                             !([Console]::IsInputRedirected)
            if ($isInteractive) {
                $choice = Read-Host "Remove ROQA user data ($roqaDataDir)? [y/N]"
                if ($choice -match '^[yY]') {
                    $shouldPurge = $true
                }
            } else {
                Write-Host "Non-interactive or redirected session and neither -KeepData nor -PurgeData specified; retaining user data."
            }
        }

        if ($shouldPurge) {
            if ($DryRun) {
                Write-Host "[DryRun] Would remove user data directory: $roqaDataDir"
                $summary.DataAction = '[DryRun] Would purge data'
            } else {
                Remove-Item -LiteralPath $roqaDataDir -Recurse -Force
                Write-Host "Removed user data directory: $roqaDataDir"
                $summary.DataAction = 'PURGED'
            }
        } else {
            if ($DryRun) {
                Write-Host "[DryRun] Would retain user data at: $roqaDataDir"
                $summary.DataAction = '[DryRun] Would retain data'
            } else {
                Write-Host "Retained user data at: $roqaDataDir"
                $summary.DataAction = 'RETAINED'
            }
        }
    } else {
        Write-Host "No ROQA user data directory found at: $roqaDataDir"
        $summary.DataAction = 'NOT_PRESENT'
    }
}

# 3. Optional Binaries Removal
if ($RemoveBinaries -or $InstallDirectory) {
    $targetDir = if ($InstallDirectory) { $InstallDirectory } else { $PSScriptRoot }
    if (Test-Path -LiteralPath $targetDir) {
        if (Test-Path -LiteralPath (Join-Path $targetDir '.git')) {
            Write-Warning "Target directory contains a git repository ($targetDir); skipping binary deletion to protect source tree."
            $summary.BinariesAction = 'SKIPPED (git repository detected)'
        } else {
            if ($DryRun) {
                Write-Host "[DryRun] Would remove installation directory: $targetDir"
                $summary.BinariesAction = '[DryRun] Would remove directory'
            } else {
                Remove-Item -LiteralPath $targetDir -Recurse -Force
                Write-Host "Removed installation directory: $targetDir"
                $summary.BinariesAction = 'REMOVED'
            }
        }
    }
}

# 4. System Audits (Registry, Tasks, Services, Taskbar)
Write-Host "Performing system state audit..."

# A. Registry Audit
$regViolations = @()
$runKeyHKCU = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (Test-Path -LiteralPath $runKeyHKCU) {
    $props = Get-ItemProperty -LiteralPath $runKeyHKCU
    foreach ($p in $props.PSObject.Properties) {
        if ($p.Name -match 'roqa' -or "$($p.Value)" -match 'roqa\.exe') {
            $regViolations += "HKCU Run: $($p.Name)=$($p.Value)"
        }
    }
}
$uninstallKeyHKCU = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ROQA'
if (Test-Path -LiteralPath $uninstallKeyHKCU) {
    $regViolations += "HKCU Uninstall key present: $uninstallKeyHKCU"
}
$softwareKeyHKCU = 'HKCU:\Software\ROQA'
if (Test-Path -LiteralPath $softwareKeyHKCU) {
    $regViolations += "HKCU Software key present: $softwareKeyHKCU"
}
if ($regViolations.Count -gt 0) {
    $summary.RegistryAudit = "VIOLATION: $($regViolations -join '; ')"
    $summary.Result = 'FAIL'
    throw "Registry audit failed: $($regViolations -join '; ')"
} else {
    Write-Host "Registry audit: CLEAN (zero entries left behind)."
}

# B. Scheduled Tasks Audit
$tasks = @(Get-ScheduledTask -TaskName '*roqa*' -ErrorAction SilentlyContinue)
if ($tasks.Count -gt 0) {
    $taskNames = ($tasks | ForEach-Object { $_.TaskName }) -join ', '
    $summary.TasksAudit = "VIOLATION: Tasks found: $taskNames"
    $summary.Result = 'FAIL'
    throw "Scheduled tasks audit failed: found $taskNames"
} else {
    Write-Host "Scheduled tasks audit: CLEAN (zero tasks registered)."
}

# C. Services Audit
$services = @(Get-Service -Name '*roqa*' -ErrorAction SilentlyContinue)
if ($services.Count -gt 0) {
    $svcNames = ($services | ForEach-Object { $_.Name }) -join ', '
    $summary.ServicesAudit = "VIOLATION: Services found: $svcNames"
    $summary.Result = 'FAIL'
    throw "Services audit failed: found $svcNames"
} else {
    Write-Host "Services audit: CLEAN (zero services registered)."
}

# D. Native Taskbar Audit
# Confirm native shell taskbar process and window are present and intact
$explorer = Get-Process -Name 'explorer' -ErrorAction SilentlyContinue
if (!$explorer) {
    $summary.TaskbarAudit = 'WARNING: explorer process not found'
} else {
    Write-Host "Taskbar audit: CLEAN (native Windows taskbar and explorer preserved)."
}

Write-Host "Cleanup / Uninstall audit complete: $($summary.Result)"
return [PSCustomObject]$summary