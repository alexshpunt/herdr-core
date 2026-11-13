param([Parameter(Mandatory=$true)][string]$Source)
$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ('hc-fork-test-' + [Guid]::NewGuid().ToString('N'))
$installer = Join-Path $PSScriptRoot '..\distribution\fork-install.ps1'
$oldInstall = $env:HC_INSTALL_DIR
$oldBase = $env:HC_FORK_RELEASE_BASE
$oldSession = $env:HC_SESSION
$oldState = $env:XDG_STATE_HOME
$oldConfig = $env:XDG_CONFIG_HOME
$oldSocket = $env:HC_SOCKET
$started = $false
try {
    $assets = Join-Path $root 'assets'
    $stage = Join-Path $root 'stage'
    $dir = Join-Path $root 'install'
    New-Item -ItemType Directory -Force $assets, $stage, $dir | Out-Null
    Copy-Item (Join-Path $source 'herdr-core.exe') $stage
    Copy-Item -Recurse (Join-Path $source 'conpty') $stage
    $zip = Join-Path $assets 'herdr-core-windows-x86_64.zip'
    Compress-Archive -Path "$stage\*" -DestinationPath $zip
    $sum = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
    $checksum = "$zip.sha256"
    [IO.File]::WriteAllText($checksum, ('0' * 64) + "`n")
    $env:HC_INSTALL_DIR = $dir
    $env:HC_FORK_RELEASE_BASE = ([Uri]::new($assets)).AbsoluteUri.TrimEnd('/')
    $old = Join-Path $dir 'herdr-core.exe'
    Copy-Item (Join-Path $source 'herdr-core.exe') $old
    Copy-Item -Recurse (Join-Path $source 'conpty') $dir
    $before = (Get-FileHash $old).Hash
    $rejected = $false
    try { & $installer } catch { $rejected = $true }
    if (-not $rejected -or (Get-FileHash $old).Hash -ne $before) { throw 'checksum mismatch did not preserve the existing binary' }
    [IO.File]::WriteAllText($checksum, $sum + "`n")
    [IO.File]::SetLastWriteTimeUtc($old, [DateTime]::UtcNow.AddDays(-10))
    $stamp = [IO.File]::GetLastWriteTimeUtc($old)
    & $installer
    if ([IO.File]::GetLastWriteTimeUtc($old) -ne $stamp) { throw 'unchanged binary was replaced' }
    Remove-Item $old
    & $installer
    if ((Get-FileHash $old).Hash -ne $before) { throw 'fresh installation does not contain the verified binary' }

    # A changed binary must use the planned update path and keep a real terminal alive.
    $env:HC_SESSION = 'forktest-' + [Guid]::NewGuid().ToString('N')
    $env:XDG_STATE_HOME = Join-Path $root 'state'
    $env:XDG_CONFIG_HOME = Join-Path $root 'config'
    $env:HC_SOCKET = $null
    $conpty = Join-Path $dir 'conpty\conpty.dll'
    $conptyHash = (Get-FileHash (Join-Path $stage 'conpty\conpty.dll')).Hash
    [IO.File]::AppendAllText($conpty, 'test marker')
    & $old server start | Out-Null
    if ($LASTEXITCODE) { throw 'isolated core did not start' }
    $started = $true
    $terminal = (& $old --json run --bg -- powershell.exe -NoProfile -Command 'Start-Sleep 30' | ConvertFrom-Json).address
    if ($LASTEXITCODE -or -not $terminal) { throw 'isolated terminal did not start' }
    & $installer
    $live = (& $old --json get $terminal | ConvertFrom-Json).state
    if ($LASTEXITCODE -or $live -ne 'live') { throw 'updating did not preserve the terminal' }
    if ((Get-FileHash $old).Hash -ne $before) { throw 'update did not install the verified binary' }
    if ((Get-FileHash $conpty).Hash -ne $conptyHash) { throw 'update did not install the verified ConPTY' }
} finally {
    $env:HC_INSTALL_DIR = $oldInstall
    if ($started) { & $old server stop | Out-Null }
    $env:HC_SESSION = $oldSession
    $env:XDG_STATE_HOME = $oldState
    $env:XDG_CONFIG_HOME = $oldConfig
    $env:HC_SOCKET = $oldSocket
    $env:HC_FORK_RELEASE_BASE = $oldBase
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
