# LovelyCarve integration

APFS 0.4.1 from Dil4rd/dpp at 3a73ee28fdab192fcdfbe8edcad7b520867f2417 (MIT).

Preserved local APIs: open_index/list_volumes and indexed VolumeInfo, access/change times, cancellable streaming catalog walk and OMAP cache, and boxed streaming reads for compressed files and symlinks. The walk validates object checksums like upstream lookups. The old local decmpfs/xattr modules are replaced by upstream cmpfs and catalog attribute parsing.

Upstream audit and verification commands: [../UPSTREAM.md](../UPSTREAM.md).
