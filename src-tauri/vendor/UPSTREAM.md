# Vendored upstream audit

核对日期：2026-10-12。核对上游默认分支、已声明的来源分支和 crates.io 发布版本。

更新保留 LovelyCarve 的只读接口、边界检查、线程保护、取消支持及独立测试夹具。
表中的提交是核对或选用的源码基准；本地移植/兼容补丁并不等于整个上游仓库。
同一版本的上游维护改动也按源码核对；不通过修改版本号伪造升级。

仓库只保留当前构建使用的依赖源码。移除随已删除分析插件失去用途的 EVTX 库，以及 vendor 中不在本仓库运行的上游 GitHub Actions；保留其余读取器的许可证、来源和回归样本。

| 目录 | 当前版本 | 上游提交 / 来源 | 本次处理 |
| --- | --- | --- | --- |
| `ad1` | `0.1.2` | [afc3963659a1](https://github.com/SecurityRonin/ad1-forensic/commit/afc3963659a13acf10082e7d76194d3ab789583e) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `apfs` | `0.4.1` | [3a73ee28fdab](https://github.com/Dil4rd/dpp/commit/3a73ee28fdab192fcdfbe8edcad7b520867f2417) | 升级 0.2.4 → 0.4.1；保留多卷、MACB 时间、可取消的批量目录遍历与压缩流接口。 |
| `backhand` | `0.25.5` | [bf678c7f9237](https://github.com/wcampbell0x2a/backhand/commit/bf678c7f923799a57d972cf695daf69d8c6ddafb) | 升级 0.25.1 → 0.25.5；保留精简特性、std I/O 别名及可读错误信息，修复 Windows 根路径判断。 |
| `bitlocker` | `0.3.5` | [e6674dcda241](https://github.com/SecurityRonin/bitlocker-forensic/commit/e6674dcda2413b1a02a915f2255af8fda787a154) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `btrfs-diskformat` | `0.5.1` | [bb84745422ed](https://github.com/GodTamIt/btrfs-diskformat/commit/bb84745422ed2c2aa99b3b151f6247a3b51638f4) | 已最新；保留 zerocopy Immutable 兼容补丁。 |
| `cdfs` | `0.2.3` | [8cc434a31983](https://git.sr.ht/~az1/iso9660-rs/commit/8cc434a319832ae43d1c7685477809d75f313990) | 发布版本及核心解析器已最新；保留现有独立 crate 布局。 |
| `cmpfs` | `0.1.0` | [3a73ee28fdab](https://github.com/Dil4rd/dpp/commit/3a73ee28fdab192fcdfbe8edcad7b520867f2417) | 新增上游共享 Apple decmpfs 解码器，供 APFS/HFS+ 使用。 |
| `ewf` | `0.4.10` | [c456f993f683](https://github.com/SecurityRonin/ewf-forensic/commit/c456f993f68385f3438b3ff16ba5ab6fd0850fb4) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `exfat` | `0.1.0` | [b930023e0636](https://github.com/obhq/exfat/commit/b930023e06363371b1f26df1f45e17eaa0c7a290) | 保留删除项修复；本地 VDL 尾部、严格簇链、原始属性/extent、UTF-16 与时间补丁见 [FAT](../../docs/FAT.md)。 |
| `ext4-view` | `1.0.0` | [8facbf4c2986](https://github.com/nicholasbishop/ext4-view-rs/commit/8facbf4c2986b7e2dbdef92514d696f1559a2b90) | 升级 0.9.3 → 1.0.0；保留 inode 索引 API、128 字节 inode 校验和、兼容旧 journal；本地内联数据、EA-inode、meta_bg 等能力补丁见 [EXT](../../docs/EXT.md)。 |
| `fatfs` | `0.4.0` | [2aefc2a027ce](https://github.com/rafalh/rust-fatfs/commit/2aefc2a027ce94ed0671752814dac203f0450e11) | 保留显式 root_dir 生命周期；本地目录项偏移、簇链防环、有限 extent 与大扇区补丁见 [FAT](../../docs/FAT.md)。 |
| `gpt` | `4.1.0` | [73086deff58c](https://github.com/Quyzi/gpt/commit/73086deff58cb3c741b2ba51a53a2a7c6231a3f1) | 同步上游整数边界测试修正；保留精简测试依赖。 |
| `hfsplus` | `0.4.0` | [3a73ee28fdab](https://github.com/Dil4rd/dpp/commit/3a73ee28fdab192fcdfbe8edcad7b520867f2417) | 升级 0.2.4 → 0.4.0；保留 attribute/access 时间和权限标志。 |
| `ios-backup` | `0.1.0` | [8d8cef447ae2](https://github.com/SecurityRonin/ios-backup-forensic/commit/8d8cef447ae212e7dd22dc0425156bf4ddfcfa05) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `luks` | `0.1.8` | [284f06487d7a](https://github.com/SecurityRonin/luks-forensic/commit/284f06487d7a66f13ed27c61be5e26f0863c0e4f) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `magika` | `1.1.1-dev` | [26b6a9ba7e92](https://github.com/google/magika/commit/26b6a9ba7e92f2b0a3745970a9190ec0dde9bf83) | 同步最后的 ORT 兼容源码（RAR MIME 修正）；上游 2.0.0-dev 已迁至 tract 且未发布，本次不迁移推理运行时，也不降级至 1.1.0。 |
| `notatin` | `1.0.1` | [ae3a70611bc3](https://github.com/strozfriedberg/notatin/commit/ae3a70611bc37b2ca04637671ceb14ccab9d5a87) | 同步默认分支的 nom 8 / thiserror 2 解析器和版权更新；保留独立 crate 布局。 |
| `ntfs` | `0.4.0` | [47ae0168f6c6](https://github.com/ColinFinck/ntfs/commit/47ae0168f6c607208b27c61ed89b4ffac9a3c99a) | 默认分支运行时代码已最新；保留损坏 attribute-list 迭代终止修复。 |
| `qcow2` | `0.1.3-lovelycarve` | [3ed3ff55b96a](https://github.com/vasi/qcow2-rs/commit/3ed3ff55b96a2f519216ace7b3a02ac29bf8ba65) | 上游代码未更新；保留 v2 支持、L1 缓存和边界检查。 |
| `realm-db-reader` | `0.2.1` | [e6c90c65c04b](https://github.com/maartenstaa/realm-db-reader/commit/e6c90c65c04b1e3fcfef796571a664bfa98e2f5b) | 发布版本和运行时代码已最新；保留内存读取及空表补丁。 |
| `refs-reader` | `0.1.0` | [391c1bd56986](https://github.com/b3nguang/refsprogs/commit/391c1bd56986142fc359288b7d2cb41cb3eb3680) | 核对 feature/build-librefs-dll；指定来源分支未变化。Rust 移植不能直接覆盖为 C 源码。 |
| `rsqlite-vfs` | `0.2.0` | [3d8318430bac](https://github.com/Spxg/sqlite-wasm-rs/commit/3d8318430bacd7d6a1aafdf5b5e54fb0fee8d451) | 升级 0.1.1 → 0.2.0；文件表、文件内容和错误槽使用 Arc/Mutex，注册操作串行化；应用改用新导入 API。 |
| `rufs` | `0.6.0` | [36bc35a5fb9e](https://github.com/realchonk/fuse-ufs/commit/36bc35a5fb9e05e214fa3d9619822763d7d8cb9b) | 同步 bincode-next 3.1.1 和 MSRV 1.90；保留 Windows 只读移植和解码器 API。 |
| `sqlite-core` | `0.12.0` | [df511309e086](https://github.com/SecurityRonin/sqlite-forensic/commit/df511309e086825fb194ea8a721e94bbd0033fe0) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `udif` | `0.4.2` | [3a73ee28fdab](https://github.com/Dil4rd/dpp/commit/3a73ee28fdab192fcdfbe8edcad7b520867f2417) | 升级 0.3.4 → 0.4.2；采用上游有边界的块解码及纯 Rust XZ。 |
| `vdi` | `0.2.0` | [f1f88806c260](https://github.com/cohaereo/vdi-rs/commit/f1f88806c26042de342d1fbcb7f286590686e2fc) | 发布版本已最新；保留稀疏块、边界检查和 positioned I/O 重写。 |
| `veracrypt` | `0.2.4` | [e3883a5b347f](https://github.com/SecurityRonin/veracrypt-forensic/commit/e3883a5b347f50b51de70ca08655b2fd4b0885b2) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `vhd` | `0.1.4` | [2835523fe39c](https://github.com/SecurityRonin/vhd-forensic/commit/2835523fe39c16740133dc8d51932cad46bcc4b6) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `vhdx` | `0.4.1` | [b03f1f5c1be9](https://github.com/SecurityRonin/vhdx-forensic/commit/b03f1f5c1be960d79af73f895017169171c00996) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `vmdk` | `0.8.4` | [8a9831125c6d](https://github.com/SecurityRonin/vmdk-forensic/commit/8a9831125c6db5cff9a24414481037487a92d410) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `vsc` | `0.2.2` | [0af70bed6910](https://github.com/SecurityRonin/vsc-forensic/commit/0af70bed6910fef542224ecd94928b4bc0ae920b) | 来源记录与上游最新提交一致，保留现有本地补丁。 |
| `xfs-fuse` | `0.6.0` | [2ea4e250db00](https://github.com/KhaledEmaraDev/xfuse/commit/2ea4e250db00d3f0f845499c8a83f85b9a77b7f8) | 合入上游 v1 inode / real-time 标记；本地 XFS 1–5、目录、属性、checked extent/范围读取、纳秒时间与 meta UUID 补丁见 [XFS](../../docs/XFS.md)。无独立 real-time 设备时拒绝正文。未引入上游 FUSE 服务或 libfsxfs C 实现。 |
| `zfs` | `0.1.0` | [原来源](https://github.com/Hopffenn/zfs-rs) | 原来源返回 Repository not found；保留现有稳定 Rust 移植，不能声称已同步无法访问的上游。 |

`leveldb-rs-binding` 是已移除 LevelDB/MMKV 功能后遗留的空目录，没有被 Git 跟踪的源码或 Cargo 依赖，不属于待更新库。

## 验证入口

- 后端：`just fmt`、`just check`、`just clippy`、`just test --jobs 1`。Windows 页面文件不足时避免并行编译全部示例。
- 独立库：`just test-vendor <目录名>`；已有专用 reader 配方仍优先使用。
- ext4 原始夹具来自上游并校验 SHA-256，详见 `ext4-view/test_data/README.md`；测试通过 Rust zstd 在内存解码，不依赖系统 zstd 程序。
- 可选外部镜像/oracle 未提供时，忽略的测试不代表真实镜像已验证。
