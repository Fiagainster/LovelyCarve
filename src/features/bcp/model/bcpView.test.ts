import { describe, expect, it } from 'vitest';
import { bcpNavGroup } from '../../../app/workspace/bcpNavigation';
import { bcpDatasetLabel } from './bcpView';
import type { BcpCatalog, BcpDataset } from '../../../lib/ipc/bcp';

function dataset(code: string, rows: number): BcpDataset {
  return { code: `WA_MFORENSICS_${code}`, rows, encoding: 'UTF-8', fields: [], files: [] };
}

describe('BCP navigation', () => {
  it('groups record counts by dataset category', () => {
    const catalog: BcpCatalog = {
      path: 'D:/evidence', totalRows: 29, totalFiles: 3,
      datasets: [dataset('010700', 10), dataset('010600', 7), dataset('020600', 12)],
    };
    const group = bcpNavGroup(catalog);
    expect(group.items.map(item => [item.id, item.label, item.badge])).toEqual([
      ['bcp', '全部数据', null],
      ['bcp:01', '设备与通信', 17],
      ['bcp:02', '即时通信', 12],
    ]);
    expect(bcpDatasetLabel(catalog.datasets[0]!)).toBe('短信');
  });
});
