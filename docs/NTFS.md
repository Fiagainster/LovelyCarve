# NTFS 与 libyal/libfsntfs 能力对照

核对日期：2026-10-12。参照 [libfsntfs](https://github.com/libyal/libfsntfs/tree/faaa031d0f779578559d5c025c9583d103702bcc)
固定提交 `faaa031d0f779578559d5c025c9583d103702bcc` 的 README、
[公开头文件](https://github.com/libyal/libfsntfs/blob/faaa031d0f779578559d5c025c9583d103702bcc/include/libfsntfs.h.in)、
[NTFS 格式说明](https://github.com/libyal/libfsntfs/blob/faaa031d0f779578559d5c025c9583d103702bcc/documentation/New%20Technologies%20File%20System%20%28NTFS%29.asciidoc)
及 WOF、文件条目、安全描述符实现。生产读取仍为 Rust，不加载 libfsntfs DLL。

这份清单区分格式读取能力、核心 API 和桌面入口。源码对照及固定样本回归不等于
已用 libfsntfs 对所有检材做完整差分；未覆盖的真实场景列在验证边界中。

## 格式与核心能力

| 能力 | libfsntfs | LovelyCarve 当前实现与入口 | 验证 |
| --- | --- | --- | --- |
| NTFS 3.0 / 3.1、BPB、扇区/簇/记录尺寸、卷号 | 支持只读 | `Ntfs` + `NtfsVolume::volume_metadata` | vendor NTFS 真实格式样本、主工程合成卷 |
| 卷名、版本、卷状态 | 支持 | `volume_metadata` 保留原始 flags | 卷 3 驻留属性回归 |
| MFT 按记录读取、序列号、基记录引用、LSN、分配状态 | 支持 | 缓存 MFT run map；`record_metadata` | 跨 MFT 扩展记录、普通卷回归 |
| `$MFT` 自身的 `$ATTRIBUTE_LIST` | 支持 | 从已可映射前缀读取扩展，避免递归重入 | 合成分裂 MFT；断链/不连续明确失败 |
| 文件属性链与 ADS | 支持 | 按类型、原始 UTF-16 名称和连续 VCN 连接；instance 仅定位记录内属性 | 不同 instance、填充名称偏移、相邻不同 ADS |
| 驻留 / 非驻留 / 碎片 / 稀疏 / 未初始化尾部 | 支持 | `DataStream`，零 LCN 差值仍为分配簇；短读失败 | 稀疏、碎片、尾部、短读、损坏头和 runlist |
| 原始属性、类型/flags/name/size、映射段 | 支持 | `read_attribute_value`、`record_metadata.attributes.extents`；未知类型保留数值 | 驻留值、稀疏与已分配映射；压缩映射表示存储字节 |
| LZNT1 | 支持 | 完整/流式/范围/ADS/采样统一解码 | Windows 固定向量及检材记录 149731 |
| WOF XPRESS-Huffman 4K / 8K / 16K | 支持 | `WofCompressedData` + WOF 文件 provider；同一逻辑读取入口 | Windows 原生 codec 向量、压缩/存储混合块 |
| WOF LZX 32K | 支持 | WIM 变种 LZX，含最后一个不足 32K 的块 | wimlib C oracle 向量与跨块读取 |
| WOF offset table / 64 位 offsets | 支持 | N−1 个相对偏移、末端隐式边界；按需读取两项，缓存一个块 | 32 位表覆盖；64 位分支尚无 >4 GiB 固定样本 |
| 默认流和 ADS 随机读取 | 支持 | `read_range` / `read_data_stream_range`；`ntfs-ads:` 接入 `FsSession` 直接范围读取 | ADS 跨属性边界、LZNT1、WOF 跨块 |
| 目录小索引/大索引、按路径查找 | 支持 | `$I30`；`find_record_by_path` 精确名称匹配，不自动跨重解析点 | vendor 的大目录索引；循环索引回归 |
| 大小写敏感名称 / 同目录多个硬链接 | 支持 | 用 (record, raw name) / (parent, raw name) 保留全部名称边；DOS 别名仅从普通列表隐藏 | 同父目录 A.txt / a.txt、DOS 别名 |
| 所有 `$FILE_NAME` 名称、命名空间、父引用、FN 时间/尺寸/flags | 支持 | `record_metadata.filenames`，保留 DOS/POSIX/Win32/组合 namespace | FN 原始时间与 UTF-16 字节 |
| SI 时间、owner/security ID、USN、quota | 支持 | `record_metadata`，SI/FN 分开，100 ns FILETIME 整数不丢失 | SI/FN 不同时间、原始 FILETIME |
| Object ID、GUID、EA、bitmap 等属性 | 属性读取及部分结构化 getter | vendor 结构化 reader；主接口可枚举并读取原始属性值 | 未新增专用 GUI 解析面板 |
| 符号链接 / junction / reparse | 支持名称/标签/压缩方法 | bounded parser、substitute/print name、flags/raw data；目录和扫描投影为 Symlink | UTF-16 目标、越界拒绝；不跟随宿主路径 |
| 旧安全描述符属性 | 支持 | `security_descriptor` | 合成 self-relative SID |
| 现代安全描述符 `$Secure:$SII` / `$SDS` | 支持 | B-tree 查找 security ID；核对位置/长度与两个头；返回原始 SID/ACL 描述符 | 小索引查询、调用预算、缺失/越界失败；大索引复用 vendor 路径 |
| 删除 / 未分配记录 | 支持分配状态及 MFT 枚举 | 删除快照、全 MFT 边扫描、原始属性/内容读取 | 原有主工程删除/扫描回归；残留内容不保证可恢复 |
| 独立导出的 `$MFT` 元数据文件 | 支持 metadata-only | `NtfsMftMetadataFile`：按槽读取、fixup、卷 3 标签/版本、SI/FN、原始驻留属性 | 从合成卷导出的 MFT；非驻留内容明确要求原卷 |
| USN journal | 支持读取 journal record | 稀疏前缀跳过，逻辑 reader + V2/V3 解码；V4 不伪造日期 | 原有分块、页填充、FILETIME 精度回归 |
| BitLocker / VSS 检测 | 支持签名检测 | 由证据/加密/VSS 域处理；已解锁卷复用相同 NTFS reader | 现有 BitLocker / VSS 测试 |
| NTFS 1.0 / 1.1 / 1.2 | README 明确不支持 | 不声称旧版本完整兼容 | 无承诺 |
| EFS 解密 | README 明确不支持 | 明确拒绝逻辑内容读取，仍可检查记录元数据 | 不实现用户私钥导入 |
| NTFS 写入、journal replay / 修复 | 只读库范围外 | 不修改检材 | 范围外 |

## 接口和运行方式

`NtfsVolume` 新增的 metadata/security API 用于 Rust 调用及只读 JSON probe。
压缩读取、硬链接、重解析投影和 ADS 范围读取已经接入原有桌面/CLI 文件操作入口；
细粒度元数据、安全描述符、独立 MFT 尚未新增桌面专用面板。

```powershell
just run-example probe_ntfs_metadata 'D:\evidence\disk.E01' 0 149731
just run-example probe_ntfs_metadata --mft 'D:\exports\$MFT' 149731
just run-example probe_ntfs_stream 'D:\evidence\disk.E01' 0 149731 'D:\oracle.bin'
```

`read_attribute_value(record, instance, budget)` 读取物理属性内的值。
非驻留压缩值仍是编码后的存储，不能当作已解码正文；文件正文必须走
`read_file` / `stream_file` / `read_range`。扩展属性头可按其物理 record/instance 查询。
映射中的 offset 是卷内偏移；BitLocker 解密卷也采用该坐标，不能误当原镜像物理字节。

## 验证边界

- 严格 reader 对损坏属性/USA/BAAD 返回具体错误；未提供 libfsntfs 损坏条目的部分恢复对象。

- 固定二进制只含合成字节，来源、输入生成规则与 SHA-256 见
  [NTFS fixtures](../src-tauri/src/core/filesystem/volumes/ntfs/testdata/README.md)。
- WOF 回归在合成 NTFS 卷中使用独立原生 codec 输出；尚未完成大批真实 Compact/WOF
  文件的全量 libfsntfs 差分，不声称在所有 Windows 版本及损坏检材上完全一致。
- WIMBoot provider 1 需要外部 WIM 资源，当前明确报错；未知 provider/算法、缺 ADS
  和不合法 offset table 也报错，不把稀疏默认流的零数据当作正文。
- `ms-compress` 的 LZX 跟随 wimlib 的编码位零填充行为；WOF 本身没有内容校验和，
  成功解码不能证明数据未损坏，取证完整性仍由原镜像及导出哈希确认。
- 独立 MFT 无 BPB/cluster bytes；只报告实际存在的记录/驻留值，不猜簇大小或解密内容。
  扩展记录独立寻址，不把缺失的 volume 数据拼成空文件。
- 核心返回完整的安全描述符原始字节，并检查 SID/ACL 边界；未实现宿主权限应用、
  SDDL 美化或专用权限 GUI。
- Rust 最低版本升为 1.99；只启用 `ms-compress` 的 std/xpress/lzx features，
  发行 profile 给 codec `opt-level=3`。来源/许可保留在第三方声明。

## 实际回归结果

本次主工程 `just test --lib`：951 passed / 13 ignored；vendor 默认 feature 测试：14 passed。
`just fmt`、`just check`、`just clippy`、`just check-examples` 通过，clippy 保留 11 条既有警告。
额外尝试 vendor 的 `--no-default-features` 测试未通过：上游测试夹具依赖 std，
不计入本次生产 std reader 的验证结果，也不声称 no_std 测试已经通过。

用户 E01 的记录 149731：全文/流式/8 个范围与先前 Windows 原生 LZNT1 oracle 相同，
逻辑长度 3883，SHA-256 `16298630cc18d08942efa5faf964bd9429eeaa3af95c1425e25399906b3e83a3`。
新 metadata probe 读取到 NTFS 3.1、1,431,040 个 MFT 槽、该记录 4 个逻辑属性和 2 个名称；
安全 ID 1720 通过 `$SII` / `$SDS` 定位，返回 164 字节自相对安全描述符。
上述 JSON/正文 oracle 留在本机临时目录，不纳入仓库。

## 本地验证入口

`just fmt`、`just check`、`just clippy`、`just test --lib`、`just check-examples`，
以及 `just test-vendor ntfs`（vendor tests 不会由主工程 `--lib` 自动执行）。
资源紧张的 Windows 环境可设置 `CARGO_BUILD_JOBS=1`、`CARGO_PROFILE_TEST_DEBUG=0`、
`CARGO_INCREMENTAL=0` 后执行同一 just 配方。

## 公开 C API 逐项索引

下表覆盖固定头文件的全部函数，映射的是读取能力；Rust 不复制 C 指针管理、
UTF-8/UTF-16 缓冲区长度查询、通知流及 bfio handle 绑定。生命周期由 RAII，
错误由 Result/anyhow，输出由 tracing/JSON，取消由调用者 token 管理。

| libfsntfs 函数 | 本地对应能力/入口 |
| --- | --- |
| `libfsntfs_get_version` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_get_access_flags_read` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_get_codepage` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_set_codepage` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_check_volume_signature` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_check_volume_signature_wide` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_check_volume_signature_file_io_handle` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_check_mft_metadata_file_signature` | 公共辅助/生命周期：Rust 所有权、标准 IO、Result（不复制 C ABI） |
| `libfsntfs_check_mft_metadata_file_signature_wide` | 公共辅助/生命周期：Rust 所有权、标准 IO、Result（不复制 C ABI） |
| `libfsntfs_check_mft_metadata_file_signature_file_io_handle` | 公共辅助/生命周期：Rust 所有权、标准 IO、Result（不复制 C ABI） |
| `libfsntfs_notify_set_verbose` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_notify_set_stream` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_notify_stream_open` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_notify_stream_close` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_error_free` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_error_fprint` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_error_sprint` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_error_backtrace_fprint` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_error_backtrace_sprint` | Rust Result/tracing/编译版本；卷探针；UTF-16 名称（不暴露 C ABI） |
| `libfsntfs_volume_initialize` | `NtfsVolume::open*` / Rust RAII / 上层 CancellationToken |
| `libfsntfs_volume_free` | `NtfsVolume::open*` / Rust RAII / 上层 CancellationToken |
| `libfsntfs_volume_signal_abort` | `NtfsVolume::open*` / Rust RAII / 上层 CancellationToken |
| `libfsntfs_volume_open` | `NtfsVolume::open*` / Rust RAII / 上层 CancellationToken |
| `libfsntfs_volume_open_wide` | `NtfsVolume::open*` / Rust RAII / 上层 CancellationToken |
| `libfsntfs_volume_open_file_io_handle` | `NtfsVolume::open*` / Rust RAII / 上层 CancellationToken |
| `libfsntfs_volume_close` | `NtfsVolume::open*` / Rust RAII / 上层 CancellationToken |
| `libfsntfs_volume_has_bitlocker_drive_encryption` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_has_volume_shadow_snapshots` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_bytes_per_sector` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_cluster_block_size` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_mft_entry_size` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_index_entry_size` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_utf8_name_size` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_utf8_name` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_utf16_name_size` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_utf16_name` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_version` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_flags` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_serial_number` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_root_directory` | 按记录 / `find_record_by_path` / `children(5)` |
| `libfsntfs_volume_get_number_of_file_entries` | `volume_metadata` / 证据层 BitLocker、VSS 检测 |
| `libfsntfs_volume_get_file_entry_by_index` | 按记录 / `find_record_by_path` / `children(5)` |
| `libfsntfs_volume_get_file_entry_by_utf8_path` | 按记录 / `find_record_by_path` / `children(5)` |
| `libfsntfs_volume_get_file_entry_by_utf16_path` | 按记录 / `find_record_by_path` / `children(5)` |
| `libfsntfs_volume_get_usn_change_journal` | `visit_usn_journal` |
| `libfsntfs_file_entry_free` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_is_empty` | `list_data_streams` 的逻辑长度 / 缺失流类型错误 |
| `libfsntfs_file_entry_is_allocated` | `record_metadata.allocated` |
| `libfsntfs_file_entry_is_corrupted` | 严格结构校验返回具体 `NtfsError`；不提供损坏条目的部分恢复对象 |
| `libfsntfs_file_entry_get_file_reference` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_base_record_file_reference` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_parent_file_reference` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_parent_file_reference_by_attribute_index` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_journal_sequence_number` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_creation_time` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_modification_time` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_access_time` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_entry_modification_time` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_file_attribute_flags` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_i30_file_reference` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_i30_creation_time` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_i30_modification_time` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_i30_access_time` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_i30_entry_modification_time` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_i30_size` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_i30_file_attribute_flags` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf8_name_size` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf8_name` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf16_name_size` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf16_name` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_name_attribute_index` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf8_name_size_by_attribute_index` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf8_name_by_attribute_index` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf16_name_size_by_attribute_index` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf16_name_by_attribute_index` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf8_path_hint_size` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf8_path_hint` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf16_path_hint_size` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf16_path_hint` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf8_symbolic_link_target_size` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf8_symbolic_link_target` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf16_symbolic_link_target_size` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_utf16_symbolic_link_target` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_security_descriptor_size` | `security_descriptor` |
| `libfsntfs_file_entry_get_security_descriptor` | `security_descriptor` |
| `libfsntfs_file_entry_get_number_of_attributes` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_attribute_by_index` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_has_directory_entries_index` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_has_default_data_stream` | `list_data_streams` / `read_data_stream*` |
| `libfsntfs_file_entry_has_i30_entry` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_is_symbolic_link` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_number_of_alternate_data_streams` | `list_data_streams` / `read_data_stream*` |
| `libfsntfs_file_entry_get_alternate_data_stream_by_index` | `list_data_streams` / `read_data_stream*` |
| `libfsntfs_file_entry_has_alternate_data_stream_by_utf8_name` | `list_data_streams` / `read_data_stream*` |
| `libfsntfs_file_entry_has_alternate_data_stream_by_utf16_name` | `list_data_streams` / `read_data_stream*` |
| `libfsntfs_file_entry_get_alternate_data_stream_by_utf8_name` | `list_data_streams` / `read_data_stream*` |
| `libfsntfs_file_entry_get_alternate_data_stream_by_utf16_name` | `list_data_streams` / `read_data_stream*` |
| `libfsntfs_file_entry_get_number_of_sub_file_entries` | `children` / 精确名称 `find_record_by_path` |
| `libfsntfs_file_entry_get_sub_file_entry_by_index` | `children` / 精确名称 `find_record_by_path` |
| `libfsntfs_file_entry_get_sub_file_entry_by_utf8_name` | `children` / 精确名称 `find_record_by_path` |
| `libfsntfs_file_entry_get_sub_file_entry_by_utf16_name` | `children` / 精确名称 `find_record_by_path` |
| `libfsntfs_file_entry_read_buffer` | 逻辑 full / stream / range reader |
| `libfsntfs_file_entry_read_buffer_at_offset` | 逻辑 full / stream / range reader |
| `libfsntfs_file_entry_seek_offset` | 逻辑 full / stream / range reader |
| `libfsntfs_file_entry_get_offset` | 逻辑 full / stream / range reader |
| `libfsntfs_file_entry_get_size` | `record_metadata` / `list_data_streams` / 原始属性值；RAII 管理条目 |
| `libfsntfs_file_entry_get_number_of_extents` | `record_metadata.attributes.extents`（编码流为存储映射） |
| `libfsntfs_file_entry_get_extent_by_index` | `record_metadata.attributes.extents`（编码流为存储映射） |
| `libfsntfs_attribute_free` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_get_type` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_get_data_flags` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_get_utf8_name_size` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_get_utf8_name` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_get_utf16_name_size` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_get_utf16_name` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_get_data_vcn_range` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_get_data_size` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_get_valid_data_size` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_list_attribute_get_number_of_entries` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_list_attribute_get_entry_by_index` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_list_entry_free` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_list_entry_get_attribute_type` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_list_entry_get_file_reference` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_list_entry_get_utf8_name_size` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_list_entry_get_utf8_name` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_list_entry_get_utf16_name_size` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_attribute_list_entry_get_utf16_name` | `record_metadata.attributes` / `read_attribute_value` |
| `libfsntfs_file_name_attribute_get_parent_file_reference` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_creation_time` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_modification_time` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_access_time` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_entry_modification_time` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_file_attribute_flags` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_name_space` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_utf8_name_size` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_utf8_name` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_utf16_name_size` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_file_name_attribute_get_utf16_name` | `record_metadata.filenames` / 原始 `$FILE_NAME` |
| `libfsntfs_object_identifier_attribute_get_droid_file_identifier` | vendor `NtfsObjectId` / 原始 `$OBJECT_ID` |
| `libfsntfs_object_identifier_attribute_get_birth_droid_volume_identifier` | vendor `NtfsObjectId` / 原始 `$OBJECT_ID` |
| `libfsntfs_object_identifier_attribute_get_birth_droid_file_identifier` | vendor `NtfsObjectId` / 原始 `$OBJECT_ID` |
| `libfsntfs_object_identifier_attribute_get_birth_droid_domain_identifier` | vendor `NtfsObjectId` / 原始 `$OBJECT_ID` |
| `libfsntfs_reparse_point_attribute_get_tag` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_reparse_point_attribute_get_compression_method` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_reparse_point_attribute_get_utf8_substitute_name_size` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_reparse_point_attribute_get_utf8_substitute_name` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_reparse_point_attribute_get_utf16_substitute_name_size` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_reparse_point_attribute_get_utf16_substitute_name` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_reparse_point_attribute_get_utf8_print_name_size` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_reparse_point_attribute_get_utf8_print_name` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_reparse_point_attribute_get_utf16_print_name_size` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_reparse_point_attribute_get_utf16_print_name` | `record_metadata` reparse 字段 / WOF reader / 原始值 |
| `libfsntfs_security_descriptor_attribute_get_security_descriptor_size` | `security_descriptor` / 原始安全属性 |
| `libfsntfs_security_descriptor_attribute_get_security_descriptor` | `security_descriptor` / 原始安全属性 |
| `libfsntfs_standard_information_attribute_get_creation_time` | `record_metadata` / 原始 `$STANDARD_INFORMATION` |
| `libfsntfs_standard_information_attribute_get_modification_time` | `record_metadata` / 原始 `$STANDARD_INFORMATION` |
| `libfsntfs_standard_information_attribute_get_access_time` | `record_metadata` / 原始 `$STANDARD_INFORMATION` |
| `libfsntfs_standard_information_attribute_get_entry_modification_time` | `record_metadata` / 原始 `$STANDARD_INFORMATION` |
| `libfsntfs_standard_information_attribute_get_file_attribute_flags` | `record_metadata` / 原始 `$STANDARD_INFORMATION` |
| `libfsntfs_standard_information_attribute_get_owner_identifier` | `record_metadata` / 原始 `$STANDARD_INFORMATION` |
| `libfsntfs_standard_information_attribute_get_security_descriptor_identifier` | `record_metadata` / 原始 `$STANDARD_INFORMATION` |
| `libfsntfs_standard_information_attribute_get_update_sequence_number` | `record_metadata` / 原始 `$STANDARD_INFORMATION` |
| `libfsntfs_volume_information_attribute_get_version` | `volume_metadata.version/flags` |
| `libfsntfs_volume_information_attribute_get_flags` | `volume_metadata.version/flags` |
| `libfsntfs_volume_name_attribute_get_utf8_name_size` | `volume_metadata.name` / 原始 UTF-16 属性值 |
| `libfsntfs_volume_name_attribute_get_utf8_name` | `volume_metadata.name` / 原始 UTF-16 属性值 |
| `libfsntfs_volume_name_attribute_get_utf16_name_size` | `volume_metadata.name` / 原始 UTF-16 属性值 |
| `libfsntfs_volume_name_attribute_get_utf16_name` | `volume_metadata.name` / 原始 UTF-16 属性值 |
| `libfsntfs_data_stream_free` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_get_utf8_name_size` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_get_utf8_name` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_get_utf16_name_size` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_get_utf16_name` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_read_buffer` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_read_buffer_at_offset` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_seek_offset` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_get_offset` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_get_size` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_get_number_of_extents` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_data_stream_get_extent_by_index` | `list_data_streams` / named stream full、stream、range / extents |
| `libfsntfs_mft_metadata_file_initialize` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_free` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_open` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_open_wide` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_open_file_io_handle` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_close` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_get_utf8_volume_name_size` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_get_utf8_volume_name` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_get_utf16_volume_name_size` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_get_utf16_volume_name` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_get_volume_version` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_get_volume_flags` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_get_number_of_file_entries` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_mft_metadata_file_get_file_entry_by_index` | `NtfsMftMetadataFile`（元数据；外部簇不可用） |
| `libfsntfs_usn_change_journal_free` | `visit_usn_journal` / `read_usn_journal` |
| `libfsntfs_usn_change_journal_get_offset` | `visit_usn_journal` / `read_usn_journal` |
| `libfsntfs_usn_change_journal_read_usn_record` | `visit_usn_journal` / `read_usn_journal` |

共 197 个公开函数；头文件 SHA-256：`16767e38a7c50f054325abc319e37bb34b6e819323dda183d196ecdb79ea3139`。
