[CmdletBinding()]
param(
    [string]$RemoteHost = "discord_bot",
    [string]$RemoteDir = "/home/haru/discord_ai_bot_2",
    [string]$ServiceName = "discord-ai-bot",
    [string]$IdentityFile = "", # オプション: 秘密鍵パス (未指定時は ~/.ssh/config やデフォルト鍵を使用)
    [switch]$SkipRestart = $false
)

$ErrorActionPreference = "Stop"

# ==============================================================================
# Global Configuration & Validation
# ==============================================================================
$ProjectDir = (Resolve-Path $PSScriptRoot).Path
$BinaryName = "discord_ai_bot"

# コミット情報の取得
$CommitHash = git -C "$ProjectDir" rev-parse --short HEAD 2>$null
$CommitSummary = git -C "$ProjectDir" log -1 --format="%h - %s (%cr) <%an>" 2>$null

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host " Discord AI Bot Fast Native Deployment" -ForegroundColor Cyan
Write-Host " Target:  ${RemoteHost}:${RemoteDir} (Service: ${ServiceName})" -ForegroundColor Cyan
if ($CommitSummary) {
    Write-Host " Commit:  $CommitSummary" -ForegroundColor Cyan
}
if ($IdentityFile) {
    Write-Host " Key:     $IdentityFile" -ForegroundColor Cyan
}
Write-Host "================================================================" -ForegroundColor Cyan

# SSH コマンドの組み立て
$sshArgs = @("-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=accept-new")
if ($IdentityFile) {
    $sshArgs += @("-i", $IdentityFile)
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

function Sync-SourceCode {
    Write-Host "`n[2/3] Syncing source code to remote server ($RemoteHost)..." -ForegroundColor Yellow

    # リモートに必要なディレクトリを作成
    $mkdirCmd = "mkdir -p '$RemoteDir/src' '$RemoteDir/data'"
    & ssh @sshArgs $RemoteHost $mkdirCmd
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to prepare remote directory: $RemoteDir"
    }

    # git archive をリモートへストリーム転送して展開
    $tarCmd = "tar -xf - -C '$RemoteDir'"
    & git -C "$ProjectDir" archive HEAD | & ssh @sshArgs $RemoteHost $tarCmd
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to stream source archive to remote server."
    }

    Write-Host "  -> Source synced successfully to ${RemoteHost}:${RemoteDir}" -ForegroundColor Green
}

function Build-AndRestartOnServer {
    Write-Host "`n[3/3] Building binary natively on remote server ($RemoteHost)..." -ForegroundColor Yellow

    $remoteScript = @"
set -e

# Cargo の PATH 設定 (未インストールの場合は自動インストール)
if [ -f "`$HOME/.cargo/env" ]; then
    . "`$HOME/.cargo/env"
fi
export PATH="`$HOME/.cargo/bin:`$PATH"

if ! command -v cargo >/dev/null 2>&1; then
    echo '[INFO] Rust/Cargo not found. Installing Rust on remote server...'
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    . "`$HOME/.cargo/env"
fi

cd '$RemoteDir'
echo '[INFO] Running native cargo build --release...'
cargo build --release --bin '$BinaryName'
chmod +x '$RemoteDir/target/release/$BinaryName'
echo '  -> Binary built successfully: $RemoteDir/target/release/$BinaryName'

$(if (-not $SkipRestart) {
@"
echo '[INFO] Restarting $ServiceName.service...'
sudo -n /usr/bin/systemctl restart $ServiceName
sudo -n /usr/bin/systemctl is-active --quiet $ServiceName
echo '  -> $ServiceName is active and running!'
"@
})
"@

    & ssh @sshArgs $RemoteHost $remoteScript
    if ($LASTEXITCODE -ne 0) {
        throw "Server build or service restart failed with exit code $LASTEXITCODE"
    }

    Write-Host "  -> Server build and service restart completed successfully." -ForegroundColor Green
}

# ==============================================================================
# Main Deployment Workflow
# ==============================================================================

$succeeded = $false

try {
    Test-GitWorkingTree
    Sync-SourceCode
    Build-AndRestartOnServer
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
