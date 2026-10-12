# 后端探针与基准

源码按职责分类；Cargo.toml 显式登记全部 46 个 example，目标名称保持稳定。
使用仓库根 Justfile：`just run-example <name> [args]`，编译验证使用 `just check-examples`。

仅读取源字节、分区或 RAID 的探针使用 `evidence::image::DiskImage`；调用文件树、预览、索引、插件浏览或挂载的探针使用 `filesystem::evidence_session::EvidenceSession`。RAID 的成员仍为字节读者，选择或组装目标后再包装浏览会话。

## evidence

`probe_partitions`, `probe_raid`, `probe_raid_nometa`, `probe_physical_disk`, `probe_evidence_names`, `probe_evidence_path`, `probe_lvm_meta`

## filesystem

`fs_dispatch_smoke`, `probe_access_bitmap`, `probe_apfs`, `probe_btrfs`, `probe_cdfs`, `probe_docker`, `probe_docker_fs`, `probe_exfat`, `probe_ext4`, `probe_fat_deleted`, `probe_hfsplus`, `probe_lvm_active`, `probe_refs`, `probe_squashfs`, `probe_ufs`, `probe_xfs`, `probe_xfs_magic`, `probe_zfs`

## catalog

`benchmark_filedb_store`, `benchmark_flat_page`, `probe_btrfs_snapshot`, `probe_filedb`, `probe_refs_snapshot`, `probe_snapshot`

## analysis

`probe_ntfs_timeline`, `probe_report_sources`

## carving

`probe_carve`, `probe_carve_truth`

## search

`bench_global_search`, `keyword_scan`

## preview

`probe_pe`, `probe_vim_swap`, `qr_test`, `verify_csv_recovery`

## extensions

`probe_plugin`, `probe_plugin_sources`

## platform

`probe_dokan_docker`, `probe_dokan_mount`, `probe_dokan_slice`
