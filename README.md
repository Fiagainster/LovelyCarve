# LovelyCarve

LovelyCarve 是基于 Rust、Tauri v2 和 Vue 3 的桌面磁盘取证工具，提供案件管理、只读证据浏览、原类型预览、导出、哈希、搜索、时间线与文件雕刻。界面以中文为主，支持浅色、深色和樱花色主题。

项目持续开发中，桌面开发与发布配方面向 Windows x64。源证据采用只读访问，案件、索引、缓存和导出产物写入主机目录。

## 功能

| 领域 | 能力 |
| --- | --- |
| 案件与证据 | 多证据案件、标签、卷快照、索引与分析结果持久化 |
| 镜像与逻辑证据 | RAW、E01/EWF、VMDK、VHD/VHDX、QCOW2、VDI、DMG、AD1；文件夹、归档、Docker、iOS 本地备份与物理磁盘 |
| 分区与卷 | GPT、MBR/EBR、APM、BSD，LVM2 线性/碎片卷读取、mdadm RAID 检测、RAID0/RAID5 重组 |
| 文件系统 | NTFS、Ext4、FAT16/32、exFAT、APFS、HFS+/HFSX、Btrfs、ZFS、XFS、UFS/UFS2、SquashFS、ReFS 等 Rust 读取器 |
| 加密与历史视图 | BitLocker、LUKS、VeraCrypt、iOS 本地备份解锁与 NTFS 卷影副本 |
| 浏览、预览与导出 | 目录树、分页、筛选、删除项、命名流；文本、Hex、媒体、文档、数据库与可执行文件预览；文件、目录与镜像导出 |
| 分析 | 多编码关键词与正则搜索、文件与插件时间线、NTFS USN / Ext4 日志、结构化雕刻 |
| 插件 | LPP 动态库接口，内置 `linuxinfo` Linux 基本信息分析插件 |
| 命令行与挂载 | 独立 `lovelycarve-cli`、交互浏览与 JSONL 自动化；Windows 桌面只读挂载 |

支持范围取决于格式变体、文件系统特性和镜像完整性；删除项不保证内容可恢复，雕刻受块对齐与碎片化限制。独立孤儿扫描和丢失分区扫描尚未接入，时间线文件投影目前采用修改时间。早期 `.carve` 案件不兼容，需新建案件。

## 使用

在 [GitHub Releases](https://github.com/LovelyTeams/LovelyCarve/releases) 下载已发布版本，或按下文从源码构建。启动后创建或打开案件，添加证据，再浏览、预览、校验或分析。

操作说明见 [使用指南](docs/USAGE.md)，自动化接口见 [CLI 手册](docs/CLI.md)，Linux 工件来源见 [linuxinfo](docs/LINUXINFO.md)。Windows 只读挂载需要 Dokany 驱动。

## 从源码构建

默认构建关闭二维码识别和 OCR，不需要 OpenCV 或 CUDA。准备 Rust stable（最低 **1.99**）、Bun（版本见 `package.json`）、just、PowerShell 7、Visual Studio C++ x64 工具、Windows SDK、LLVM 工具组件、OpenSSL 开发文件和 WebView2。

```text
git clone --recurse-submodules https://github.com/LovelyTeams/LovelyCarve.git
cd LovelyCarve
just bootstrap
just --list
just dev
```

Windows 配方通过 `vswhere` 查找 Visual Studio；需要指定安装位置时设置 `VS_PATH`。OpenSSL 默认在 `C:\Program Files\OpenSSL-Win64`，其他位置通过 `OPENSSL_DIR` / `OPENSSL_LIB` 覆盖。环境变量、可选 QR/OCR 与跨平台 CLI 配置见 [开发指南](docs/DEVELOPMENT.md)。

| 目的 | 命令 |
| --- | --- |
| 前端开发 / 生产构建 | `just dev-fe` / `just build-fe` |
| Windows 桌面开发 | `just dev` |
| 桌面安装包 / 便携包 | `just build-release` / `just pack` |
| CLI 开发 / 发布 / Windows ZIP | `just run-cli` / `just build-cli` / `just pack-cli` |
| linuxinfo 与隔离宿主 | `just build-plugins` / `just pack-plugins` |
| 含 QR/OCR 的发行版 | `just build-release-full` / `just pack-full` / `just pack-cli-full` |

仅前端运行无法读取证据。安装包位于 `src-tauri/target/release/bundle/`，便携包和插件包位于 `dist-pack/`。仓库使用本地构建与验证，发布步骤见 [发布指南](docs/RELEASING.md)。

## 开发

```text
just verify-fe
just verify-rust
just verify-plugins
```

也可按 [贡献指南](CONTRIBUTING.md) 的改动范围选择单项验证配方。

```text
src/app/                主壳、工作区与跨功能装配
src/features/           业务组件、状态、纯模型与测试
src/components/         共享 UI、反馈、布局与显示组件
src/lib/ipc/            类型化命令与事件边界
src-tauri/src/app/       启停与跨域工作流
src-tauri/src/commands/  Tauri 命令
src-tauri/src/core/      取证核心引擎
src-tauri/vendor/        维护中的解析器、读取器和来源声明
plugins/                LovelyCarve-Plugins 子模块：SDK、隔离宿主与 linuxinfo
scripts/                构建、资源准备与验证脚本
docs/                   开发、架构、协议和使用说明
```

开发前阅读 [架构说明](docs/ARCHITECTURE.md)、[前端规范](docs/FRONTEND.md) 和 [仓库约定](AGENTS.md)。专题入口见 [文档导航](docs/README.md)。

插件由独立仓库 [LovelyCarve-Plugins](https://github.com/LovelyTeams/LovelyCarve-Plugins) 维护，当前内置 `linuxinfo 1.0.0`。已有克隆运行 `git submodule update --init --recursive` 或 `just bootstrap` 获取固定版本。GitHub 自动生成的源码压缩包不包含子模块，开发请使用递归克隆。

## 反馈与许可

通过 [Issues](https://github.com/LovelyTeams/LovelyCarve/issues) 报告问题，请附版本、复现步骤和可公开的合成样本，不上传真实检材、案件或凭据。贡献流程见 [CONTRIBUTING.md](CONTRIBUTING.md)，安全问题见 [SECURITY.md](SECURITY.md)。

项目代码采用 [AGPL-3.0-only](LICENSE)。Copyright (C) 2026 LovelyCarve contributors.
第三方组件、衍生代码、模型和测试数据的来源及许可见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 与各 vendor 目录；发布时保留对应声明。
