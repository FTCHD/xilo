# Installs Xilo on Windows for the current user, with PowerShell 5.1 or later.
#
#   irm https://raw.githubusercontent.com/ftchd/xilo/main/install.ps1 | iex
#
# Options, as environment variables:
#   $env:XILO_VERSION = 'v0.2.0'   Install this release instead of the latest one.
#   $env:XILO_NO_OPEN = '1'        Do not open Xilo after installing.
#   $env:XILO_DRY_RUN = '1'        Show what the script would do, and change nothing.
#
# To uninstall. Your settings and history stay, and the script says where they are:
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/ftchd/xilo/main/install.ps1))) -Uninstall
#
# From a downloaded copy: .\install.ps1 -Uninstall

# Everything runs in this block, so that its settings stay out of your session
# and a partial download does nothing.
& {
    param([switch]$Uninstall)

    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    # Windows PowerShell 5.1 does not use TLS 1.2 by default.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $repo = 'ftchd/xilo'
    $releases = if ($env:XILO_DOWNLOAD_URL) { $env:XILO_DOWNLOAD_URL } else { "https://github.com/$repo/releases" }
    $version = if ($env:XILO_VERSION) { $env:XILO_VERSION } else { 'latest' }
    $installDir = Join-Path $env:LOCALAPPDATA 'Programs\Xilo'
    $exe = Join-Path $installDir 'Xilo.exe'
    $binDir = Join-Path $installDir 'bin'
    $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Xilo.lnk'
    # The login item that Xilo writes when you turn on Open at login.
    $runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'

    function Stop-Xilo {
        $running = @(Get-Process -Name Xilo -ErrorAction SilentlyContinue)
        if ($running.Count -gt 0) {
            Write-Host 'Stopping the running Xilo...'
            $running | Stop-Process -Force
            $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
        }
    }

    # The user PATH, as written, so that entries such as %USERPROFILE%\bin stay as they are.
    function Get-UserPath {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
        $value = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $key.Close()
        @($value -split ';' | Where-Object { $_ })
    }

    function Set-UserPath([string[]]$entries) {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        $key.SetValue('Path', ($entries -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
        $key.Close()
        # Changing a user variable through .NET tells Windows to reload the environment,
        # so that new terminals see the new PATH.
        [Environment]::SetEnvironmentVariable('XILO_PATH_CHANGED', '1', 'User')
        [Environment]::SetEnvironmentVariable('XILO_PATH_CHANGED', $null, 'User')
    }

    if ($Uninstall) {
        Stop-Xilo
        $entries = @(Get-UserPath)
        if ($entries -contains $binDir) {
            Set-UserPath @($entries | Where-Object { $_ -ne $binDir })
        }
        Remove-ItemProperty -Path $runKey -Name 'Xilo' -ErrorAction SilentlyContinue
        foreach ($path in @($installDir, $shortcut)) {
            Remove-Item -Recurse -Force -Path $path -ErrorAction SilentlyContinue
        }
        Write-Host 'Xilo is uninstalled.'
        Write-Host "Your settings and history are still in $(Join-Path $env:APPDATA 'xilo\Xilo'). Delete the folder if you do not need them."
        return
    }

    # 32-bit PowerShell on 64-bit Windows reports the real CPU in PROCESSOR_ARCHITEW6432.
    $cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    $arch = switch ($cpu) {
        'AMD64' { 'x64' }
        'ARM64' { 'arm64' }
        default { throw "There is no Xilo build for $cpu." }
    }

    $suffix = "-windows-$arch.zip"
    if ($version -eq 'latest') {
        $base = "$releases/latest/download"
    } else {
        if (-not $version.StartsWith('v')) { $version = "v$version" }
        $base = "$releases/download/$version"
    }

    if ($env:XILO_DRY_RUN) {
        Write-Host "Would download SHA256SUMS.txt and the file that ends with $suffix from $base"
        Write-Host "Would install Xilo to $installDir, with a Start menu entry"
        Write-Host "Would add $binDir to your PATH, for the xilo command"
        if (-not $env:XILO_NO_OPEN) { Write-Host 'Would open Xilo' }
        return
    }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ('xilo-' + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        # The file names have the version in them, so read the name from SHA256SUMS.txt.
        $sums = Join-Path $tmp 'SHA256SUMS.txt'
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS.txt" -OutFile $sums
        } catch {
            throw "Could not download $base/SHA256SUMS.txt. Is there a release yet?"
        }
        $entry = Get-Content $sums | ForEach-Object {
            $fields = $_ -split '\s+'
            if ($fields.Count -ge 2) {
                $name = $fields[1].TrimStart('*')
                if ($name.StartsWith('Xilo-') -and $name.EndsWith($suffix)) {
                    [pscustomobject]@{ Name = $name; Hash = $fields[0].ToLowerInvariant() }
                }
            }
        } | Select-Object -First 1
        if (-not $entry) { throw "The release has no file that ends with $suffix." }

        $asset = $entry.Name
        Write-Host "Downloading $asset..."
        $download = Join-Path $tmp $asset
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile $download
        $actual = (Get-FileHash -Algorithm SHA256 -Path $download).Hash.ToLowerInvariant()
        if ($actual -ne $entry.Hash) {
            throw "The download of $asset is damaged (expected $($entry.Hash), got $actual). Try again."
        }

        $unpacked = Join-Path $tmp 'unpacked'
        Expand-Archive -Path $download -DestinationPath $unpacked
        $folder = Join-Path $unpacked ([IO.Path]::GetFileNameWithoutExtension($asset))
        if (-not (Test-Path (Join-Path $folder 'Xilo.exe'))) { throw "$asset does not contain Xilo.exe." }

        Stop-Xilo
        Remove-Item -Recurse -Force -Path $installDir -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path (Split-Path $installDir) | Out-Null
        Move-Item -Path $folder -Destination $installDir
        # Invoke-WebRequest does not mark the files as downloaded, but clear the mark in case something else did.
        Get-ChildItem -Recurse -File -Path $installDir | Unblock-File -ErrorAction SilentlyContinue
    } finally {
        Remove-Item -Recurse -Force -Path $tmp -ErrorAction SilentlyContinue
    }
    Write-Host "Installed $installDir"

    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($shortcut)
    $link.TargetPath = $exe
    $link.WorkingDirectory = $installDir
    $link.Description = 'Monitor and control Anker SOLIX devices over Bluetooth'
    $link.Save()

    # Lets you type xilo in a terminal.
    $entries = @(Get-UserPath)
    if ($entries -notcontains $binDir) {
        Set-UserPath ($entries + $binDir)
    }
    if (($env:Path -split ';') -notcontains $binDir) {
        $env:Path = "$env:Path;$binDir"
    }

    Write-Host ''
    if (-not $env:XILO_NO_OPEN) {
        Start-Process -FilePath $exe
        Write-Host 'Xilo is open.'
    }
    Write-Host 'To open Xilo when you sign in, turn on Open at login in its Preferences.'
    Write-Host 'Choose Xilo in the Start menu to open it, and type xilo in a new terminal for the command line tool.'
} @args
