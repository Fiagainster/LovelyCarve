# 开发指南

统一在仓库根目录通过 `just` 执行任务。首次运行 `just bootstrap`，初始化插件子模块并执行 `bun install --frozen-lockfile`，再用 `just --list` 查看当前入口。建议使用 just 1.53 或更新版本。

## 环境

| 组件 | 要求 |
| --- | --- |
| Rust | stable，最低版本以 `src-tauri/Cargo.toml` 的 `rust-version` 为准，当前为 1.99 |
| Bun | 版本以 `package.json` 的 `packageManager` 为准 |
| Windows 桌面 | PowerShell 7、Visual Studio C++ x64 工具、Windows SDK、OpenSSL 开发文件、WebView2 |
| macOS | Xcode Command Line Tools；桌面检查另需相应 Tauri 系统依赖 |
| Linux | 编译工具链、WebKitGTK、OpenSSL 等 Tauri 系统依赖 |
| CLI smoke 测试 | Python 3 |
| Windows 桌面便携打包 | PATH 中的 `upx` 与 `7z` |

默认构建关闭 QR/OCR，无需 OpenCV 或 CUDA。桌面开发和打包配方仅支持 Windows；Rust 检查、前端任务、插件构建和 CLI 构建可在相应平台运行，macOS / Linux 产物需单独验证。

前端离线预览资源由准备脚本从依赖复制到 `public/file-viewer/`，不提交 Git。`just dev-fe` / `just dev` 先准备资源；仅前端开发无法读取证据。Vite 固定监听 `http://127.0.0.1:14200`，更换端口时同时修改 `vite.config.ts` 与 `src-tauri/tauri.conf.json`，保留 `strictPort`。

## Windows 配置

配方在当前命令的 PowerShell 中加载 `scripts/windows-env.ps1`。Visual Studio 默认通过 `vswhere` 查找已安装 C++ x64 工具的最新实例；通过环境变量或 `just name=value <任务>` 指定其他位置。

```powershell
$env:VS_PATH = 'C:\Program Files\Microsoft Visual Studio\2022\BuildTools'
$env:OPENSSL_DIR = 'C:\Program Files\OpenSSL-Win64'
$env:OPENSSL_LIB = "$env:OPENSSL_DIR\lib\VC\x64\MT"
just dev
```

| just 变量 | 环境变量 | 默认行为 |
| --- | --- | --- |
| `vs_path` | `VS_PATH` | 空值，由 `vswhere` 查找 |
| `openssl_dir` | `OPENSSL_DIR` | `C:\Program Files\OpenSSL-Win64` |
| `openssl_lib` | `OPENSSL_LIB` | OpenSSL 目录下 `lib\VC\x64\MT` |
| `perl_bin` | `PERL_BIN` | 空值，不修改 Perl 搜索路径 |
| `opencv_dir` | `OPENCV_DIR` | 仓库下 `opencv-contrib-install` |
| `opencv_lib` | `OPENCV_LIB` | `opencv_world4130` |
| `opencv_msvc` | `OPENCV_MSVC` | `vc18`，按实际安装修改 |
| `opencv_src` | `OPENCV_SRC` | 仓库下 `opencv\sources` |
| `opencv_contrib` | `OPENCV_CONTRIB` | 仓库下 `opencv_contrib-4.x\opencv_contrib-4.x\modules` |
| `opencv_install` | `OPENCV_INSTALL` | 与 `opencv_dir` 相同 |

轻量 Rust 检查只配置 OpenSSL。桌面及发布 CLI 构建初始化 VS x64 环境，使用 MSVC 默认链接器。只有 `*-full` 入口配置 OpenCV / libclang，需另装 Visual Studio LLVM 工具组件以提供 libclang。Cargo 并发由工具链默认值决定，可通过 `CARGO_BUILD_JOBS` 限制，内存不足时设为 1 或 2。

