[CmdletBinding()]
param(
    [string]$RemoteHost = "myserver",
    [string]$RemoteDir = "/home/haru/discord_ai_bot_2",
    [string]$ServiceName = "discord-ai-bot",
    [string]$IdentityFile = "", # 秘密鍵パス (Windows パス 'C:\Users\harun\.ssh\id_rsa' または WSL パス '~/.ssh/id_rsa')
    [string]$WslDistro = "",
    [switch]$SkipRestart = $false
)

$ErrorActionPreference = "Stop"

# ==============================================================================
# Global Configuration & Validation
# ==============================================================================
$ProjectDir = (Resolve-Path $PSScriptRoot).Path
$BinaryName = "discord_ai_bot"
$Timestamp = Get-Date -Format "yyyyMMdd_HHmmss"

# 秘密鍵パスの事前検証 (Windows パスと思われる場合は存在確認)
$ResolvedIdentityFile = $IdentityFile
if ($IdentityFile) {
    $isWindowsPath = $IdentityFile -match "^[a-zA-Z]:" -or $IdentityFile.Contains("\") -or (Test-Path $IdentityFile)
    if ($isWindowsPath) {
        if (-not (Test-Path $IdentityFile)) {
            throw "[ERROR] Specified Windows IdentityFile was not found: $IdentityFile"
        }
        $ResolvedIdentityFile = (Resolve-Path $IdentityFile).Path
    }
}

# コミット情報の取得
$CommitHash = git -C "$ProjectDir" rev-parse --short HEAD 2>$null
$CommitSummary = git -C "$ProjectDir" log -1 --format="%h - %s (%cr) <%an>" 2>$null

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host " Discord AI Bot Native Server Deployment via SSH" -ForegroundColor Cyan
Write-Host " Target: $($RemoteHost):$($RemoteDir) (Service: $($ServiceName))" -ForegroundColor Cyan
if ($CommitSummary) {
    Write-Host " Commit: $CommitSummary" -ForegroundColor Cyan
}
if ($ResolvedIdentityFile) {
    Write-Host " Key:    $ResolvedIdentityFile" -ForegroundColor Cyan
}
Write-Host "================================================================" -ForegroundColor Cyan

# ==============================================================================
# Helper Functions
# ==============================================================================

function Convert-ToWslPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not $Path) { return "" }
    if ($Path.StartsWith("/") -or $Path.StartsWith("~")) { return $Path }

    $fullWinPath = if (Test-Path $Path) { (Resolve-Path $Path).Path } else { $Path }
    if ($fullWinPath -match "^([a-zA-Z]):\\?(.*)$") {
        $driveLetter = $Matches[1].ToLower()
        $subPath = $Matches[2] -replace "\\", "/"
        return "/mnt/$driveLetter/$subPath"
    }

    try {
        $wslOut = if ($WslDistro) { & wsl.exe -d $WslDistro -- wslpath -a -u "$fullWinPath" 2>$null } else { & wsl.exe -- wslpath -a -u "$fullWinPath" 2>$null }
        if ($wslOut) { return ($wslOut | Out-String).Trim() }
    }
    catch {}

    throw "Failed to convert path to WSL format: $Path"
}

function Invoke-WSL {
    param([Parameter(Mandatory = $true)][string]$Command)

    $processInfo = New-Object System.Diagnostics.ProcessStartInfo
    $processInfo.FileName = "wsl.exe"
    if ($WslDistro) {
        $processInfo.Arguments = "-d $WslDistro -- bash -s"
    } else {
        $processInfo.Arguments = "-- bash -s"
    }
    $processInfo.RedirectStandardInput = $true
    $processInfo.UseShellExecute = $false

    $process = [System.Diagnostics.Process]::Start($processInfo)
    $writer = [System.IO.StreamWriter]$process.StandardInput
    $writer.Write("set -e`n$Command")
    $writer.Close()
    $process.WaitForExit()

    if ($process.ExitCode -ne 0) {
        throw "WSL command failed with exit code $($process.ExitCode)"
    }
}

