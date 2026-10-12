# Changelog

本文件记录 LovelyCarve 的重要变更。

格式遵循 [Keep a Changelog 0.3.0](https://keepachangelog.com/zh-CN/0.3.0/)，
版本号遵循[语义化版本](https://semver.org/lang/zh-CN/)。条目以用户可感知的变化为主，按版本倒序排列。

## [Unreleased]

### Added

- 补齐 ext2/3/4 内联文件与目录、EA-inode 扩展属性、旧式目录和 meta_bg 读取，并记录完整 libfsext 能力对照。
- ext 读者提供卷挂载历史、feature masks、UID/GID、链接数和设备编号等元数据 API。

### Changed

- 插件 SDK、隔离宿主与 `linuxinfo` 改由独立的 [LovelyCarve-Plugins](https://github.com/LovelyTeams/LovelyCarve-Plugins) 子模块维护；源码构建需递归克隆或运行 `just bootstrap` 初始化子模块。

### Fixed

- ext 文件范围与流式读取统一完整间接块映射，未写入 extent 读零，越过卷边界或来源短读时报错。
- 修复 ext signed/扩展 epoch/纳秒时间戳及 birth 投影、32768-block extent 长度、64 KiB 目录记录和旧组描述符 CRC16 校验。

## [1.0.0] - 2026-10-12

首次公开源码发布，对应 Git 标签 `v1.0`，采用 AGPL-3.0-only 许可证。桌面开发与发布配方面向 Windows x64。
当前案件格式不兼容早期开发版的 `.carve` 案件，需要新建案件。

### Added

- 中文桌面工作区，提供浅色、深色、樱花色主题及可配置的界面动画。
- 案件创建、打开与最近案件记录，支持多检材登记、文件标签和操作审计。
- 卷目录快照、文件索引及分析结果持久化；原镜像缺失时，可通过已验证的索引浏览目录和元数据。
- RAW、E01/EWF、VMDK、VHD/VHDX、QCOW2、VDI、DMG、AD1 镜像及 Windows 物理磁盘的只读访问。
- 文件夹、归档与 iOS 本地备份检材，支持 Docker/OCI 镜像及磁盘中 Docker 容器的合并文件视图。
- GPT、MBR/EBR、APM、BSD 分区识别，LVM 逻辑卷浏览，以及 mdadm 成员检测与 RAID0/RAID5 重组。
- NTFS、Ext4、FAT16/32、exFAT、APFS、HFS+/HFSX、Btrfs、ZFS、XFS、UFS/UFS2、SquashFS、ReFS 文件系统读取。
- NTFS 命名流、LZNT1/WOF 压缩文件读取，以及 ReFS 文件快照流浏览。
- BitLocker、LUKS、VeraCrypt 和加密 iOS 本地备份解锁，建立按需读取的只读解密视图。
- NTFS 卷影副本枚举与只读历史会话，可浏览快照时刻的文件内容。
- 目录树、分页文件列表、排序与筛选，按文件系统能力展示删除项、未分配空间、符号链接及元数据节点。
- 文本、Hex、图片、音视频、PDF、办公文档及归档等原类型预览；文件预览可拆出为独立窗口并重新停靠。
- SQLite、Realm 数据库与 PE、ELF、Mach-O、GGUF 等结构化预览，以及 SQLite 删除记录恢复。
- 镜像与文件的 MD5、SHA-1、SHA-256、SM3 校验，文件、目录、ZIP、RAW 镜像及解密镜像导出。
- 多编码关键词与正则全局搜索，支持范围筛选、任务历史、分页命中、源文件定位及 CSV 导出。
- 文件修改时间、插件事件与 NTFS USN 日志的时间线投影，提供热力图、趋势图、筛选和导出；可查看 NTFS USN 与 Ext4 日志。
- 图片、音频、归档、PDF、SQLite 等格式的结构化文件雕刻，支持暂停、继续、取消、结果恢复及 SHA-256/DFXML 输出。
- LPP 动态库插件接口与独立进程宿主，内置 `linuxinfo`，分析 Linux 系统、用户、登录、网络、服务、软件与定时任务等信息。
- 任务中心统一展示分析与后台任务的进度、控制操作和错误，分析结果支持分页、详情、源文件定位与 CSV 导出。
- BCP 索引与字段说明、按数据集分类的分页浏览、当前页筛选及来源文件查看。
- 独立 `lovelycarve-cli`，支持交互式证据浏览、哈希、导出、分析命令及 JSONL 自动化会话。
- Windows Dokany 只读挂载，可将支持的卷与 Docker 合并视图挂载为盘符或文件夹。
- 可选二维码识别与 OCR 构建；默认轻量构建关闭这些功能，完整构建通过 `*-full` 配方启用。
- 源码构建、Windows 安装包与便携包、CLI 和插件的本地打包配方，以及使用、开发、接口和第三方来源说明。

[Unreleased]: https://github.com/LovelyTeams/LovelyCarve/compare/v1.0...HEAD
[1.0.0]: https://github.com/LovelyTeams/LovelyCarve/releases/tag/v1.0
