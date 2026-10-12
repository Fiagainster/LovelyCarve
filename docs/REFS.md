# ReFS 读取器与验证


## 当前结果

LovelyCarve 的正式 ReFS 适配器已切换到本地 `refs-reader` Rust crate。
默认桌面、CLI、只读挂载和快照构建不加载或分发 `librefs-13.dll`。
保留旧 DLL 和 FFI 源码作为显式 `refs-dll-oracle` 功能下的对照工具；
正式 reader 不会自动回退到 DLL。沿用现有 `refs:` / `refs-snap:` 节点 ID 和 IPC。
这次完成的是应用使用的只读能力移植，没有另外发布兼容 C ABI 的 Rust DLL。

实现包括分区相对精确读取、卷头、superblock / checkpoint、碎片化元数据节点、
虚拟块映射、object table、递归目录、hardlink、驻留及非驻留数据、extent 子树、
索引及外置属性列表、稀疏和未写入区间、ADS、文件流快照和 CoW 共享块。
普通文件和快照流均直接按映射随机读取；流式读取验证完整长度。
目录缓存限制为 128 个目录 / 16 MiB，随卷实例释放。快照扫描在挂载、元数据读取
和目录项间检查调用方的 CancellationToken，没有另起轮询线程。
Reparse 链接保留目标，文件读取返回 UTF-8 目标文本；FILETIME 保留 100 ns 精度，
长记录按磁盘 MACB 顺序解码，hardlink 数据解析保留文件名记录的时间。

## 现成 Rust 库调查

