# FAT / exFAT 与 libfsfat 能力对照

对照日期：2026-10-12。固定上游：[libfsfat `98782bf5eb33a804f118e27b77382aeff7f02b1d`](https://github.com/libyal/libfsfat/tree/98782bf5eb33a804f118e27b77382aeff7f02b1d)。审阅公开头文件的 **53 个函数**、boot record、allocation table、directory、directory entry、file system、file entry、name、格式文档与工具入口。
上游标记为 experimental，包含 FAT12/16/32 和 exFAT，打开时仍拒绝写访问。

证据：[公开 API](https://github.com/libyal/libfsfat/blob/98782bf5eb33a804f118e27b77382aeff7f02b1d/include/libfsfat.h.in)、[卷打开](https://github.com/libyal/libfsfat/blob/98782bf5eb33a804f118e27b77382aeff7f02b1d/libfsfat/libfsfat_volume.c)、[目录与簇链](https://github.com/libyal/libfsfat/blob/98782bf5eb33a804f118e27b77382aeff7f02b1d/libfsfat/libfsfat_file_system.c)、[名称与时间](https://github.com/libyal/libfsfat/blob/98782bf5eb33a804f118e27b77382aeff7f02b1d/libfsfat/libfsfat_directory_entry.c)、[格式文档](https://github.com/libyal/libfsfat/blob/98782bf5eb33a804f118e27b77382aeff7f02b1d/documentation/File%20Allocation%20Table%20%28FAT%29%20format.asciidoc)。

## 格式能力

| 能力 | libfsfat | LovelyCarve 当前结果 |
| --- | --- | --- |
| FAT12/16/32 | BPB、packed 12-bit / 16-bit / 32-bit FAT | 已有三种读者；本次将检测改为按数据簇数分类，忽略可能错误的 `FATxx` 提示字符串 |
| 固定/簇链根目录 | FAT12/16 固定区域，FAT32 簇链 | 支持；FAT32 最后一个 EOC 簇仍读取 |
| 非标准逻辑扇区 | 接受 512–32768 字节的二次幂 | 检测、活文件与恢复统一接受该范围；独立测试覆盖 8192 字节 |
| exFAT | 独立 boot / entry-set 分支 | 已有独立读者；本次补齐范围、初始化尾部、精度与 extent |
| 来源与卷偏移 | 普通/wide 文件名、libbfio | DiskImage / partition slice，复用已有镜像格式、物理盘和解密字节来源 |
| 备份引导 | 未以恢复作为公开 API | FAT32 常规 sector 6 / BPB 指定位置；exFAT sector 12 同时供活文件与恢复使用，只覆盖读到的 BPB，不写回证据 |
| 活动 FAT | 公开 API 无 table selector | 活文件与恢复均尊重 FAT32 禁用镜像时的 table selector 及 exFAT ActiveFat |
| 短名、LFN、UTF-16 | 支持 | FAT read/stat 共用长名、短名别名与大小写规则；exFAT 修复跨 FileName 槽位的 surrogate pair |
| OEM codepage | 全局 codepage 用于窄系统字符串；名称转换仍有 `TODO add codepage support` | 默认 FAT converter 对非 ASCII 短名有损；未误算为上游具备完整 OEM 名称支持；LFN/exFAT 使用 UTF-16 |
| exFAT 名称比较 | name / directory-entry 比较 | 精确 Unicode 名及 ASCII 不区分大小写；尚未根据自定义磁盘 Up-case Table 完成所有 Unicode 折叠 |
| 原始属性 | read-only/hidden/system/volume/directory/archive 等 | 新增 file_metadata 原始 FAT u8 / exFAT u16 flags；不混入 POSIX mode |
| 目录项标识 | 卷内短名目录项偏移，可反向打开 | 活节点 inode/stat/metadata 保留**绝对镜像**目录项偏移；应用 ID 仍按路径寻址，无任意裸偏移反向打开接口 |
| FAT 时间 | 日期、时间与 10ms fraction | 保留无时区本地时间；活项/删除项保留创建时间精度，非法 fraction 不发布 |
| exFAT 时间 | packed time、fraction、offset 字段 | 修复 signed 7-bit offset、OffsetValid 未设置与 10ms fraction；活项/删除项共用转换 |
| 全量/流式/范围读取 | read buffer、at-offset、seek | 两种适配器均接入 Volume::read_range；EOF 空结果、跨 EOF 裁剪；现有预览与 Dokan 读取自动复用 |
| 物理 extent | 数量与按索引取 extent | stat 与 data_runs 从实际读者分配图导出；相邻物理簇合并，按逻辑长度裁剪，无簇尾 slack |
| NoFatChain | 连续 stream extent | 活文件支持；本次同时修复连续 exFAT 子目录中的删除项扫描 |
| ValidDataLength / DataLength | 分别解析 stream 字段 | 修复 VDL 被当成文件长度：大小取 DataLength，VDL 后返回零，完整/流式/范围/采样一致 |
| 损坏与短读 | 错误、边界与重复簇检查 | 补齐循环、非法/坏簇、FAT 容量、分区外 extent、短读错误；不得把前缀当完整文件 |
| 删除项 | FAT 0xe5 标为 unallocated，在普通枚举中跳过；无公开 undelete API | 已有删除槽位/entry-set 扫描；本次保留同名候选并按父目录和槽位区分 ID，只有完整单 run 可读 |
| 只读 | 拒绝写访问 | FAT bookkeeping 写由 cursor 消化；exFAT 只暴露读；测试比对访问前后镜像所有字节 |

## 53 个公开 API 的完整映射

以下省略 `libfsfat_`；大括号表示多个真实函数。例如 `volume_get_{utf8,utf16}_label{,_size}` 表示四个函数。“对应”是应用提供相应工作流，不代表提供 C ABI。

| 函数组 | 数量 | 当前对应或差异 |
| --- | ---: | --- |
| get_version、get_access_flags_read | 2 | 应用/crate 版本、只读访问契约 |
| get_codepage、set_codepage | 2 | 无全局窄字符串 codepage 状态；Rust Unicode 路径；OEM 限制见上表 |
| check_volume_signature{,_wide,_file_io_handle} | 3 | partitions parser、BPB 验证、证据字节来源 |
| notify_set_verbose、notify_set_stream、notify_stream_open、notify_stream_close | 4 | tracing / RUST_LOG，无 FILE* 日志 API |
| error_free、error_fprint、error_sprint、error_backtrace_fprint、error_backtrace_sprint | 5 | anyhow::Result、错误上下文和传播、RAII |
| volume_initialize、volume_free、volume_close | 3 | Volume / Arc / EvidenceSession 卸载生命周期 |
| volume_signal_abort | 1 | 长任务使用 TaskManager/CancellationToken 和 stream visitor；同步 FAT 加载/目录打开无独立 abort handle |
| volume_open{,_wide,_file_io_handle} | 3 | Unicode 原生路径、DiskImage 和 partition 字节接口 |
| volume_get_file_system_format | 1 | PartitionKind、FAT volume_metadata.format、exFAT 适配器身份 |
| volume_get_{utf8,utf16}_label{,_size} | 4 | volume_metadata.label；Rust String 可转 UTF-8/UTF-16，无 C buffer 长度查询 |
| volume_get_root_directory | 1 | 分区节点、root_children、children 空路径 |
| volume_get_file_entry_by_identifier | 1 | 无裸目录项偏移反向打开；路径 ID 可打开，偏移作为来源 metadata 保留 |
| volume_get_file_entry_by_{utf8,utf16}_path | 2 | Unicode 字符串路径；UTF-16 在系统边界转换 |
| file_entry_free、file_entry_get_identifier | 2 | RAII、inode / metadata.entry_offset；注意绝对镜像偏移与上游卷内偏移的差别 |
| file_entry_get_{access,creation,modification}_time | 3 | Mactb accessed/created/modified，保留精度和时区依据 |
| file_entry_get_file_attribute_flags | 1 | file_metadata.attributes，后端 Rust 接口 |
| file_entry_get_{utf8,utf16}_name{,_size} | 4 | FsNode.name、metadata.name / FAT short_name、标准 Unicode 转换 |
| file_entry_get_number_of_sub_file_entries、file_entry_get_sub_file_entry_by_index | 2 | children Vec 长度/索引，上层 filedb 分页 |
| file_entry_get_sub_file_entry_by_{utf8,utf16}_name | 2 | 名称/路径 lookup，exFAT Unicode 折叠限制见上表 |
| file_entry_read_buffer、file_entry_read_buffer_at_offset | 2 | read_file、stream_file、read_file_range |
| file_entry_seek_offset、file_entry_get_offset | 2 | 无状态显式 offset 请求；vendor File 支持 Seek，无跨 IPC 共享 cursor |
| file_entry_get_size | 1 | FsNode/FsStat.size；exFAT 使用 DataLength |
| file_entry_get_number_of_extents、file_entry_get_extent_by_index | 2 | FsStat.data_runs / data_runs / live_data_runs 长度与索引 |
| **合计** | **53** | 所有对应、接口差异与剩余缺口均明确列出 |

fsfatinfo 的卷信息与文件层级对应现有镜像信息、文件树、CLI；fsfatmount 对应现有 Dokan 只读挂载。Python bindings 是另一种语言封装，不加入桌面取证运行时。

## 实现与边界

`volumes/fat/read.rs` 维护 validated extent 计划，供完整/流式/范围/stat 使用；`volumes/exfat/read.rs` 使用 vendor 的实际分配图。
`vendor/fatfs` 增加目录项偏移、短名匹配、有限 extent 迭代与目录重复簇检查；`vendor/exfat` 修复逻辑大小、初始化尾部、严格簇链、时间与 UTF-16，并提供 native extent/属性。
`fat_deleted` / `exfat_recover` 统一活动 FAT、时间、同名记录身份与恢复目录读取。

范围读取跳过不需要的**正文**，仍有路径解析和簇链元数据遍历成本，不宣称任意 FAT 跳转是 O(1)。exFAT 正文打开仍重新加载 FAT；目录 snapshot walk 继续复用 Root。

FAT 本地时间和 OffsetValid 未设置的 exFAT 时间输出为 `YYYY-MM-DDTHH:mm:ss.SSS`，不附加虚构的 Z。已知 exFAT offset 才归一化为 UTC RFC3339。
现有时间线只投影有时区的时间，不猜测检材所在时区。已有快照/索引仍保留旧字段，需要重建后使用新结果。

新 metadata 是后端 Rust 接口；已有 inode 字段返回目录项偏移。没有新增 Tauri 命令或 UI，raw 属性与卷序列号不会自动新增到详情面板。

尚未覆盖：任意裸目录项偏移反向打开、完整 OEM 多字节代码页、自定义 exFAT Up-case Table 的 Unicode 比较，以及 Boot Region/entry-set/upcase 全部 checksum 的严格验证链。
删除连续 run 和 FAT32 高字截断重建仍为候选推断，未保证簇未重用；碎片删除正文、缺失父链及删除目录递归未提供完整恢复。删除 LFN 序号丢失后的名称仍是 best effort。

## 验证

```powershell
just fmt
just check
just clippy
just test
just test-vendor fatfs
just test-vendor exfat
```

专项回归为 `just test fat`。独立合成样本逐字段构造，不由待测库格式化，不含检材数据：

| 测试 | 覆盖 |
| --- | --- |
| volumes/fat/tests.rs | 三种 FAT、碎片范围/EOF、错误链、活动 FAT32、大逻辑扇区、属性/偏移、10ms 时间、同名删除项、证据不变 |
| volumes/exfat/fixture_tests.rs | 碎片/连续、VDL 0/700/1100、跨初始化边界、尾部零、跨槽位 surrogate pair、UTC-5、属性/偏移、坏链、备份 BPB、连续子目录删除项 |
| fat_deleted / exfat_recover 原有测试 | 删除短名/LFN/entry-set、目录 slack、最后 EOC 簇、备份选择、高字截断候选 |
| vendor/exfat 时间测试 | 未知/正/负 offset、10ms 上限与非法值 |

本次没有运行当前 libfsfat 二进制逐文件 differential oracle，也没有验证真实 Windows 挂载；证据为固定源码审计、独立镜像与本地测试，不宣称格式完全兼容。
实现独立编写，未复制 LGPL C 实现；fatfs/exfat 现有许可证与来源继续保留。