SQLCipher 配方设置 `OPENSSL_NO_VENDOR=1` 和 `OPENSSL_STATIC=1`，使用本机 OpenSSL 开发文件。若改为源码编译 OpenSSL，准备 Windows Perl 与 NASM。静态链接标记不保证没有 DLL 依赖：导入库仍可能需要运行库，调整 `OPENSSL_LIBS` 或分发 DLL 前检查实际二进制依赖。

## 日常任务

```text
just                         # 按工作流顺序显示任务
just --list --group frontend  # 查看一个分组
just --show build-cli         # 查看本平台配方
just --dry-run pack           # 展开依赖与命令，不执行构建
```

| 工作 | 入口 |
| --- | --- |
| 前端 | `dev-fe`、`typecheck`、`test-fe`、`build-fe`、`ipc-check`、`frontend-structure-check` |
| 后端 | `fmt`、`check`、`clippy`、`test [名称及 Cargo 参数]`、`check-examples` |
| Windows 桌面 | `dev`、`build`、`build-release`、`pack`、`repack`；QR/OCR 使用对应 `*-full` |
| CLI | `run-cli`、`build-cli`、`build-cli-full`、`test-cli`、`smoke-cli <exe>`；Windows `pack-cli`、`pack-cli-full`、`repack-cli` |
| 插件 | `build-plugins [--release]`、`test-plugins`、`clippy-plugins`；Windows `pack-plugins` |
| 读取器 | `test-vendor <目录>`、`test-ewf`、`test-vmdk`、`test-vhd`、`test-vhdx`、`test-qcow2`、`test-vdi`、`test-bitlocker` / `clippy-bitlocker`、`test-luks` / `clippy-luks`、`test-veracrypt`、`test-vsc`、`test-refs`、`test-ad1`、`test-sqlite-core`、`test-ios-backup` |
| 探针与性能 | `run-example`、`run-example-fast`、`bench-filedb-store`、`probe-refs` |
| 依赖更新 | `update-fe`、`update-rust`、`update-plugins`、`update-tauri`、`update-file-viewer <版本>` |
| 资源维护 | `icons`、`qr-env`、`build-opencv-contrib`、`qr-test <图片>` |

