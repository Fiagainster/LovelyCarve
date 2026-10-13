# XFS 与 libyal/libfsxfs 能力对照

核对日期：2026-10-13。基准为 [libfsxfs 固定提交](https://github.com/libyal/libfsxfs/tree/cf474697de8a32f7eba4122e469b2ecd818054c6)
`cf474697de8a32f7eba4122e469b2ecd818054c6`。核对 README、公开头文件、superblock、inode、directory、extent、data stream、extended attribute 实现及测试。
生产读取继续使用 BSD-2-Clause 的纯 Rust `xfs-fuse` 本地 fork；没有链接、复制或分发 libfsxfs 的 C 实现。

## 格式与应用能力

| 能力 | 修改前 | 当前 | 验证边界 |
| --- | --- | --- | --- |
| XFS 1–5 superblock | 仅 4/5；非法 superblock 会 panic | fallible open，校验 geometry、扇区/块/inode 大小、AG、CRC | 1–5 合成卷；没有真实 IRIX 镜像 oracle |
| v1/v2/v3 inode | 已有 v1 与 NREXT64 字段解析 | 按 fork 边界解码；V3 校验 CRC、inode 号、metadata UUID | inode 版本、64 位计数、非法 fork 与 CRC 回归 |
| 目录 v1 | 无 | inline、leaf、node；有界遍历、循环拒绝 | 版本 1–3 inline 和 leaf/node 合成卷 |
| 目录 v2/v3 | xfuse 哈希索引及数据解码 | 枚举已分配数据块；跳过叶/节点索引区；校验名称、tag、free record、block tail、V3 CRC/owner/address | shortform、data、block 格式；未以真实百万项目录测性能 |
| 文件正文 | extent/B-tree，但潜在 EOF/短读处理问题 | 统一校验 extent 计划；空洞与 unwritten 为零；allocated 短读/越界失败 | 稀疏、unwritten、跨叶空洞、EOF、截断与坏 extent |
| 随机读取 | 应用 dispatch 使用 stream 前缀回退 | `XfsVolume::read_file_range` → `Volume::read_range` → 缓存会话 | 完整、流式、直接与会话范围读取逐字节一致 |
| 来源范围 | `FsStat.data_runs` 为空 | written 正文映射回原镜像；分割跨 LVM segment 的 run；按逻辑 EOF 裁剪 | 最小镜像与碎片映射，unwritten 不冒充 authentic bytes |
| 符号链接 | 目录没有 target；外置读法有 offset 风险 | local / extent target；长度裁剪；V5 每块校验、逻辑 offset 连续；完整/流式/范围一致 | 本地及外置 V4/V5；上限 1024 字节 |
| 文件名/硬链接 | lossy UTF-8 同名会造成路径碰撞 | 原始 POSIX 字节进入 hex node ID；显示文字可替换无效字符；独立名称保留同一 inode | 两个显示相同而 raw bytes 不同的名称均能读取 |
| 属性 | 解析器内部存在，未开放 | shortform、leaf、node、bmap B-tree；V4 原始 remote / V5 XARM；二进制值、原始 flags、payload extents、范围读取 | 所有组合、哈希碰撞、同名跨 namespace、跨 remote block、损坏 CRC、node cycle |
| 属性命名空间 | secure.；按 hash 取值，忽略精确名字 | user. / trusted. / security.；按完整名称选择；INCOMPLETE 不视为有效属性 | 同名与同哈希回归；parent pointer 以 parent. + flags 保留内部记录，重复名字不选取任意一个 |
| 时间 | 负秒转成巨大无符号；旧 inode birth 显示 epoch | signed POSIX 秒及原始纳秒；BIGTIME；旧 inode birth/created 为 None | Windows SystemTime 的 100 ns 粒度不替代原始纳秒；UI 时间串使用 raw 数值 |
| POSIX 元数据 | FsStat 只有 permission 位，device 为 0 | 完整 mode；核心 API 保留 uid/gid/nlink/device/version/raw timestamps | `FsStat.mode` 已接入，其他字段通过 `inode_metadata` 获取，未新增专用 IPC/UI |
| 卷元数据 | 部分 geometry 可读 | label 原始 12 字节、UUID/meta UUID、版本及 feature words、统计 | `volume_metadata` 核心 API；未新增专用 IPC/UI |
| META_UUID | 明确不支持 | source UUID 与 metadata UUID 分开；所有新读取路径按 meta UUID 校验 | UUID 改写后的 V5 合成卷；libfsxfs README 将它列为不支持 |
| quota feature | 不单独解析 quota | 可读普通文件；没有 quota records API | libfsxfs superblock 拒绝 QUOTABIT；本地没有声称实现 quota 审计 |
| ASCII CI / MOREBITS | MOREBITS 常量错误为 0x4000 | MOREBITS 修正为 0x8000；V4 BORG(0x4000) ASCII 忽略大小写 lookup | UTF-8/non-ASCII 不做 locale case folding |
| real-time 文件 | 无独立设备，拒绝正文 | 继续明确拒绝，不从 data device 错读 | metadata 可浏览；未提供独立 real-time reader |
| NEEDSREPAIR / METADIR / ZONED / ZONE_GAPS / 未知 incompat | 部分按 rblocks 条件拒绝，或 panic | 打开时统一报错 | 不把不支持布局当成已支持 |
| reflink/rmap/finobt 等只读兼容位 | 使用普通 inode 数据映射 | 保留 feature words，按已提交 inode map 只读 | 没有共享块引用计数/反向映射审计或 log replay |
| 删除与孤儿恢复 | 无原生恢复投影 | 无新增删除项/孤儿扫描或日志恢复 | libfsxfs 公开 API 也没有完整 undelete 接口；空闲结构不宣称为恢复文件 |

公开的“格式支持”表示实现可读取相应布局，不代表对所有历史版本、mkfs 选项或损坏场景完成了真实检材验证。

## 公开 API 逐项映射

[公开头文件](https://github.com/libyal/libfsxfs/blob/cf474697de8a32f7eba4122e469b2ecd818054c6/include/libfsxfs.h.in)
中有 **82 个函数**（含 wide/BFIO 条件声明），下表全部列出。
“等价”指 Rust 中的能力，不要求模拟 C 的初始化/free、UTF-16 双接口、隐含游标或 NUL 终止缓冲区。
核心 API 与应用 IPC/UI 的公开程度单独说明，不能由底层能力推定已有界面。

| libfsxfs API | Rust 对应或差异 |
| --- | --- |
| `libfsxfs_get_version` | Cargo 包版本和来源记录；不提供 C ABI 版本函数 |
| `libfsxfs_get_access_flags_read` | 只读 `Read + Seek` 契约，没有写模式 |
| `libfsxfs_get_codepage` | 保留原始 POSIX 字节；没有全局 codepage 状态 |
| `libfsxfs_set_codepage` | 没有全局 codepage 转码；显示与路径字节分离 |
| `libfsxfs_check_volume_signature` | `is_xfs_magic` + fallible open；路径/镜像容器/切片由 DiskImage 处理 |
| `libfsxfs_check_volume_signature_wide` | `is_xfs_magic` + fallible open；路径/镜像容器/切片由 DiskImage 处理 |
| `libfsxfs_check_volume_signature_file_io_handle` | `is_xfs_magic` + fallible open；路径/镜像容器/切片由 DiskImage 处理 |
| `libfsxfs_notify_set_verbose` | 宿主 `tracing` 日志；没有独立 C FILE 流或全局 verbose 状态 |
| `libfsxfs_notify_set_stream` | 宿主 `tracing` 日志；没有独立 C FILE 流或全局 verbose 状态 |
| `libfsxfs_notify_stream_open` | 宿主 `tracing` 日志；没有独立 C FILE 流或全局 verbose 状态 |
| `libfsxfs_notify_stream_close` | 宿主 `tracing` 日志；没有独立 C FILE 流或全局 verbose 状态 |
| `libfsxfs_error_free` | Rust Result/errno → anyhow 上下文；错误所有权随 RAII 释放，未仿制 C backtrace 格式 |
| `libfsxfs_error_fprint` | Rust Result/errno → anyhow 上下文；错误所有权随 RAII 释放，未仿制 C backtrace 格式 |
| `libfsxfs_error_sprint` | Rust Result/errno → anyhow 上下文；错误所有权随 RAII 释放，未仿制 C backtrace 格式 |
| `libfsxfs_error_backtrace_fprint` | Rust Result/errno → anyhow 上下文；错误所有权随 RAII 释放，未仿制 C backtrace 格式 |
| `libfsxfs_error_backtrace_sprint` | Rust Result/errno → anyhow 上下文；错误所有权随 RAII 释放，未仿制 C backtrace 格式 |
| `libfsxfs_volume_initialize` | Rust 所有权/Drop；EvidenceSession 生命周期及 Mutex，无手动 free ABI |
| `libfsxfs_volume_free` | Rust 所有权/Drop；EvidenceSession 生命周期及 Mutex，无手动 free ABI |
| `libfsxfs_volume_signal_abort` | 应用任务取消在 streamed visitor/任务层检查；parser 没有独立 abort flag，单次 metadata 解码只由资源界限约束 |
| `libfsxfs_volume_open` | `Volume::open(Read + Seek) -> Result`；DiskImage/PartitionCursor 支持镜像与碎片 LVM |
| `libfsxfs_volume_open_wide` | `Volume::open(Read + Seek) -> Result`；DiskImage/PartitionCursor 支持镜像与碎片 LVM |
| `libfsxfs_volume_open_file_io_handle` | `Volume::open(Read + Seek) -> Result`；DiskImage/PartitionCursor 支持镜像与碎片 LVM |
| `libfsxfs_volume_close` | Rust 所有权/Drop；EvidenceSession 生命周期及 Mutex，无手动 free ABI |
| `libfsxfs_volume_get_format_version` | `Sb::version()` / `XfsVolume::volume_metadata()` |
| `libfsxfs_volume_get_features_flags` | `Sb::feature_flags()` / `incompat_flags()` 与公开 feature words |
| `libfsxfs_volume_get_utf8_label_size` | `Sb.label` 原始字节；UTF-8 展示/UTF-16 转码及长度由调用方完成，无 NUL 缓冲区 API |
| `libfsxfs_volume_get_utf8_label` | `Sb.label` 原始字节；UTF-8 展示/UTF-16 转码及长度由调用方完成，无 NUL 缓冲区 API |
| `libfsxfs_volume_get_utf16_label_size` | `Sb.label` 原始字节；UTF-8 展示/UTF-16 转码及长度由调用方完成，无 NUL 缓冲区 API |
| `libfsxfs_volume_get_utf16_label` | `Sb.label` 原始字节；UTF-8 展示/UTF-16 转码及长度由调用方完成，无 NUL 缓冲区 API |
| `libfsxfs_volume_get_root_directory` | `sb_rootino` + `readdir` / `root_children` |
| `libfsxfs_volume_get_file_entry_by_inode` | `getattr/read/readlink/extents/extended_attributes` 接受 inode；应用 node ID 仍以 raw path 定位 |
| `libfsxfs_volume_get_file_entry_by_utf8_path` | `lookup_path` / `lookup_path_bytes`；UTF-16 在调用边界转码 |
| `libfsxfs_volume_get_file_entry_by_utf16_path` | `lookup_path` / `lookup_path_bytes`；UTF-16 在调用边界转码 |
| `libfsxfs_file_entry_free` | Rust 所有权/Drop；EvidenceSession 生命周期及 Mutex，无手动 free ABI |
| `libfsxfs_file_entry_get_inode_number` | `DirEntry.ino` / `FileAttr.ino` / `FsNode.inode` |
| `libfsxfs_file_entry_get_creation_time` | `FileAttr.raw_crtime: Option<(i64,u32)>`；旧 inode None |
| `libfsxfs_file_entry_get_modification_time` | `FileAttr.raw_mtime`，原始 signed 秒/纳秒 |
| `libfsxfs_file_entry_get_access_time` | `FileAttr.raw_atime`，原始 signed 秒/纳秒 |
| `libfsxfs_file_entry_get_inode_change_time` | `FileAttr.raw_ctime`，原始 signed 秒/纳秒 |
| `libfsxfs_file_entry_get_file_mode` | `FileAttr.mode` → `FsStat.mode`，含 type bits |
| `libfsxfs_file_entry_get_number_of_links` | `FileAttr.nlink`；`inode_metadata` 核心 API |
| `libfsxfs_file_entry_get_owner_identifier` | `FileAttr.uid`；核心 API，未新增 IPC 字段 |
| `libfsxfs_file_entry_get_group_identifier` | `FileAttr.gid`；核心 API，未新增 IPC 字段 |
| `libfsxfs_file_entry_get_device_identifier` | `FileAttr.rdev` 原始 SysV device identifier |
| `libfsxfs_file_entry_get_device_number` | `FileAttr::device_numbers()` 返回 major/minor；仅 device 有值 |
| `libfsxfs_file_entry_get_utf8_name_size` | `DirEntry.name: Vec<u8>`；node ID 保留 raw bytes，显示不决定身份；转码/长度由调用方完成 |
| `libfsxfs_file_entry_get_utf8_name` | `DirEntry.name: Vec<u8>`；node ID 保留 raw bytes，显示不决定身份；转码/长度由调用方完成 |
| `libfsxfs_file_entry_get_utf16_name_size` | `DirEntry.name: Vec<u8>`；node ID 保留 raw bytes，显示不决定身份；转码/长度由调用方完成 |
| `libfsxfs_file_entry_get_utf16_name` | `DirEntry.name: Vec<u8>`；node ID 保留 raw bytes，显示不决定身份；转码/长度由调用方完成 |
| `libfsxfs_file_entry_get_utf8_symbolic_link_target_size` | `readlink` 返回原始 target 字节；`FsNode.link_target` 是 display text，正文保持原字节 |
| `libfsxfs_file_entry_get_utf8_symbolic_link_target` | `readlink` 返回原始 target 字节；`FsNode.link_target` 是 display text，正文保持原字节 |
| `libfsxfs_file_entry_get_utf16_symbolic_link_target_size` | `readlink` 返回原始 target 字节；`FsNode.link_target` 是 display text，正文保持原字节 |
| `libfsxfs_file_entry_get_utf16_symbolic_link_target` | `readlink` 返回原始 target 字节；`FsNode.link_target` 是 display text，正文保持原字节 |
| `libfsxfs_file_entry_get_number_of_extended_attributes` | `Volume::extended_attributes` 返回完整集合；count/index 由 Vec 提供；核心 API，未新增专用 IPC |
| `libfsxfs_file_entry_get_extended_attribute_by_index` | `Volume::extended_attributes` 返回完整集合；count/index 由 Vec 提供；核心 API，未新增专用 IPC |
| `libfsxfs_file_entry_has_extended_attribute_by_utf8_name` | `get_extended_attribute` 精确 namespace + raw name；absence/ambiguity 明确报错；存在检查由结果表达 |
| `libfsxfs_file_entry_has_extended_attribute_by_utf16_name` | `get_extended_attribute` 精确 namespace + raw name；absence/ambiguity 明确报错；存在检查由结果表达 |
| `libfsxfs_file_entry_get_extended_attribute_by_utf8_name` | `get_extended_attribute` 精确 namespace + raw name；absence/ambiguity 明确报错；存在检查由结果表达 |
| `libfsxfs_file_entry_get_extended_attribute_by_utf16_name` | `get_extended_attribute` 精确 namespace + raw name；absence/ambiguity 明确报错；存在检查由结果表达 |
| `libfsxfs_file_entry_get_number_of_sub_file_entries` | `readdir` 的 Vec/count/index 及 `lookup(parent,name)`；超过目录资源界限报错，不返回假完整列表 |
| `libfsxfs_file_entry_get_sub_file_entry_by_index` | `readdir` 的 Vec/count/index 及 `lookup(parent,name)`；超过目录资源界限报错，不返回假完整列表 |
| `libfsxfs_file_entry_get_sub_file_entry_by_utf8_name` | `readdir` 的 Vec/count/index 及 `lookup(parent,name)`；超过目录资源界限报错，不返回假完整列表 |
| `libfsxfs_file_entry_get_sub_file_entry_by_utf16_name` | `readdir` 的 Vec/count/index 及 `lookup(parent,name)`；超过目录资源界限报错，不返回假完整列表 |
| `libfsxfs_file_entry_read_buffer` | `Volume::read(ino, explicit_offset, length)`；应用 `read_file/stream_file` |
| `libfsxfs_file_entry_read_buffer_at_offset` | `Volume::read` / 应用 `read_file_range`，同一校验计划 |
| `libfsxfs_file_entry_seek_offset` | 显式 offset + EOF 裁剪；不维护 file_entry 隐含游标 |
| `libfsxfs_file_entry_get_offset` | 显式 offset 由调用方保留；不是缺少随机寻址 |
| `libfsxfs_file_entry_get_size` | `FileAttr.size` / `FsStat.size` |
| `libfsxfs_file_entry_get_number_of_extents` | `Volume::extents().len()`，含 allocated unwritten；holes 没有物理来源 |
| `libfsxfs_file_entry_get_extent_by_index` | `Extent { logical_offset, physical_offset, length, unwritten }`；FsStat 投影 written payload |
| `libfsxfs_extended_attribute_free` | Rust 所有权/Drop；EvidenceSession 生命周期及 Mutex，无手动 free ABI |
| `libfsxfs_extended_attribute_get_utf8_name_size` | `ExtendedAttribute.name` 原始字节；明确 namespace 前缀；长度/转码由调用方完成 |
| `libfsxfs_extended_attribute_get_utf8_name` | `ExtendedAttribute.name` 原始字节；明确 namespace 前缀；长度/转码由调用方完成 |
| `libfsxfs_extended_attribute_get_utf16_name_size` | `ExtendedAttribute.name` 原始字节；明确 namespace 前缀；长度/转码由调用方完成 |
| `libfsxfs_extended_attribute_get_utf16_name` | `ExtendedAttribute.name` 原始字节；明确 namespace 前缀；长度/转码由调用方完成 |
| `libfsxfs_extended_attribute_read_buffer` | `ExtendedAttribute.value`；完整二进制值，上限 64 KiB |
| `libfsxfs_extended_attribute_read_buffer_at_offset` | `Volume::read_extended_attribute`，包含 EOF/overflow 安全裁剪 |
| `libfsxfs_extended_attribute_seek_offset` | 显式 offset，在 bounded value 上寻址；无隐含游标 |
| `libfsxfs_extended_attribute_get_offset` | 调用方保留显式 offset |
| `libfsxfs_extended_attribute_get_size` | `ExtendedAttribute.value.len()` |
| `libfsxfs_extended_attribute_get_number_of_extents` | `ExtendedAttribute.extents.len()`；payload 不含 V5 block header |
| `libfsxfs_extended_attribute_get_extent_by_index` | payload logical/physical extents；应用 `extended_attributes` 映射原镜像/LVM |

## 实现与验证

- `vendor/xfs-fuse/src/libxfuse/sb.rs`：fallible superblock 与 geometry/feature 验证。
- `dinode.rs`：按 data/attr fork 实际容量解码 bmap root；旧指针间距猜测不再用于生产解码。
- `logical.rs`：extent list/B-tree 的统一校验、循环/层级/计数限制及 exact reads；只缓存一个当前文件计划。
- `directory.rs` / `legacy_dir.rs`：v2/v3 数据枚举与 v1 目录；缓存最多 32 个目录、250000 个条目。
- `attributes.rs`：内联、leaf/node/bmap、remote、精确名字与 payload 来源；单值 64 KiB、集合 64 MiB，node 遍历最多 65536 块。
- `symlink_extent.rs`：按 inode 逻辑长度解码 target，V5 每块 offset/address/owner/UUID/CRC 校验。
- `core/filesystem/volumes/xfs.rs` 与 `fs_dispatch.rs`：raw node ID、可寻址读取、时间/链接/来源映射；原证据及分区身份不变。

Bmap 深度最多 32，遍历最多 1000000 个块、4000000 个 extent；目录底层最多 100000 个条目，应用单次 listing 上限 50000。
达到限制会失败；没有将截断目录伪装成全目录。根 inode 打开时必须可解析，但后续目录项指向损坏 inode 仍按既有取证遍历规则记录日志并跳过。
这些资源上限不表示 parser 可以在块内部即时取消。

复现入口：

```text
just test-vendor xfs-fuse
just test xfs --lib
just fmt
just check
just clippy
```

合成回归覆盖格式 1–5、signed/ns/BIGTIME、64 位计数、metadata UUID、多卷上下文、内联/外置链接、
目录 inline/leaf/node/data/block、属性 shortform/leaf/node/bmap/remote、哈希碰撞与 namespace、
extent 空洞/unwritten/跨叶/EOF、CRC/owner/address/范围错误、源短读与原镜像/LVM payload 来源。
固定应用夹具来源、SHA-256 和重建脚本见 [test_data](../src-tauri/vendor/xfs-fuse/test_data/README.md)。

`LOVELYCARVE_XFS_TEST` / `LOVELYCARVE_XFS_LVM_TEST` 是已有外部检材回归，未设置时测试提前返回，不表示检材读取已验证。
本次没有执行 libfsxfs 二进制对真实镜像的目录/属性/正文哈希 oracle，也没有执行 Windows Dokan 实际挂载验收。
需要真实 IRIX、不同目录块/扇区尺寸、多层 Bmap、reflink、日志未回放检材与超大目录补充验证。
本次执行 `just test --lib`：1013 通过、12 ignored；XFS 外部检材变量未提供。
独立 reader 回归：29 通过、1 个上游示例式 hash 测试 ignored；应用 XFS 回归另覆盖直接/会话随机读和跨 segment 来源。
已有纯 Rust 后端不做 journal replay、quota 审计、独立 real-time 设备、删除/孤儿恢复，也不读取未知 incompatible layout。

## 参照来源与许可

除 [README](https://github.com/libyal/libfsxfs/blob/cf474697de8a32f7eba4122e469b2ecd818054c6/README) 与公开头文件，
对照 [superblock](https://github.com/libyal/libfsxfs/blob/cf474697de8a32f7eba4122e469b2ecd818054c6/libfsxfs/libfsxfs_superblock.c)、
[inode](https://github.com/libyal/libfsxfs/blob/cf474697de8a32f7eba4122e469b2ecd818054c6/libfsxfs/libfsxfs_inode.c)、
[directory](https://github.com/libyal/libfsxfs/blob/cf474697de8a32f7eba4122e469b2ecd818054c6/libfsxfs/libfsxfs_directory.c) 及相关属性/数据流源文件。
磁盘布局交叉核对 Linux XFS 的 [format](https://github.com/torvalds/linux/blob/master/fs/xfs/libxfs/xfs_format.h)、
[directory/attribute format](https://github.com/torvalds/linux/blob/master/fs/xfs/libxfs/xfs_da_format.h) 和
[bmap arrays](https://github.com/torvalds/linux/blob/master/fs/xfs/libxfs/xfs_bmap_btree.h)。Linux master 链接是辅助格式参考，libfsxfs 对照提交已固定。
新代码是根据字段结构独立编写的 Rust 实现，未复制 C 函数或第三方镜像。现有 xfuse 许可证继续保留。
