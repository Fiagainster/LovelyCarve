# 文档导航

文档围绕当前实现、开发约定和可复现验证维护。构建命令以根目录 `justfile` 为准，命令注册以 `src-tauri/src/lib.rs` 为准。

| 任务 | 文档 |
| --- | --- |
| 配置环境、开发、构建与打包 | [开发指南](DEVELOPMENT.md) |
| 提交修改、选择验证范围 | [贡献指南](../CONTRIBUTING.md)、[仓库约定](../AGENTS.md) |
| 理解后端、证据会话与任务生命周期 | [架构说明](ARCHITECTURE.md) |
| 修改界面与前端状态 | [前端规范](FRONTEND.md)、[前端模块与生命周期](FRONTEND_STRUCTURE.md) |
| 增加命令或事件 | [IPC 对照](IPC.md)、[架构扩展流程](ARCHITECTURE.md#扩展流程) |
| 开发插件 | [LPP 协议与 SDK](PLUGIN_PROTOCOL.md)、[Linux 基本信息插件](LINUXINFO.md) |
| 修改读取器、核验字节与来源 | [文件系统维护](FILESYSTEM_NOTES.md)、[ext2/3/4](EXT.md)、[FAT/exFAT](FAT.md)、[NTFS](NTFS.md)、[ReFS](REFS.md)、[vendor 来源](../src-tauri/vendor/UPSTREAM.md) |
| 验证性能与读取行为 | [性能验证](PERFORMANCE.md)、[后端探针](../src-tauri/examples/README.md) |
| 发布版本 | [发布指南](RELEASING.md)、[第三方声明](../THIRD_PARTY_NOTICES.md) |
| 报告安全问题 | [安全报告](../SECURITY.md) |

使用与接口说明保留 [使用指南](USAGE.md)、[CLI 手册](CLI.md)、[镜像信息](IMAGE_INFO.md)、[MBR/EBR](MBR.md)、[BCP](BCP.md)、[LUKS](LUKS.md) 和 [iOS 备份](IOS_BACKUPS.md)，帮助开发者复现对应功能。

功能概览统一维护在根目录 [README](../README.md)。新增能力时更新所属专题；测试样本的来源、许可和哈希保存在样本目录，第三方许可证与来源说明随代码保留。
