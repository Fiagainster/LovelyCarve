# Dot-source from just recipes so the environment stays in the command's shell.
[CmdletBinding()]
param(
    [string]$OpenSSLDir = 'C:\Program Files\OpenSSL-Win64',
    [string]$OpenSSLLib = '',
    [string]$PerlBin = '',
    [switch]$VisualStudio,
    [string]$VsPath = '',
    [switch]$OpenCV,
    [string]$OpenCVDir = '',
    [string]$OpenCVLib = 'opencv_world4130',
    [string]$OpenCVMsvc = 'vc18'
)
$ErrorActionPreference = 'Stop'

if ($VisualStudio) {
    if (-not $VsPath) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        if (-not (Test-Path -LiteralPath $vswhere)) {
            throw 'Cannot find Visual Studio Installer. Set VS_PATH to your C++ tools installation.'
        }
        $VsPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($LASTEXITCODE -ne 0 -or -not $VsPath) {
            throw 'Cannot find Visual Studio C++ x64 tools. Install them or set VS_PATH.'
        }
    }
    Import-Module (Join-Path $VsPath 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')
    Enter-VsDevShell -VsInstallPath $VsPath -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' | Out-Null
    $llvmBin = Join-Path $VsPath 'VC/Tools/Llvm/x64/bin'
    if (-not (Test-Path -LiteralPath (Join-Path $llvmBin 'lld-link.exe'))) {
        throw 'Install the Visual Studio LLVM tools component (lld-link).'
    }
    $env:PATH = $llvmBin + ';' + $env:PATH
    if ($OpenCV) { $env:LIBCLANG_PATH = $llvmBin }
}

if (-not $OpenSSLLib) { $OpenSSLLib = Join-Path $OpenSSLDir 'lib/VC/x64/MT' }
$env:OPENSSL_NO_VENDOR = '1'
$env:OPENSSL_STATIC = '1'
$env:OPENSSL_DIR = $OpenSSLDir
$env:OPENSSL_LIB_DIR = $OpenSSLLib
$env:OPENSSL_INCLUDE_DIR = Join-Path $OpenSSLDir 'include'
if ($PerlBin) { $env:PATH = $PerlBin + ';' + $env:PATH }

if ($OpenCV) {
    if (-not $OpenCVDir) { throw 'Set OPENCV_DIR to your contrib-enabled OpenCV installation.' }
    $env:OPENCV_LINK_LIBS = $OpenCVLib
    $env:OPENCV_LINK_PATHS = Join-Path $OpenCVDir "x64/$OpenCVMsvc/lib"
    $env:OPENCV_INCLUDE_PATHS = Join-Path $OpenCVDir 'include'
    $env:PATH = (Join-Path $OpenCVDir "x64/$OpenCVMsvc/bin") + ';' + $env:PATH
}
