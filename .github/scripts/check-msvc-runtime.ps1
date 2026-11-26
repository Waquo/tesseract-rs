# Checks which C runtime the bundled Leptonica and Tesseract libraries were
# compiled against, by reading the linker directives embedded in their object
# files. Needs dumpbin on PATH (MSVC developer environment).
#
#   -Expect dynamic   /MD: objects ask for MSVCRT
#   -Expect static    /MT: objects ask for LIBCMT
#
# Pass the CLI build output from cargo build --message-format=json as -CargoMessages.
# With -Exe, the DLL imports of a linked executable are listed too, and for
# the static runtime it must not import any C runtime DLL.
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('dynamic', 'static')]
    [string]$Expect,

    [Parameter(Mandatory = $true)]
    [string[]]$CargoMessages,

    [string]$Exe
)

$ErrorActionPreference = 'Stop'

# Cargo may have several OUT_DIRs for this package because the CLI and
# tests use different features. Read the directory for this CLI build from
# Cargo's build-script message, which is emitted on fresh and reused builds.
$nativeBuilds = @($CargoMessages | ForEach-Object { ConvertFrom-Json -InputObject $_ } |
    Where-Object { $_.reason -eq 'build-script-executed' -and $_.linked_libs -contains 'static=tesseract' })

if ($nativeBuilds.Count -ne 1) {
    throw "Expected one bundled Tesseract build for the CLI, found $($nativeBuilds.Count)"
}

$wanted = if ($Expect -eq 'static') { 'LIBCMT' } else { 'MSVCRT' }
$forbidden = if ($Expect -eq 'static') {
    @('MSVCRT', 'MSVCRTD', 'LIBCMTD')
} else {
    @('LIBCMT', 'LIBCMTD', 'MSVCRTD')
}

foreach ($name in 'leptonica', 'tesseract') {
    $lib = Join-Path $nativeBuilds[0].out_dir "$name\lib\$name.lib"
    if (-not (Test-Path $lib)) {
        throw "$lib does not exist"
    }

    $output = dumpbin /nologo /directives $lib
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin failed for $lib"
    }

    # One "/DEFAULTLIB:<name>" line per object file; collect the distinct names.
    $defaultLibs = @($output |
        Select-String -Pattern '/DEFAULTLIB:"?([A-Za-z0-9_.]+)"?' -AllMatches |
        ForEach-Object { $_.Matches } |
        ForEach-Object { $_.Groups[1].Value.ToUpper() -replace '\.LIB$', '' } |
        Sort-Object -Unique)

    Write-Host "$name.lib default libraries: $($defaultLibs -join ', ')"

    if ($defaultLibs -notcontains $wanted) {
        throw "$name.lib was not built against the $Expect runtime ($wanted missing)"
    }
    foreach ($bad in $forbidden) {
        if ($defaultLibs -contains $bad) {
            throw "$name.lib references $bad, expected the $Expect release runtime only"
        }
    }
}

Write-Host "Both libraries use the $Expect C runtime."

if ($Exe) {
    if (-not (Test-Path $Exe)) {
        throw "$Exe does not exist"
    }

    $output = dumpbin /nologo /dependents $Exe
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin failed for $Exe"
    }

    $dlls = @($output |
        Select-String -Pattern '^\s+(\S+\.dll)\s*$' |
        ForEach-Object { $_.Matches[0].Groups[1].Value.ToLower() } |
        Sort-Object -Unique)

    Write-Host "$Exe imports: $($dlls -join ', ')"

    $runtimeDlls = @($dlls | Where-Object { $_ -match '^(vcruntime|msvcp|ucrtbase|api-ms-win-crt-)' })
    if ($Expect -eq 'static' -and $runtimeDlls.Count -gt 0) {
        throw "$Exe imports the dynamic C runtime: $($runtimeDlls -join ', ')"
    }
}
