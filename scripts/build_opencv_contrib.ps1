# Build a contrib-enabled OpenCV (BUILD_opencv_world incl. the wechat_qrcode
# module) from source, so lovelycarve's `qrcode-wechat` feature can link the
# WeChat QR detector. The official OpenCV Windows prebuilt is core-only and does
# NOT ship wechat_qrcode, hence this one-time from-source build (~15-40 min).
#
# Inputs (all overridable from the justfile / env):
#   -VsPath   Visual Studio install root (provides MSVC + bundled cmake/ninja).
#   -Src      opencv\sources (the matching opencv source tree, e.g. 4.13.0).
#   -Contrib  opencv_contrib\modules (must match the opencv source version).
#   -Install  output install prefix; point OPENCV_DIR at this afterwards.
#
# Focused BUILD_LIST keeps the build small: only the modules opencv-rust needs
# for the qrcode-wechat feature (core/imgproc/imgcodecs/objdetect/dnn) plus
# wechat_qrcode and its transitive deps (calib3d/features2d/flann), and highgui
# (the `world` module's cmake unconditionally calls a highgui macro).
param(
    [string]$VsPath  = $env:VS_PATH,
    [string]$Src     = (Join-Path $PSScriptRoot '..\opencv\sources'),
    [string]$Contrib = (Join-Path $PSScriptRoot '..\opencv_contrib-4.x\opencv_contrib-4.x\modules'),
    [string]$Install = (Join-Path $PSScriptRoot '..\opencv-contrib-install')
)

$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'windows-env.ps1') -VisualStudio -VsPath $VsPath

$cmake = Join-Path $VsPath 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe'
$ninja = Join-Path $VsPath 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe'
$build = Join-Path (Split-Path -Parent $Install) 'opencv-contrib-build'

# Pass cmake args as a string array so PowerShell never re-parses commas/dots
# inside a single -D value (BUILD_LIST, version numbers).
$cmakeArgs = @(
    '-S', $Src,
    '-B', $build,
    '-G', 'Ninja',
    "-DCMAKE_MAKE_PROGRAM=$ninja",
    '-DCMAKE_BUILD_TYPE=Release',
    "-DCMAKE_INSTALL_PREFIX=$Install",
    '-DCMAKE_POLICY_VERSION_MINIMUM=3.5',
    "-DOPENCV_EXTRA_MODULES_PATH=$Contrib",
    '-DBUILD_LIST=core,imgproc,imgcodecs,highgui,calib3d,objdetect,dnn,wechat_qrcode',
    '-DBUILD_opencv_world=ON',
    '-DBUILD_opencv_apps=OFF', '-DBUILD_TESTS=OFF', '-DBUILD_PERF_TESTS=OFF',
    '-DBUILD_EXAMPLES=OFF', '-DBUILD_DOCS=OFF', '-DBUILD_opencv_python3=OFF', '-DBUILD_PACKAGE=OFF',
    '-DWITH_CUDA=OFF', '-DWITH_IPP=OFF', '-DWITH_FFMPEG=OFF', '-DWITH_GSTREAMER=OFF',
    '-DWITH_OPENCL=OFF', '-DWITH_EIGEN=OFF',
    '-DWITH_PROTOBUF=ON', '-DBUILD_PROTOBUF=ON',
    '-DOPENCV_ENABLE_NONFREE=OFF'
)

Write-Host ">> configuring OpenCV ($Src) + contrib ($Contrib)"
& $cmake @cmakeArgs
if ($LASTEXITCODE -ne 0) { throw "cmake configure failed ($LASTEXITCODE)" }

Write-Host ">> building (this is the long part)"
& $cmake --build $build --parallel
if ($LASTEXITCODE -ne 0) { throw "cmake build failed ($LASTEXITCODE)" }

Write-Host ">> installing to $Install"
& $cmake --install $build
if ($LASTEXITCODE -ne 0) { throw "cmake install failed ($LASTEXITCODE)" }

Write-Host "DONE: contrib OpenCV installed at $Install"
Write-Host "Note the vc<NN> folder under $Install\x64 and set OPENCV_MSVC / OPENCV_LIB in the justfile to match (e.g. vc18 / opencv_world4130)."
