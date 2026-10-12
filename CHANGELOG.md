# Changelog

## [Unreleased]

### Added

- 补齐 ext2/3/4 内联文件与目录、EA-inode 扩展属性、旧式目录和 meta_bg 读取，并记录完整 libfsext 能力对照。
- ext 读者提供卷挂载历史、feature masks、UID/GID、链接数和设备编号等元数据 API。

### Fixed

- ext 文件范围与流式读取统一完整间接块映射，未写入 extent 读零，越过卷边界或来源短读时报错。
- 修复 ext signed/扩展 epoch/纳秒时间戳及 birth 投影、32768-block extent 长度、64 KiB 目录记录和旧组描述符 CRC16 校验。
