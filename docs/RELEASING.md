# 发布指南

本文面向维护者，覆盖源码公开和版本发布。日常开发环境与构建参数见 [DEVELOPMENT.md](DEVELOPMENT.md)，项目采用的许可证见根目录 [LICENSE](../LICENSE)。

## 公开源码

- 保留 README、LICENSE、第三方声明、贡献指南、安全报告、CHANGELOG，以及必要的使用说明、开发、架构、格式来源和测试文档。文档导航见 [README.md](README.md)。
- 核对将公开的分支、标签和历史，移除不应公开的检材、案件、凭据、本机配置和个人数据。`.gitignore` 只影响未跟踪文件，不会清除已有 Git 历史。
- 保留 vendor 的许可证、ORIGIN / UPSTREAM / LOVELYCARVE 说明及合成测试数据来源；[ReFS 验证文档](REFS.md)包含公开镜像的来源和哈希，应继续维护。
- 本机的工具路径使用环境变量覆盖；`fixtures/` 中的外部镜像、构建输出和本机 agent 权限不作为项目源文件提交。
- 在仓库设置中开启私密漏洞报告，使 [SECURITY.md](../SECURITY.md) 的报告入口可用。操作见 [GitHub 配置说明](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository)。

仓库可见性和 GitHub 功能开关由维护者在平台上设置，提交本文件不会自动改变它们。

## 本地验证

在干净的递归克隆中执行 `just bootstrap`，然后按 [贡献指南](../CONTRIBUTING.md#验证) 完成前端、Rust、IPC 和结构检查。修改插件或读取器时执行其专用配方。涉及 example 的公共接口需要 `just check-examples`。

发布前确保 `plugins/` 的固定提交已推送到公开 [LovelyCarve-Plugins](https://github.com/LovelyTeams/LovelyCarve-Plugins)。GitHub 自动生成的主仓库源码 ZIP / tar.gz 不包含子模块，在发行说明中提供 `git clone --recurse-submodules` 和初始化步骤。

发布完整功能版本还需构建 QR/OCR，并在目标 Windows 环境检查模型加载、CPU 回退、离线文件预览、插件宿主和 DLL 加载。安装包和便携包分别做启动、建案、添加合成证据、浏览、哈希、导出及关闭验证；挂载功能需要 Dokany 驱动和对应的实际挂载测试。macOS / Linux 只有完成该平台验证后才提供相应产物。

## 版本与构建

1. 同步 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 中的应用版本。依赖锁文件一并提交，插件版本按其独立兼容要求维护。
2. 按 [Keep a Changelog 0.3.0](https://keepachangelog.com/zh-CN/0.3.0/) 将 CHANGELOG 中实际纳入发布标签的 `[Unreleased]` 条目整理为 `## [版本号] - YYYY-MM-DD`，记录格式或案件兼容变化；尚未发布的内容保留在 `[Unreleased]`，没有剩余条目时保留空的 `[Unreleased]`。
3. 根据功能选择本地构建与打包入口：

| 产物 | 默认关闭 QR/OCR | 含 QR/OCR |
| --- | --- | --- |
| Windows 桌面安装包 | `just build-release` | `just build-release-full` |
| Windows 桌面便携包 | `just pack` | `just pack-full` |
| Windows CLI 包 | `just pack-cli` | `just pack-cli-full` |

`repack` / `repack-full` 复用已有桌面产物，必须与编译特性一致；CLI 的 `repack-cli` 按实际能力选择资源。运行库、模型与插件清单由现有打包配置决定，不凭文件后缀猜测所需资源。

CHANGELOG 使用 `Added`、`Changed`、`Deprecated`、`Removed`、`Fixed`、`Security` 中实际有内容的英文分类，版本按时间倒序排列，不保留空分类，也不直接粘贴 Git 提交记录。版本号采用程序的完整语义化版本，发布日期以实际发布记录为准；版本链接指向对应比较页，首次发布可指向 Release 页。更新 `[Unreleased]` 的比较起点为最新发布标签。例如程序 `1.0.0` 对应既有标签 `v1.0`，链接沿用该实际标签。

## 第三方声明与源码

发布前按实际产物核对 [第三方声明](../THIRD_PARTY_NOTICES.md)、vendor 许可证、文件预览器资源、字体、Dokan、OpenSSL、OpenCV、ONNX Runtime 以及 QR/OCR 模型的来源、版本和许可文本。依赖锁文件记录包版本，二进制与模型还需核对其实际来源；根目录声明不是全部传递依赖的自动许可证清单。

现有打包入口随附 LICENSE 和 THIRD_PARTY_NOTICES。发行产物仍需检查内容完整，保留组件自带的声明与模型许可证，并让下载者能获得与二进制对应的项目源码、vendor 改动、锁文件、构建脚本与必要的第三方来源说明。Quicksand 的 OFL 全文同时保存在字体目录和第三方声明中。

## 标签与发行产物

完成本地验证与打包后，将 `v<版本>` 标签指向对应源码提交。仓库没有 GitHub Actions；推送标签不会触发云端构建，发行产物来自上表的本地配方。

从该标签创建 GitHub Release，附版本变化、功能组合、平台要求、已知限制和实际构建产物。上传前计算 SHA-256：

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath 'dist-pack/LovelyCarve-v<版本>-win-x64.7z'
```

源码标签、发行说明和产物应对应同一提交。说明签名状态、验证的平台范围和外部驱动要求；保留第三方来源、许可证、锁文件和必要的构建脚本。
