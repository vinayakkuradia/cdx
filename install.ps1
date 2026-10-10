# cdx installer for Windows (PowerShell 5.1 and 7+).
#
#   irm https://github.com/__REPO__/releases/latest/download/install.ps1 | iex
#
# Environment variables (set before running):
#   CDX_VERSION            release tag to install, e.g. v0.1.0   (default: latest)
#   CDX_INSTALL_DIR        where to put cdx-bin.exe              (default: %LOCALAPPDATA%\cdx\bin)
#   CDX_NO_MODIFY_SHELL=1  do not touch PowerShell profiles
#   CDX_BASE_URL           override the download location (mirrors, testing)
#   CDX_REPO               GitHub "owner/name" (baked in at release time)
#   CDX_UNINSTALL=1        remove cdx instead of installing it
#
# Note: this script never calls `exit`, because when piped into `iex` that
# would close your terminal window.

function Invoke-CdxInstaller {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'   # progress bars make Invoke-WebRequest very slow on 5.1
    try {
        [Net.ServicePointManager]::SecurityProtocol =
            [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    } catch { }

    $repo       = if ($env:CDX_REPO) { $env:CDX_REPO } else { '__REPO__' }
    $installDir = if ($env:CDX_INSTALL_DIR) { $env:CDX_INSTALL_DIR } else { Join-Path (Join-Path $env:LOCALAPPDATA 'cdx') 'bin' }
    $markBegin  = '# >>> cdx >>>'
    $markEnd    = '# <<< cdx <<<'

    # Every PowerShell profile we may have configured: Windows PowerShell 5.1,
    # PowerShell 7+, and whichever host is running this script.
    function Get-ProfileTargets {
        $docs = [Environment]::GetFolderPath('MyDocuments')
        $list = @(
            (Join-Path (Join-Path $docs 'WindowsPowerShell') 'Microsoft.PowerShell_profile.ps1'),
            (Join-Path (Join-Path $docs 'PowerShell') 'Microsoft.PowerShell_profile.ps1')
        )
        $list | Where-Object { Test-Path (Split-Path $_ -Parent) } |
            ForEach-Object { $_ }
    }

    function Test-SamePath($a, $b) {
        $a.TrimEnd('\') -eq $b.TrimEnd('\')
    }

    # ---------------------------------------------------------------- uninstall
    if ($env:CDX_UNINSTALL -eq '1') {
        Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $installDir 'cdx-bin.exe'), (Join-Path $installDir 'cdx.cmd')
        if ((Test-Path $installDir) -and -not (Get-ChildItem -Force $installDir)) {
            Remove-Item -Force $installDir
        }
        Write-Host "cdx: removed files from $installDir"

        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $kept = @($userPath -split ';' | Where-Object { $_ -and -not (Test-SamePath $_ $installDir) })
        [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')

        $profiles = @(Get-ProfileTargets) + @($PROFILE.CurrentUserCurrentHost) | Select-Object -Unique
        foreach ($p in $profiles) {
            if (-not (Test-Path $p)) { continue }
            $text = [IO.File]::ReadAllText($p)
            if ($text.Contains($markBegin)) {
                $clean = [regex]::Replace($text, '(?s)\r?\n?# >>> cdx >>>.*?# <<< cdx <<<\r?\n?', "`r`n")
                [IO.File]::WriteAllText($p, $clean)
                Write-Host "cdx: cleaned $p"
            }
        }
        Write-Host "cdx: uninstalled. Open a new terminal to finish."
        return
    }

    # ------------------------------------------------------------------ install
    if (-not $repo -or $repo -like '__REPO*') {
        throw "No repository configured. Use the install.ps1 attached to a GitHub release, or set `$env:CDX_REPO = 'owner/name'."
    }

    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    switch ($arch) {
        'X64'   { $target = 'x86_64-pc-windows-msvc' }
        'Arm64' { $target = 'aarch64-pc-windows-msvc' }
        default { throw "Unsupported CPU architecture: $arch" }
    }
    $asset = "cdx-$target.zip"

    if ($env:CDX_BASE_URL)    { $base = $env:CDX_BASE_URL }
    elseif ($env:CDX_VERSION) { $base = "https://github.com/$repo/releases/download/$($env:CDX_VERSION)" }
    else                      { $base = "https://github.com/$repo/releases/latest/download" }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("cdx-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        $zip = Join-Path $tmp $asset
        Write-Host "cdx: downloading $asset"
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile $zip
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset.sha256" -OutFile "$zip.sha256"

        $want = ((Get-Content -Raw "$zip.sha256").Trim() -split '\s+')[0].ToLower()
        $got  = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
        if ($want -ne $got) { throw "Checksum mismatch (expected $want, got $got). Aborting." }
        Write-Host "cdx: checksum verified"

        Expand-Archive -Path $zip -DestinationPath $tmp -Force
        New-Item -ItemType Directory -Force -Path $installDir | Out-Null
        $exe = Join-Path $installDir 'cdx-bin.exe'
        Copy-Item (Join-Path (Join-Path $tmp "cdx-$target") 'cdx-bin.exe') $exe -Force
        Write-Host "cdx: installed $exe"

        # cmd.exe has no functions, so give it a cdx.cmd wrapper next to the binary.
        & $exe init cmd | Set-Content -Path (Join-Path $installDir 'cdx.cmd') -Encoding ASCII
    }
    finally {
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
    }

    # Put the install folder on the user's PATH (current session and future ones).
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @($userPath -split ';' | Where-Object { $_ })
    if (-not ($parts | Where-Object { Test-SamePath $_ $installDir })) {
        [Environment]::SetEnvironmentVariable('Path', (($parts + $installDir) -join ';'), 'User')
        Write-Host "cdx: added $installDir to your PATH"
    }
    if (-not (($env:Path -split ';') | Where-Object { $_ -and (Test-SamePath $_ $installDir) })) {
        $env:Path = "$installDir;$env:Path"
    }

    # PowerShell integration.
    if ($env:CDX_NO_MODIFY_SHELL -eq '1') {
        Write-Host "cdx: skipping profile changes (CDX_NO_MODIFY_SHELL=1)"
    }
    else {
        $block = "`r`n$markBegin`r`n" +
                 "if (Get-Command cdx-bin -ErrorAction SilentlyContinue) { Invoke-Expression (& cdx-bin init powershell | Out-String) }`r`n" +
                 "$markEnd`r`n"
        $profiles = @(Get-ProfileTargets) + @($PROFILE.CurrentUserCurrentHost) | Select-Object -Unique
        foreach ($p in $profiles) {
            $dir = Split-Path $p -Parent
            if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
            if ((Test-Path $p) -and ([IO.File]::ReadAllText($p)).Contains($markBegin)) {
                Write-Host "cdx: already configured: $p"
            } else {
                [IO.File]::AppendAllText($p, $block)
                Write-Host "cdx: updated $p"
            }
        }

        $policy = Get-ExecutionPolicy
        if ($policy -eq 'Restricted' -or $policy -eq 'AllSigned') {
            Write-Host ""
            Write-Host "cdx: note: PowerShell's execution policy is '$policy', which blocks profile scripts."
            Write-Host "     To allow them, run:  Set-ExecutionPolicy -Scope CurrentUser RemoteSigned"
        }
    }

    Write-Host ""
    Write-Host "cdx: done. Open a new terminal and type: cdx"
}

Invoke-CdxInstaller
