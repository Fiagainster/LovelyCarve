# BitLocker 与 libbde 能力对照

核对日期：2026-10-13。基准为 [libyal/libbde](https://github.com/libyal/libbde/tree/dd48d476e3da4a19d7bf1c4d16d14b3b6efeddf0)
提交 `dd48d476e3da4a19d7bf1c4d16d14b3b6efeddf0`。比较了 README、公共头文件、格式文档、
`volume_header`、`metadata_block_header`、`metadata`、`sector_data`、`encryption_context` 和 EOW 读取路径。
不能把 libbde 自己不支持的 TPM/SID 解锁列作本项目相对缺口。

生产实现仍为 Apache-2.0 的 `bitlocker-core` 0.3.5 加本地补丁，直接读取解码后的检材分区，
不加载 libbde DLL，不复制 libbde 的 C 实现。原始镜像和分区 ID、偏移、长度保持不变。

## 格式与密码算法

| 能力 | libbde | 修改前 | 当前实现与证据 |
| --- | --- | --- | --- |
| Vista / v1 | 支持；前 8192 字节明文，修复 NTFS 签名和 MFT 镜像簇 | 能辨认头，但按 v2 读取，未修复启动区 | 按 v1 解释块头；恢复 NTFS/MFT mirror；结构测试及独立 CBC 数据扇区 |
| Windows 7+ / v2 | 支持 | 支持 | 保留；真实 dfVFS 镜像对照 |
| BitLocker To Go | 支持 | 支持 | 保留；真实 dfVFS 镜像对照 |
| Used Disk Space Only / EOW | 支持位图，0 位直接读明文 | 未读取 EOW，可能把明文解密为垃圾 | 专用 `eow.rs`；读取 descriptor/map/双 record，选较新 sequence，按 relocation block 位图决定解密；结构测试 |
| 部分加密卷 | encrypted-volume-size 之后读原字节 | 已有边界路径 | 保留；独立向量验证边界与 raw 尾部 |
| AES-128-CBC + Diffuser / 8000 | 支持 | 支持 | 真实 pybde 对照及独立 Python 向量 |
| AES-256-CBC + Diffuser / 8001 | 支持 | 明确拒绝 | 新增 256 位 FVEK/TWEAK 与 AES-256 sector-key；独立 Python 向量 |
| AES-128/256-CBC / 8002、8003 | 支持 | 支持 | 独立 Python 向量；原外部 oracle 保留 |
| XTS-AES-128/256 / 8004、8005 | 支持 | 支持，tweak 固定按 512 字节除 | 按实际逻辑扇区长度生成 tweak；独立 cryptography 向量 |
| 512 / 4096 字节逻辑扇区 | 按 BPB / 元数据的扇区大小 | 应用只允许 512 | 十二组已提交独立向量覆盖全部六种方法的 512 / 4096；也接受 1024 / 2048，尚无真实样本验证 |
| 未加密方法 0000 | 支持直接读取 | 解锁阶段被算法门禁拒绝 | 不要求保护器；保留启动区/元数据映射 |

## 解锁方式

| 能力 | libbde | 修改前 | 当前入口 |
| --- | --- | --- | --- |
| 用户密码 | 支持 | 已支持 | `bitlocker_unlock.password`，尝试每个同类保护器，AES-CCM 认证 |
| 48 位恢复密码 | 支持 | 已支持 | `recoveryPassword`；格式、范围、除 11 校验及 AES-CCM 认证 |
| 明文密钥 / 暂停保护 | 支持 | 库已有，应用未接通 | 不传其他凭据即尝试 clear protector；界面「其他密钥 → 明文密钥」 |
| BEK 启动密钥 | 支持 | 库已有，应用未接通 | `startupKeyPath`；64 KiB 上限；匹配 BEK/保护器 GUID 并认证 VMK/FVEK |
| 原始 FVEK / TWEAK | 支持 `set_keys` | 未提供 | `fvekHex` / `tweakHex`；严格校验算法对应长度；界面可输入 |
| 原始 VMK | 无同名公共解锁入口，保护器链内处理 | 必须先从 VMK 提取恢复密码 | 32 字节 VMK 直接认证 FVEK，不要求恢复密码保护器；`bitlocker_unlock_with_vmk` |
| 内存 VMK 扫描、提取恢复密码 | 不属于 libbde 公共扫描接口 | 已支持 | 保留。16 字节旧候选仍走既有恢复 datum 路径；32 字节 VMK 可直接解锁 |
| TPM / SID | README 明确不支持 | 不支持 | 仍不支持；元数据查询可以报告相应保护器类型 |

一个请求只选一种凭据。TWEAK 只能与 FVEK 一起提交。密码、恢复密码、BEK 和 VMK 的错误密钥
通过认证失败；原始 FVEK/TWEAK 的扇区加密本身没有认证标签，长度正确不代表密钥正确，须核验
解密后的文件系统/文件。界面有对应提示。凭据不持久化。

VMK 解锁结果保留 `{ status, recovery_key }` 契约；没有可提取的恢复密码时 `recovery_key` 为 `""`，
不会把已通过 FVEK 认证的解锁标成失败。

## 字节读取、元数据与工具/API

| libbde 能力/API | 本项目对应实现/差异 |
| --- | --- |
| signature check、open、offset/file I/O handle | 检材解码器 + 有界 `VolumeReader` + `VolumeHeader`；RAW/EWF/VMDK 等共用读取器 |
| read、seek、read_buffer_at_offset、get_offset | `DecryptedVolume` 的 `Read + Seek` / `read_at`；应用 `UnlockedVolume` 用 Mutex 串行保护，弱引用避免延长检材生命周期 |
| metadata 三份副本 | 完整结构校验后选择有效副本；坏偏移、截断/畸形块与读取失败可继续备份；Vista 从块头补充备份偏移，最多六个去重候选 |
| metadata 清零 | v1 每份 16 KiB，v2 每份 64 KiB。修复原来只覆盖 entry 内容长度的问题 |
| relocated volume header | 逻辑启动区读取重定位字节并按物理偏移生成 IV；原重定位存储区在解密视图清零 |
| EOW metadata 清零 | descriptor、map 和 relocation log 保留区清零；明文位图作用于物理扇区地址 |
| get_size | 使用传入的检材分区边界；不会将容器总文件长度或解密偏移冒充原分区几何。此契约不等同于 libbde 从 NTFS 头推导卷大小 |
| get_encryption_method / is_locked | `bitlocker_metadata` / `bitlocker_status` |
| get_volume_identifier / creation_time | `bitlocker_metadata.volume_guid` / `creation_time`；后者为原始 FILETIME 的十进制字符串，防止 JavaScript 丢低位 |
| UTF-8 / UTF-16 description | `description` 由 UTF-16LE 严格解码成 Unicode 字符串；非法编码报错 |
| key-protector count / identifier / type | `key_protectors[]` 和 `protector_types[]`，不暴露密钥 payload |
| signal_abort | 既有任务管理与取消；密钥派生前后检查，派生中不保证立即响应；内存扫描和导出支持取消 |
| bdeinfo | `bitlocker_metadata`（无凭据）、界面格式/保护器摘要；全部 metadata 信息可通过 CLI JSONL 获取 |
| bdeexport | `bitlocker_export_decrypted_image` 输出完整 RAW 镜像，将目标分区替换为解密字节；这是既有完整镜像契约，非单卷 bdeexport 字节布局 |
| bdemount / Dokan / FUSE | 复用应用已有只读文件系统挂载/共享路径；不增加 libbde/Dokan/FUSE 兼容 API |
| C / Python bindings、library version/error helpers | 对应 Rust 类型/API、typed IPC、生成的 CLI 命令。不是二进制 ABI 或 pybde 兼容替代品 |

越界、溢出、截断密文均报错，不能补假密文再解密。检材坏扇区被容错源合成时，BitLocker 读取
和解密导出会明确失败，原坏区审计仍可通过 `image_bad_regions` 获取。格式定义的元数据清零、
未加密区和逻辑映射与坏扇区补零是不同情况。导出使用 `create_new`，不覆盖已有文件；短读不能
报告完成。取消/失败仍可能留下部分导出文件，沿用已有导出契约。

## 验证与限制

```powershell
just test-bitlocker
just clippy-bitlocker -- -D warnings
just test bde_image --lib
just verify-fe
just check
just clippy
just test-cli
```

2026-10-13 核验完整 dfVFS `bdetogo.raw`：67,108,864 字节、MD5
`fcba22f9363388101ae66c741588bc45`，设置 `BDE_ORACLE_IMAGE` 后执行了现有 pybde ground-truth
扇区对照。提交的精简投影只能验证保留的原始扇区，不能用其完整导出 hash 代表真实检材。

`tests/data/parity-*.bin` 是 PyCryptodome + cryptography 独立生成的紧凑投影；涵盖六种方法 ×
两种扇区大小、认证解包、随机/顺序/未对齐读取、启动区映射、部分加密尾部和原始密钥。
`parity.rs` 另覆盖 Vista、metadata fallback、EOW 新旧 record 与异常位图，CLI 覆盖新增凭据和导出。
生成器、SHA-256 清单、来源与许可随样本保存。

新增 8001、4 KiB、Vista 和 EOW **尚未拿 Windows 实机发行的完整对应镜像做独立 libbde/pybde
全卷对照**。Python 向量证明密码算法互操作，结构测试证明已知布局与边界；两者不能替代真实
检材覆盖。原有其他 env-gated 外部 oracle 在未设置环境变量时跳过，不算样本已验证。
EOW 和 FVE 元数据 checksum 尚未认证；EOW 读取静态快照，不重放正在变化的 Windows 写入事务，
同序号双 record、畸形布局和超出资源上限会拒绝。TPM/SID、强制解密未验证扇区和写回检材均无入口。

## 参考依据

- [libbde README](https://github.com/libyal/libbde/blob/dd48d476e3da4a19d7bf1c4d16d14b3b6efeddf0/README)
- [libbde 公共 API](https://github.com/libyal/libbde/blob/dd48d476e3da4a19d7bf1c4d16d14b3b6efeddf0/include/libbde.h.in)
- [BDE 格式文档](https://github.com/libyal/libbde/blob/dd48d476e3da4a19d7bf1c4d16d14b3b6efeddf0/documentation/BitLocker%20Drive%20Encryption%20%28BDE%29%20format.asciidoc)
- [sector_data 读取规则](https://github.com/libyal/libbde/blob/dd48d476e3da4a19d7bf1c4d16d14b3b6efeddf0/libbde/libbde_sector_data.c)
- [EOW 结构定义](https://github.com/libyal/libbde/blob/dd48d476e3da4a19d7bf1c4d16d14b3b6efeddf0/libbde/bde_eow.h)
- [dfVFS 公开样本](https://github.com/log2timeline/dfvfs/tree/main/test_data)