function Test-GitWorkingTree {
    Write-Host "`n[1/3] Checking Git working tree..." -ForegroundColor Yellow
    Write-Host "  -> Deploy target commit: $CommitSummary" -ForegroundColor Gray

    $status = git -C "$ProjectDir" status --porcelain
    if ($status) {
        Write-Host ""
        Write-Host "[WARNING] Working tree has uncommitted changes:" -ForegroundColor Yellow
        Write-Host $status
        Write-Host ""
        $answer = Read-Host "Deploy committed HEAD ($CommitHash) anyway? [y/N]"
        if ($answer -ne "y") {
            throw "Deployment cancelled because working tree is dirty."
        }
    }
    Write-Host "  -> Git working tree is clean or confirmed." -ForegroundColor Green
}

function Deploy-SourceAndBuildOnServer {
    Write-Host "`n[2/3] Syncing source code to remote server ($RemoteHost)..." -ForegroundColor Yellow

    $wslProjectPath = Convert-ToWslPath -Path $ProjectDir
    $wslKeyPath = if ($ResolvedIdentityFile) { Convert-ToWslPath -Path $ResolvedIdentityFile } else { "" }

    Invoke-WSL @"
set -e

SSH_KEY_ARG=""
if [ -n '$wslKeyPath' ]; then
    SSH_KEY_ARG="-i '$wslKeyPath'"
fi

SSH_CMD="ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new `$SSH_KEY_ARG"

# 1. リモートに必要なディレクトリを作成
`$SSH_CMD '$RemoteHost' "mkdir -p '$RemoteDir/src' '$RemoteDir/data'"

# 2. 最新のソースコードをストリーム転送してリモートで展開
cd '$wslProjectPath'
git archive HEAD | `$SSH_CMD '$RemoteHost' "tar -xf - -C '$RemoteDir'"

echo "  -> Source synced successfully to ${RemoteHost}:${RemoteDir}"
"@

    Write-Host "`n[3/3] Building binary natively on remote server ($RemoteHost)..." -ForegroundColor Yellow

    Invoke-WSL @"
set -e

SSH_KEY_ARG=""
if [ -n '$wslKeyPath' ]; then
    SSH_KEY_ARG="-i '$wslKeyPath'"
fi

SSH_CMD="ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new `$SSH_KEY_ARG"

`$SSH_CMD '$RemoteHost' "
    set -e
    [ -f "`$HOME/.cargo/env" ] && . "`$HOME/.cargo/env"
    export PATH="`$HOME/.cargo/bin:`$PATH"

    if ! command -v cargo >/dev/null 2>&1; then
        echo '[INFO] Installing Rust/Cargo on remote server...'
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        . "`$HOME/.cargo/env"
    fi

    cd '$RemoteDir'
    echo '[INFO] Running native cargo build --release...'
    cargo build --release --bin '$BinaryName'
    chmod +x '$RemoteDir/target/release/$BinaryName'

    $(if (-not $SkipRestart) {
    "echo '[INFO] Restarting $ServiceName.service...'
    sudo -n /usr/bin/systemctl restart $ServiceName
    sudo -n /usr/bin/systemctl is-active --quiet $ServiceName
    echo '  -> $ServiceName is active and running!'"
    })
"
"@
    Write-Host "  -> Server build and service restart completed successfully." -ForegroundColor Green
}

# ==============================================================================
# Main Deployment Workflow
# ==============================================================================

$succeeded = $false

try {
    Test-GitWorkingTree
    Deploy-SourceAndBuildOnServer
    $succeeded = $true
}
catch {
    Write-Host "`n[ERROR] Deployment failed: $_" -ForegroundColor Red
    throw
}
finally {
    if ($succeeded) {
        Write-Host "`n================================================================" -ForegroundColor Cyan
        Write-Host " Deployment completed successfully for commit: $CommitHash" -ForegroundColor Cyan
        Write-Host "================================================================" -ForegroundColor Cyan
    }
}
