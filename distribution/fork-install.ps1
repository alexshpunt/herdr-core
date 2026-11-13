# Installs the personal fork's latest Windows build. Running terminals survive updates.
# HC_INSTALL_DIR selects another destination. HC_FORK_RELEASE_BASE selects an
# already pinned release URL, for a mirror or an isolated installer test.
& {
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
function Fail($why) { throw "hc: $why" }
function Hash($path) {
    $stream = [IO.File]::OpenRead($path)
    try { -join ([Security.Cryptography.SHA256]::Create().ComputeHash($stream) | ForEach-Object { $_.ToString('x2') }) }
    finally { $stream.Close() }
}

$dir = if ($env:HC_INSTALL_DIR) { $env:HC_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\herdr-core' }
$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($arch -ne 'AMD64') { Fail "no Windows build for $arch" }
$tar = Join-Path $env:SystemRoot 'System32\tar.exe'
if (-not (Get-Command curl.exe -ErrorAction SilentlyContinue) -or -not (Test-Path $tar)) { Fail 'needs curl.exe and Windows tar.exe' }

if ($env:HC_FORK_RELEASE_BASE) {
    $base = $env:HC_FORK_RELEASE_BASE.TrimEnd('/')
} else {
    $release = Invoke-RestMethod 'https://api.github.com/repos/alexshpunt/herdr-core/releases/latest' -Headers @{ 'User-Agent' = 'herdr-core-installer' }
    if ($release.tag_name -notmatch '^fork-[0-9]+-[0-9]+$') { Fail 'latest release is not a fork build' }
    # Pin both downloads to one release, even if latest changes between requests.
    $base = "https://github.com/alexshpunt/herdr-core/releases/download/$($release.tag_name)"
}
$name = 'herdr-core-windows-x86_64.zip'
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('hc-fork-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $tmp | Out-Null
try {
    curl.exe -fsSL "$base/$name.sha256" -o "$tmp\sha256"
    if ($LASTEXITCODE) { Fail 'cannot download checksum' }
    $want = (([IO.File]::ReadAllText("$tmp\sha256")) -split '\s+')[0].ToLowerInvariant()
    if ($want -notmatch '^[0-9a-f]{64}$') { Fail 'invalid checksum' }
    curl.exe -fsSL "$base/$name" -o "$tmp\hc.zip"
    if ($LASTEXITCODE) { Fail 'cannot download core' }
    if ((Hash "$tmp\hc.zip") -ne $want) { Fail 'checksum mismatch' }
    New-Item -ItemType Directory -Force "$tmp\hc" | Out-Null
    & $tar -xf "$tmp\hc.zip" -C "$tmp\hc"
    $new = "$tmp\hc\herdr-core.exe"
    if ($LASTEXITCODE -or -not (Test-Path $new) -or -not (Test-Path "$tmp\hc\conpty\conpty.dll") -or -not (Test-Path "$tmp\hc\conpty\x64\OpenConsole.exe")) { Fail 'the archive is incomplete' }
    & $new --version | Out-Null
    if ($LASTEXITCODE) { Fail 'the downloaded core does not run' }
    New-Item -ItemType Directory -Force $dir | Out-Null
    $exe = Join-Path $dir 'herdr-core.exe'
    $same = Test-Path $exe
    if ($same) {
        foreach ($file in @('herdr-core.exe', 'conpty\conpty.dll', 'conpty\x64\OpenConsole.exe')) {
            $installed = Join-Path $dir $file
            $incoming = Join-Path "$tmp\hc" $file
            if (-not (Test-Path $installed) -or (Hash $installed) -ne (Hash $incoming)) { $same = $false; break }
        }
    }
    if ($same) {
        Write-Host 'The fork core is already installed.'
    } elseif (Test-Path $exe) {
        & $exe update --from $new
        if ($LASTEXITCODE) { Fail 'updating core failed' }
    } else {
        New-Item -ItemType Directory -Force (Join-Path $dir 'conpty\x64') | Out-Null
        Copy-Item -Force $new $exe
        Copy-Item -Force "$tmp\hc\conpty\conpty.dll" (Join-Path $dir 'conpty\conpty.dll')
        Copy-Item -Force "$tmp\hc\conpty\x64\OpenConsole.exe" (Join-Path $dir 'conpty\x64\OpenConsole.exe')
        Write-Host "Fork core installed into $dir."
    }
    Copy-Item -Recurse -Force "$tmp\hc\THIRD-PARTY-NOTICES" $dir -ErrorAction SilentlyContinue
    if (-not (Get-Command hc -ErrorAction SilentlyContinue) -and -not (Test-Path (Join-Path $dir 'hc.exe'))) {
        New-Item -ItemType HardLink -Path (Join-Path $dir 'hc.exe') -Target $exe | Out-Null
    }
    if (-not $env:HC_INSTALL_DIR) {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        try {
            $raw = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $listed = $raw -split ';' | Where-Object { $_.Trim().TrimEnd('\') -ieq $dir.TrimEnd('\') }
            if (-not $listed) {
                $kind = if ($key.GetValueNames() -contains 'Path') { $key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
                $key.SetValue('Path', $(if ($raw) { "$($raw.TrimEnd(';'));$dir" } else { $dir }), $kind)
                [Environment]::SetEnvironmentVariable('HC_INSTALLING', '1', 'User')
                [Environment]::SetEnvironmentVariable('HC_INSTALLING', $null, 'User')
            }
        } finally { $key.Close() }
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
}
