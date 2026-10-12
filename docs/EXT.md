# ext2/3/4 能力与 libfsext 对照

本次对照基准为 2026-10-12 获取的
[libyal/libfsext](https://github.com/libyal/libfsext/tree/48d3d73911a69ef0fc100ed190d8398d1200f08d)
提交 `48d3d73911a69ef0fc100ed190d8398d1200f08d`。
对照覆盖 README、公开头文件、超级块、组描述符、inode、目录、块映射、extent、属性和数据流实现及相应测试。
这是源码与格式契约对照；没有编译运行 libfsext，不能视为两个实现对真实镜像的逐文件 oracle 验证。

LovelyCarve 保留纯 Rust `ext4-view` 及应用层取证适配器。没有引入 libfsext 的 C 代码、DLL、运行时依赖或 LGPL 链接。
磁盘、分区、LVM 的 ext 卷继续使用现有节点 ID、IPC、预览、导出、搜索与快照流程。

## 能力矩阵

“读者 API”表示后端可调用，不等于桌面属性面板已经展示该字段。
libfsext 的声明来自 README；具体边界以固定提交中的实现为准。

| 能力 | libfsext | LovelyCarve 对照前 | 本次结果与入口 |
| --- | --- | --- | --- |
| ext2 / ext3 / ext4 只读访问 | 支持 | 常见卷支持 | 保留；新增原有 ext2、ext3、4 KiB 样本回归 |
| 原始 ext2 revision 0 | 支持 | 使用动态 inode 大小字段 | 静态 revision 固定 128-byte inode |
| 没有 FILETYPE 的旧目录格式 | 支持 | 打开卷时拒绝 | 16-bit 名称长度，从目标 inode 取类型 |
| FILETYPE 为 UNKNOWN 的目录项 | 可按 inode 取类型 | 类型解码失败 | 从目标 inode 取类型；非法类型仍报错 |
| 根目录、inode 编号、路径查找 | 支持 | 支持 | 保留稳定 inode ID、按路径/编号访问 |
| 原始名称、UTF-8 / UTF-16 | C/Python API 提供转换 | 原始字节名称，UI lossy 显示 | 保留；Rust 字符串无需复制 C 的 wide API |
| 硬链接 | 目录项与 inode 关联 | 目录项分别投影 | 保留；`Metadata::links` 暴露链接数 |
| 快速、数据块符号链接 | 支持 | 支持 | 保留；新增 `read_link_inode` |
| 内联文件的前 60 字节 | 声明支持 | 卷特性被拒绝，上层存在不可达/不完整分支 | 支持，`File` 保留完整 resident payload |
| 内联文件的 `system.data` 后续部分 | 数据流有内联分段处理 | 缺失 | 拼接、EOF 校验、跨边界 seek/read |
| 内联目录 | 固定提交读取 `i_block`，最多 60 字节 | 不支持 | 枚举 `i_block[4..60]` 与 xattr 两段，重建 `.` / `..`；每段独立校验记录边界 |
| 内联符号链接 | 按 inode/流结构处理 | 超过 60 字节不展示 | `read_link_inode` 使用完整内联内容 |
| inode 内扩展属性 | 支持 | 最小 OverlayFS 解码器 | checked reader，保留二进制值、展开命名空间 |
| 外部属性块 | 支持 | 最小解码器 | 检查 magic、块数、值范围及 metadata CRC32C |
| EA-inode 属性值 | 支持，部分校验字段 TODO | 跳过 | 检查 feature、目标编号、EA 标记及长度，读取普通块/extent；禁止递归内联 EA 引用 |
| 属性按名称查找、值范围读 | 独立对象 API | 仅返回可解码值 | `extended_attributes` 返回属性数组，可按名称和 value slice 访问；单值上限 64 KiB |
| 属性自己的物理 extent 查询 | 独立对象 API | 无独立公开接口 | 本次未新增；属性 API 返回值字节，不返回 EA 存储布局 |
| direct / single / double / triple indirect | 支持 | 正文读者支持，物理 runs 只列直接块 | 补齐全部层级；连续块合并；空指针跳过整个子树 |
| extent 树、64-bit 物理块 | 支持 | 支持 | 保留；校验容量、深度递减、非零长度与偏移乘加 |
| 空洞与尾部空洞 | 支持 | 支持 | 保留，完整/范围/流式读共享逻辑片计划 |
| unwritten extent | extent 标志参与数据流 | 上层可能暴露残留物理字节 | 上层与 vendored reader 均读零；不列为正文物理 runs |
| 已初始化的 32768-block extent | 有长度处理 | 上层掩码得到零长度 | 正确保留 `ee_len == 0x8000`，只有更大值是 unwritten |
| 文件 extent / 内容区间 | 公开 extent/seek API | extent runs + 块级间接读回退 | 完整间接 runs；范围读不分配从文件头到 offset 的前缀 |
| 64 KiB 目录记录长度 | 格式处理 | sentinel 未处理 | `0` / `0xffff` 解为整块长度；拒绝不现实的超过 64 KiB 块 |
| `meta_bg` | 支持 | 拒绝打开 | 按 descriptor block 和 `s_first_meta_bg` 定位，覆盖 sparse / sparse_super2 |
| `flex_bg` / 64-bit / checksum seed | 支持 | 支持 | 保留，使用 descriptor 的真实 inode table 地址 |
| MMP / large_dir | 固定提交特性白名单有边界 | 拒绝整个卷 | 允许只读；MMP 不涉及写入协调；large_dir 仍使用既有遍历/htree |
| casefold 卷的现有原始名称 | 特性白名单接受 | 未知 incompat 位拒绝 | 可枚举和按原始名称查找；不实现 Unicode casefold 等价查找 |
| atime / mtime / ctime / crtime | 支持 | 上层只有低 32 位，缺 birth；读者 signed 转换有缺陷 | signed seconds + epoch bits + 纳秒；按 `i_extra_isize` 判断字段存在；crtime 投影到 `Mactb.birth` |
| dtime | 支持 | 原始值与删除标记 | 保留；读者额外提供 deletion time，孤儿链复用值不能直接当删除事实 |
| mode / UID / GID / links / device | 公开 API | mode/links 原始字段，其他未完整暴露 | `Metadata` 暴露完整字段、device major/minor；应用 RawInode 增加 UID/GID |
| UUID / label | 支持 | 读者有 API | 保留 |
| 版本、feature masks、inode slots、块计数、mount path/time | 支持 | 部分散落在 raw superblock 读取 | `VolumeInfo`、`Ext4Volume::volume_info`；非 64-bit 卷忽略计数高位 |
| 超级块 / inode / extent / 目录 CRC32C | 实现中存在校验与 TODO | vendored reader 支持 | 保留；新增外部 xattr CRC32C 与损坏 htree count 边界 |
| 旧 GDT CRC16 | 实现有 CRC16 处理 | 未验证 | 新增 CRC16；checksum 字段跳过，不按 CRC32C 的方式补零 |
| 内部 journal | 与文件结构关联 | 自动只读 replay + 历史块解析 | 保留现有 JBD/JBD2 实现与 journal UI |
| 外部 journal 设备 / 跨设备 replay | 接受相关特性，但不等于重放能力 | 不支持 | 仍不接入外部设备；单独 journal-device 格式明确拒绝 |
| ext v1、压缩、fscrypt 解密 | README 明确不支持 | 不支持 | 不实现；加密正文读取明确报错，不能把密文当明文 |
| 删除项 / 孤儿扫描 | 没有独立完整恢复契约 | 目录 slack 候选 | 保留 slack 恢复；不声称完整 undelete/孤儿扫描 |
| abort / IO 抽象 / mount 工具 | C/BFIO、工具层 | DiskImage、TaskManager、Dokan、CLI | 继续采用本地架构，不复制 C 生命周期与诊断控制 API |

## 校验和边界

内联内容必须有 inode 内的 `system.data`；不能把缺失尾部补零当完整文件。属性条目和值不能越界或覆盖属性表。
EA-inode 单个属性值上限为 Linux 的 64 KiB，保留原始二进制值；名称使用现有 lossy 显示语义。
本次验证 inode 和外部属性块校验和，但未补齐 EA 值 hash、条目 hash 与引用计数一致性审计。

应用层 extent 和间接块正文使用同一 `DataPiece` 计划：真实空洞/unwritten 读零；已分配范围越过分区/LVM 或来源短读则失败。
目录 slack 扫描使用完整间接块 runs；内联 inode 没有外部正文 runs，不能把它的内容字节解释成块指针。

现有 raw inode / extent 取证辅助路径与正常读者的严格 checksum 路径仍有区别；本次没有将损坏元数据“修复”为可信证据。
`FsStat` 和桌面属性面板未新增 UID/GID/device/xattr/volume-history 字段，也没有新增 IPC。
这些字段在读者/卷 API 中可用；内联文件、目录、时间戳和间接读取的改动直接进入既有应用流程。

## 验证

新增 `test_data/audit_ext.bin` 是可复现的 64 KiB 合成记录镜像，生成脚本为
`test_data/generate_audit_fixture.py`。不含用户检材；位图未填充，不可当作可挂载/e2fsck 正常卷。
SHA-256：`8ab0d910d74afccd1f47f84725a1c39a2d642494646342627d3c43272dc2081a`。

读者回归覆盖内联两段、目录原始名称查找、EA 值与错误引用、外部属性块 checksum、signed/epoch/nanosecond 时间、
旧目录格式、revision 0、meta_bg 的三种 superblock 布局、unwritten、三重间接 seek、64 KiB sentinel、特殊 inode 元数据，
并打开已有 ext2 / ext3 / 4 KiB upstream 镜像。应用层回归比较完整、流式和跨边界范围读，验证相邻分区字节不可被读取。

运行入口：

```powershell
just test-vendor ext4-view
just test ext4
just fmt
just check
just clippy
just test
```

本机默认调试构建在全量 test/example 编译时遇到 Windows `1455`（页面文件不足）和
`rustc` `STATUS_ACCESS_VIOLATION`。单线程、关闭增量编译与调试信息后，完整库测试可运行：

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
just test
```

这些是验证进程的设置；仓库构建配置未修改。最终 `just test` 完整通过：953 个后端测试、2 个集成测试通过，
12 个条件测试跳过；二进制、example 与 doc-test 构建完成。`just test-vendor ext4-view` 的 96 个测试全部通过。

真实 `mke2fs` 生成的 inline_data + EA-inode + meta_bg 组合卷、fscrypt、外部 journal、超大 large_dir/casefold 目录和
libfsext 逐文件 digest oracle 仍需要独立实盘/工具验证。本机没有可用 WSL/e2fsprogs，因此本次不声称这些验证已完成。

格式依据：Linux 内核 [inode](https://www.kernel.org/doc/html/latest/filesystems/ext4/inodes.html)、
[扩展属性](https://www.kernel.org/doc/html/latest/filesystems/ext4/attributes.html)、
[block groups](https://www.kernel.org/doc/html/latest/filesystems/ext4/blockgroup.html)，
以及固定提交的 [公开 API](https://github.com/libyal/libfsext/blob/48d3d73911a69ef0fc100ed190d8398d1200f08d/include/libfsext.h.in)。