`verify-fe` 汇总 IPC、结构、类型检查、前端测试和生产构建；`verify-rust` 汇总格式化、编译、Clippy 和后端测试；`verify-plugins` 汇总格式化、插件 Clippy 和插件测试。按 [贡献指南](../CONTRIBUTING.md#验证) 选择范围。

`fmt` 覆盖主 crate、插件与维护中的读取器。公共读取接口改动后运行 `check-examples`。参数含空格时保留一层传给子 shell 的引号：

```powershell
just run-example probe_partitions '"D:\Evidence Files\disk.E01"'
```

VSS 独立 reader 使用 `test-vsc` / `clippy-vsc`；`probe-vsc <NTFS卷RAW> --blocks --hash`
检查原始元数据并顺序计算每个快照的完整摘要，`compare-vsc <oracle目录> <数量>`
运行 pyvshadow 差分语料。样本和可选环境变量见 [VSS](VSS.md)。

桌面 `release` 体积优先，热点依赖保留独立优化；`release-fast` 用于性能比较。开发版 Argon2 / BLAKE2 也启用优化，以满足高内存 LUKS2 的派生预算。性能验证见 [PERFORMANCE.md](PERFORMANCE.md)。

## 插件与资源

`plugins/` 是 [LovelyCarve-Plugins](https://github.com/LovelyTeams/LovelyCarve-Plugins) 的 Git 子模块，只包含 `sdk`、`host` 与 `linuxinfo`。主仓库固定插件提交；递归克隆或运行 `just bootstrap` 初始化，已有克隆可运行 `git submodule update --init --recursive`。GitHub 标签源码压缩包不包含子模块，应使用 Git 克隆获取完整开发源码。

`build-plugins` 按提交的锁文件构建宿主和 linuxinfo，把动态库复制到清单旁。`dev` 使用开发插件；`build-release` 与 CLI 打包先构建发布插件。改动插件时先在插件仓库提交、推送，再在主仓库提交子模块引用；不要仅提交主仓库指针而漏推插件提交。

安装包和便携包读取同一份 `src-tauri/tauri.plugins.conf.json`：主程序旁放置 `lc-plugin-host.exe`，插件目录包含 `plugins/linuxinfo/plugin.toml` 和 DLL。插件不嵌入主程序。单独部署使用 `just pack-plugins`，更换文件前退出应用，重启后重新发现。接口与分析来源见 [PLUGIN_PROTOCOL.md](PLUGIN_PROTOCOL.md) 和 [LINUXINFO.md](LINUXINFO.md)。

ReFS 使用纯 Rust reader；旧 `librefs-13.dll` 仅供显式 `refs-dll-oracle` 对照，不随正式包分发。Dokan、OpenSSL 与可选 OpenCV DLL，以及模型和许可证是构建 / 运行资源，按打包配置保留。测试夹具及来源说明保存在 vendor / tests 中，不用真实检材替换。

图标保留透明和有背景两张原图，位于 `src-tauri/icons/source*.png`。修改后执行 `just icons`，同步平台图标及前端 512px 资源。

## 可选 QR/OCR 与发布

Windows `dev-full`、`build-full`、`build-release-full` 和 `build-cli-full` 显式开启 WeChat QR 与带 CPU 回退的 CUDA OCR。准备版本匹配的 OpenCV 和 contrib 源码，使用 `just build-opencv-contrib` 构建 `wechat_qrcode`；核对安装目录的 `vc*` 与 world 库名称，再设置对应变量。官方核心 OpenCV 包不满足该构建所需模块。

Unix `build-cli-full` 启用 `qrcode-wechat ocr`（CPU OCR），需自行配置系统依赖。Windows 发布 CLI 使用 `mt.exe` 嵌入清单，Cargo 构建失败时不执行该步骤。

`pack` / `pack-cli` 默认不带 QR/OCR 资源；完整包使用 `pack-full` / `pack-cli-full`，额外资源来自 `src-tauri/tauri.full.conf.json`。`repack` / `repack-full` 复用已有桌面产物，必须匹配编译特性；`repack-cli` 按 CLI 实际能力选择资源。便携包位于 `dist-pack/`，安装包位于 `src-tauri/target/release/bundle/`。

仓库不提供 GitHub Actions 工作流。验证与打包均在本地执行，源码标签和发行产物对应同一提交，步骤见 [发布指南](RELEASING.md)。

## 更新依赖

先审查清单约束，再用 `update-fe`、`update-rust` 或 `update-plugins` 更新对应锁文件。`update-tauri` 同步前后端 Tauri 包族；各包独立发布，不强求补丁版本号一致。vendor 的来源与补丁见 [UPSTREAM.md](../src-tauri/vendor/UPSTREAM.md)，用对应读取器配方验证。

Vue 类型检查通过 `typescript` 的 npm 别名接入 `typescript-native-bridge@6.0.3-bridge.18.tsgo.7.0.2`，兼容 `vue-tsc` 所需的经典编译器 API。别名与 override 固定为同一完整版本；不要直接替换为 `typescript@7`。`just typecheck` 会显示 `TNB ACTIVE`。普通组件测试默认关闭动效，动效专用测试单独验证开关与收尾。

前端生产构建使用 Vite 8 自带的 Oxc 压缩器，不依赖额外安装 esbuild。旧配置中的 `build.minify: 'esbuild'` 会在干净安装后触发 `Failed to load transformWithEsbuild`；使用更新后的 `vite.config.ts`，再运行 `just bootstrap` 和 `just build-fe`。非 Windows 的 Safari 13 构建目标保持不变，但地图渲染器依赖 BigInt，map/shp 预览需要 Safari 14 或更新的 WebKit。

升级后按改动范围执行前端、Rust、插件与读取器验证。不要将本机路径、依赖目录、案件、缓存、导出结果和签名凭据提交仓库；`.gitignore` 为回归样本保留例外。