| 候选 | 来源 | 相同镜像上的结果 |
| --- | --- | --- |
| `refs-core` 0.1.0 | [SecurityRonin/refs-forensic](https://github.com/SecurityRonin/refs-forensic) | 4 KiB / 64 KiB 镜像均找不到根 object 0x600；发布版未实现文件内容提取 |
| Rust `librefs` 0.1.0 | [发布文档](https://docs.rs/librefs/0.1.0/librefs/) | 4 KiB 根目录报 `volume corrupt: key u64`；64 KiB 挂载报 `hash_type out of range (value=4)` |
| `AlanIWBFT/refs-linux` | [源码](https://github.com/AlanIWBFT/refs-linux) | Rust Linux 内核模块，依赖 kernel API 和 C VFS shim；未作为跨平台用户态 reader 接入 |

`librefs` 发布包的仓库地址 ReFS-Linux/librefs 调查时返回 404，实际使用 crates.io
原始包，VCS revision 为 `a8a89b1a09bcf0c4b91b0b2970c5a88562c297ba`。
其 B+tree reader 固定读取 4096 字节页，snapshot manager 的内存列表不能证明
已有磁盘快照读取能力。两个候选仅为探针的开发依赖，不进入生产 ReFS 路径。

## 移植来源

以 [b3nguang/refsprogs 的 DLL 分支](https://github.com/b3nguang/refsprogs/tree/feature/build-librefs-dll)
commit `391c1bd56986142fc359288b7d2cb41cb3eb3680` 为来源，包含 Erik Larsson 的
refsprogs 解析与命名流扩展。新 crate 保留 GPL-2.0-or-later LICENSE 和 ORIGIN.md。
读取通道直接使用 DiskImage，可复用容器及解密后的证据字节，无需中间 RAW 导出。

## 公开镜像

来源：[xbqt/forefst 的公开样本](https://github.com/xbqt/forefst/tree/d4d5fd28942eb14ebe11d90f4eb9342f5e3d7b92/analysis/samples/disks)。
所有下载固定到 commit `d4d5fd28942eb14ebe11d90f4eb9342f5e3d7b92`，压缩包哈希
与该 commit 的 Git LFS pointer 一致，保存的是实际内容。

| 镜像 | ReFS / cluster / metadata checksum | RAW 逻辑大小 | 压缩包 SHA-256 |
| --- | --- | --- | --- |
| win11refstestmftecmd | 3.14 / 4 KiB / CRC64 | 4,294,967,296 | dc4fb09a134a1b16d289e192d6a9dbc8a5e6779f278735ff78372b4cf259cfb1 |
| wininsiderrefs8gtest2 | 3.14 / 64 KiB / SHA-256 | 8,589,934,592 | 274d0387656a1f6e36e1128e60d9baa0f71bc37d511914cc68f238323ea61d64 |
| win11refs2tsnapshots | 3.14 / 4 KiB / CRC64 | 2,199,023,255,552 | 36d6a02992f333af5fba2c2663688c6d9e7add124412cd317b6dbf663fb15913 |

镜像、第三方验证脚本和本地 JSON 报告放在忽略的 `fixtures/refs-experiment/`，不提交。
4 GiB RAW SHA-256 为 `f7392dc51e9228f292efb10152181781fc424a0a64baf1c8a5c3d9d71eee400e`；
8 GiB RAW 为 `246d3c2f969ff6a67d6a9879723de50c20152b0e11d61034e38c4c7851c24ffc`。
2 TiB 镜像完整流式解压到 Windows 稀疏文件，逻辑长度与 zstd frame 一致；只跳过
确认全零的 32 MiB 块，非零块实际写入约 608 MiB。没有截取或伪造卷内数据，
未对完整 2 TiB RAW 另算哈希。

下载（用表中名称替换 `<name>`，2 TiB 镜像应使用支持稀疏输出的解压器）：

```powershell
curl.exe -fL "https://media.githubusercontent.com/media/xbqt/forefst/d4d5fd28942eb14ebe11d90f4eb9342f5e3d7b92/analysis/samples/disks/<name>/<name>.raw.zst" -o "fixtures/refs-experiment/<name>.raw.zst"
Get-FileHash "fixtures/refs-experiment/<name>.raw.zst" -Algorithm SHA256
zstd -d --sparse "fixtures/refs-experiment/<name>.raw.zst"
```

## 真实内容验证

| 镜像 | 目录项 / 遍历目录（含根） | 命名流 | 独立解析器验证的普通非链接文件 |
| --- | --- | --- | --- |
| 4 KiB | 1744 / 238 | 675 | 1063 个，长度和 SHA-256 全部一致 |
| 64 KiB | 894 / 119 | 245 | 529 个，长度和 SHA-256 全部一致 |
| 2 TiB 快照 | 304 / 35 | 13，其中 10 个快照 | 267 个，长度和 SHA-256 全部一致 |

Rust 完整 corpus 遍历和内容读取均无失败。全部 2,942 个目录项的文件名、目录身份、非链接大小、MACB 时间和链接目标
与发布 CSV 一致；链接大小采用 UTF-8 目标文本长度，与原始 DATA 大小区分。
快照镜像的 5 个文件共 10 个历史版本另用 forefst 独立恢复，长度和 SHA-256 全部一致，
覆盖 5 字节到 3,202,036 字节。另有 923 个 ADS 的完整长度和 SHA-256 与 forefst 一致。独立解析器固定为同一 commit，Python 代码只在忽略
目录运行，没有复制到生产 crate。extent 的 WRITTEN / HOLE 标志、MACB 时间字段和 CoW 共享块的
格式解释参考该项目的磁盘结构分析，并以真实字节和独立哈希验证。

DLL 在 64 KiB 镜像上无法挂载。4 KiB 全树对照只能列出 599 项 / 73 个目录，
有 10 项目录、链接或短读错误；可成功读取的文件用于对照，不把 DLL 的失败视为通过。
149 MiB 预分配文件揭示 DLL 未处理未写入 extent：旧哈希为
`8c63cbc67d8ff5481e01ea2f42ec770669239811932e294a8b286a879845a269`，
Rust 与独立解析器一致的哈希为
`c7d96f97bfab0380aa99d8c0c8ffdec69715d637febcc74e5e83cc8292ecb664`。
没有为了匹配旧 DLL 而输出未写入区间里的旧数据。

当前 4 KiB checkpoint 为物理 block 123892 / sequence 617；64 KiB 为 15637 / 161；
快照镜像为 5368580 / 265。偏移表按磁盘字段定位，修正了旧 C 实现中 SHA-256 引用
布局与物理 checkpoint 地址的问题。

验证范围是这三份 ReFS 3.14 公共镜像和结构单元测试。未声称覆盖所有 ReFS 版本、
损坏卷恢复、数据完整性校验、写入或日志重放；没有原生 Windows 文件导出 oracle，
独立验证来源为 forefst。元数据结构与引用有边界检查，读取成功不等于 CRC/SHA 校验已认证。

## 复现

```powershell
just test-refs
just clippy-refs
just probe-refs D:/LovelyCarve/fixtures/refs-experiment/win11refstestmftecmd.raw --backend ported --walk --output D:/LovelyCarve/fixtures/refs-experiment/new-4k.json
just probe-refs D:/LovelyCarve/fixtures/refs-experiment/wininsiderrefs8gtest2.raw --backend ported --walk --output D:/LovelyCarve/fixtures/refs-experiment/new-64k.json
just probe-refs D:/LovelyCarve/fixtures/refs-experiment/win11refs2tsnapshots.raw --backend ported --walk --file /testsnapshots/test.txt --output D:/LovelyCarve/fixtures/refs-experiment/new-snapshots.json
$env:REFS_RUST_IMAGE='D:/LovelyCarve/fixtures/refs-experiment/win11refstestmftecmd.raw'
just test --jobs 1 --lib production_reader_traverses_streams_and_seeks_without_the_dll -- --ignored
$env:REFS_RUST_SNAPSHOT_IMAGE='D:/LovelyCarve/fixtures/refs-experiment/win11refs2tsnapshots.raw'
just test --jobs 1 --lib production_snapshot_streams_keep_versions_ranges_and_partial_output_contracts -- --ignored
$env:REFS_ORACLE_IMAGE='D:/LovelyCarve/fixtures/refs-experiment/win11refstestmftecmd.raw'
just test-refs-oracle --jobs 1 -- --ignored
```

探针路径应使用绝对路径（recipe 在 src-tauri 下运行）；`--output` 使用 create_new，
复跑选新报告名。`--backend all|dll|refs-core|librefs|ported` 默认 all；候选失败保留
原始错误并返回非零，ported 完整读取要验证计数等于声明大小。

## 本地验证结果

- `just fmt`、`just check`、`just clippy`、`just check-examples` 通过。Clippy 保留
  原代码的 11 条提示，reader 自身无 Clippy 提示；binrw 有上游 future-incompat 提示。
- `just test --jobs 1`：912 项库测试通过，11 项按设置忽略；mount CLI 与
  CLI module resolution 各 2 项通过。
- `just test-refs`：16 项结构、extent、短读、稀疏、版本隔离、流描述符与时间测试通过。
  `just clippy-refs` 通过。
- `just test-refs-oracle --jobs 1`：2 项边界测试通过；显式 `--ignored` 的真实
  4 KiB DLL 对照通过，并断言未写入区间采用独立解析器确认的正确结果。
- 不启用 DLL feature 的应用层真实镜像测试通过：4 KiB / 64 KiB 普通文件的完整
  哈希、随机范围和 EOF；2 TiB 镜像的 10 个快照版本哈希、refs-snap 范围读取、
  快照行、条数截断和取消清理。三份镜像的 `.dirz` 与实时目录 JSON 等价测试通过。
- `just test-cli` 的编译与 CLI smoke 通过，覆盖目录、读取、哈希、导出、雕刻、
  搜索、时间线、案件生命周期、关闭与中断。打包脚本 PowerShell 语法检查通过，
  Tauri 资源表与两种便携打包路径均已排除 librefs DLL。

本机 Rust 1.99 在整库代码生成时多次发生 `STATUS_ACCESS_VIOLATION`。
最终全量测试与 CLI 编译通过时使用下列进程环境参数；它们也可用于复现：

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_INCREMENTAL='0'
$env:RUST_MIN_STACK='33554432'
just test --jobs 1
just test-cli
```

未改前端与 IPC，未运行前端测试、发行包打包或 macOS / Linux 运行验证。
