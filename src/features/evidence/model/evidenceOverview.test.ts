import { describe, expect, it } from 'vitest';
import type { ImageInfo } from '../../../lib/ipc/image';
import { evidenceDisplayName, evidenceFacts, evidenceHashVerdict, evidenceLayout, formatBytes, partColor, partitionGeometry, storedHashRows } from './evidenceOverview';

const info: ImageInfo = { id: 'a', path: 'a', source_format: 'raw', size_bytes: 1000, sector_size: 100, table_scheme: 'gpt', disk_id: null, on_disk_bytes: null, created_unix_ms: null, modified_unix_ms: null, ewf: null, partitions: [
  { id: 1, kind: 'ntfs', label: '末尾', start_lba: 8, length_lba: 8 },
  { id: 2, kind: 'ext4', label: null, start_lba: 0, length_lba: 2 },
  { id: 3, kind: 'unknown', label: null, start_lba: 20, length_lba: 2 },
] };

describe('evidence overview projections', () => {
  it('clips the disk layout to image bytes while preserving the original partition colours', () => {
    expect(evidenceLayout(info)).toEqual([
      { key: 'part-2', leftPct: 0, widthPct: 20, lenBytes: 200, color: partColor(1), tooltip: '分区2 · Ext4 · 200 B', partition: info.partitions[1] },
      { key: 'part-1', leftPct: 80, widthPct: 20, lenBytes: 200, color: partColor(0), tooltip: '末尾 · NTFS · 200 B', partition: info.partitions[0] },
    ]);
    expect(evidenceLayout({ ...info, size_bytes: 0 })).toEqual([]);
  });
  it('retains binary capacity precision and inclusive sector geometry', () => {
    expect(formatBytes(1024 * 1024)).toBe('1.00 MiB'); expect(formatBytes(1023)).toBe('1023 B');
    expect(partitionGeometry(info.partitions[0]!, 100, '扇区')).toBe('扇区 8 – 15，共 8 扇区\n字节偏移 0x320');
  });
  it('names an automatic reconstruction without exposing its identity as the heading', () => {
    const evidence = { ...info, source_format: 'assembled', path: 'assembled://md:auto-raid5-59494dc69af24768b6a690819d35420e' };
    expect(evidenceDisplayName(evidence)).toBe('RAID 5 重组镜像');
    expect(evidenceFacts(evidence)).toContainEqual({ label: '重组镜像标识', value: evidence.path, mono: true, copyable: true });
    expect(evidenceDisplayName({ ...info, path: 'D:\\evidence\\disk.E01' })).toBe('disk.E01');
    expect(evidenceDisplayName({ ...evidence, path: 'assembled://md:custom-array' })).toBe('RAID 重组镜像');
  });
  it('normalizes recorded hashes and judges only algorithms with a computed result', () => {
    const evidence = { ...info, ewf: { stored_md5: ' AA BB ', stored_sha1: 'ccdd' } as NonNullable<ImageInfo['ewf']> };
    const rows = storedHashRows(evidence, { md5: 'aabb', sha1: null });
    expect(rows).toEqual([{ label: 'MD5', value: ' AA BB ', verdict: 'match' }, { label: 'SHA1', value: 'ccdd', verdict: 'pending' }]);
    expect(evidenceHashVerdict(rows)).toBe('match');
    expect(evidenceHashVerdict(storedHashRows(evidence, { md5: 'aabb', sha1: 'wrong' }))).toBe('mismatch');
    expect(evidenceHashVerdict(storedHashRows(evidence, undefined))).toBeNull();
  });
});
