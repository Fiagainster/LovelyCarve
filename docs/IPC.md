# 功能与 IPC 对齐

本文按当前工作流列出前端入口与后端命令域。完整注册表以 `src-tauri/src/lib.rs` 为准，组件只调用 `src/lib/ipc/` 的类型封装。

## 当前入口

| 功能 | 前端入口 | 主要命令 / 事件 |
| --- | --- | --- |
| 案件与证据 | 主壳、案件管理、文件会话 | `case_*`、`image_load`、`image_unload`、`physical_disks_list`、`image-load-progress` |
| 镜像信息 | 分区、校验、识别与 RAID | `image_info`、`image_bad_regions`、`os_detect`、`autodetect_evidence`、`raid_*` |
| 文件浏览与预览 | 文件列表、详情、独立预览窗 | `fs_children*`、`fs_flat_page`、`fs_read*`、`fs_preview*`、原类型预览命令 |
| 索引与快照 | 卷快照、全显和文件查询 | `snapshot_*`、`filedb_*`、`fs_child_counts` |
| 校验与导出 | 哈希、导出任务 | `image_hash_compute`、`image_export_raw`、`fs_digest`、`fs_export*` |
| 数据浏览 | 数据库、日志、命名流和 BCP | `fs_sqlite*`、`fs_realm*`、`fs_*journal*`、`fs_stream*`、`bcp_*` |
| 全局搜索 | `SearchResultsPanel.vue` | `global_search_*` |
| 时间线 | `TimelinePanel.vue` | `timeline_*`、`fs_timeline_export` |
| 文件雕刻 | `CarvePanel.vue` | `carve_*`、`carve-scan-progress`、`task-progress` |
| 加密与历史卷 | 解锁控件、卷影副本 | `bitlocker_*`、`encrypted_volume_unlock`、`shadow_copy_*` |
| iOS 本地备份 | 镜像信息页的解锁与异常检查 | `ios_backup_unlock`、`ios_backup_audit`，操作与边界见 [iOS 备份](IOS_BACKUPS.md) |
| 挂载 | 卷挂载与共享控件 | `fs_mount_*`、`fs_unmount_readonly`、`fs_share_mount`、挂载点命令 |
| QR/OCR | 文件详情 | `qr_scan`、`ocr_scan`，可用性由构建特性决定 |
| 插件与任务 | 设置、任务中心、结果面板 | `plugin_*`、`sched_*`、`analysis_*`、`sched-task-event` |
| 用户资料 | 设置 | 纯前端本机状态 |

## 不属于当前构建的入口

旧 AI Agent、Python IDE、软件更新、授权管理、工具下载、PCAP 主页面、右键加解密、IOC 与独立综合报告入口已移除。不得仅恢复菜单或包装函数而留下未注册命令。全局搜索、时间线和雕刻已有当前 Vue / Rust 实现，不适用旧路由的移除状态。

## 维护要求

新增功能同时更新核心逻辑、命令、注册表、类型封装与调用方。事件应具有明确 payload、归属、清理和错误处理；插件能力与 QR/OCR 等可选能力需要显示准确可用状态。

运行 `just ipc-check`、`just typecheck`、`just test-fe`、`just build-fe` 和相关 Rust 验证。检查切换镜像、面板卸载与迟到响应，避免旧任务结果写入新上下文。完整矩阵见 [贡献指南](../CONTRIBUTING.md)。
