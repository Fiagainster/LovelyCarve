// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';
import { createApp, nextTick, type App } from 'vue';
import BcpPanel from './BcpPanel.vue';
import type { BcpCatalog } from '../../../lib/ipc/bcp';

const rows = vi.hoisted(() => vi.fn());
vi.mock('../../../lib/ipc/bcp', () => ({ bcpRows: rows }));
let app: App | undefined;
afterEach(() => { app?.unmount(); document.body.replaceChildren(); });
async function settle() { for (let index = 0; index < 12; index++) await nextTick(); }

it('opens shared record details with every field and source, switches rows and closes without losing the table', async () => {
  const catalog: BcpCatalog = {
    path: 'sample.zip', totalRows: 2, totalFiles: 1,
    datasets: [{ code: 'TEST', encoding: 'UTF-8', rows: 2, files: [{ name: 'sample.bcp', rows: 2 }], fields: [
      { key: 'target', name: 'COLLECT_TARGET_ID', label: '采集对象' },
      { key: 'content', name: 'CONTENT', label: '内容' },
    ] }],
  };
  rows.mockResolvedValue({ rows: [
    { number: 1, fileName: 'sample.bcp', values: ['target-1', '完整内容\n第二行', '附加值'] },
    { number: 2, fileName: 'sample.bcp', values: ['target-2', '下一条记录'] },
  ], nextCursor: null });
  const host = document.createElement('div');
  document.body.append(host);
  app = createApp(BcpPanel, { imageId: 'image', catalog, catalogError: '', preferredFile: '', category: '' });
  app.mount(host);
  await settle();
  expect(host.querySelector('.detail-dock')).toBeNull();
  (host.querySelector('tbody tr') as HTMLElement).click();
  await settle();
  expect([...host.querySelectorAll('.file-detail-fields .field-value')].map(element => element.textContent)).toEqual([
    'sample.bcp', 'TEST', 'target-1', '完整内容\n第二行', '附加值',
  ]);
  (host.querySelectorAll('tbody tr')[1] as HTMLElement).click();
  await settle();
  expect(host.querySelector('.file-summary h3')?.textContent).toBe('记录 #2');
  expect(host.querySelector('.detail-dock')?.textContent).not.toContain('附加值');
  (host.querySelector('[aria-label="关闭详情"]') as HTMLButtonElement).click();
  await settle();
  await vi.waitFor(() => expect(host.querySelector('.detail-dock')).toBeNull());
  expect(host.querySelectorAll('tbody tr')).toHaveLength(2);
});
