// @vitest-environment happy-dom
import { createApp, h, nextTick, type App } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ImageInfo, IosBackupInfo } from '../../../lib/ipc/image';
import IosBackupCard from './IosBackupCard.vue';

const rpc = vi.hoisted(() => ({ unlock: vi.fn(), audit: vi.fn(), export: vi.fn() }));
vi.mock('../../../lib/ipc/iosBackup', () => ({ iosBackupUnlock: rpc.unlock, iosBackupAudit: rpc.audit }));
vi.mock('../../../lib/ipc/image', () => ({ imageUnload: vi.fn() }));
vi.mock('../../../lib/ipc/taskRuntime', () => ({ taskCancel: vi.fn() }));
vi.mock('../../../composables/export', () => ({ saveTextWithDialog: rpc.export }));
vi.mock('../../../composables/useToast', () => ({ notifyError: vi.fn() }));
const metadata: IosBackupInfo = {
  locked: true, declared_encrypted: true, observed_encrypted: null,
  device_name: '测试 iPhone', product_type: 'iPhone13,3', product_version: '18.0',
  serial_number: 'SERIAL', unique_identifier: 'UDID', snapshot_state: 'finished',
  file_count: null, unreadable_rows: null, unresolved_sizes: 0,
};
let app: App | undefined;
let host: HTMLDivElement;
async function settle() { for (let index = 0; index < 5; index += 1) await nextTick(); }
async function mount(ios: IosBackupInfo) {
  host = document.createElement('div'); document.body.append(host);
  app = createApp({ render: () => h(IosBackupCard, { info: { id: 'a', path: 'D:/backup', ios_backup: ios } as ImageInfo }) });
  app.mount(host); await settle();
}
function button(label: string) { return [...host.querySelectorAll('button')].find(element => element.textContent?.includes(label))!; }
beforeEach(() => vi.clearAllMocks());
afterEach(() => { app?.unmount(); host?.remove(); });

describe('iOS backup card', () => {
  it('shows device provenance and associates authentication errors with the password field', async () => {
    rpc.unlock.mockRejectedValue(new Error('wrong password'));
    await mount(metadata);
    expect(host.textContent).toContain('测试 iPhone');
    const input = host.querySelector('input')!;
    input.value = 'wrong'; input.dispatchEvent(new Event('input', { bubbles: true }));
    host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
    expect(input.value).toBe('');
    expect(input.getAttribute('aria-invalid')).toBe('true');
    expect(host.querySelector(`#${CSS.escape(input.getAttribute('aria-describedby')!)}`)?.textContent).toContain('wrong password');
  });

  it('renders unrated domain observations and exports lossless findings with provenance', async () => {
    const findings = [{ code: 'IOS-BACKUP-DOMAIN-ABSENT', severity: null, note: 'observed absence', category: 'Structure', source: { analyzer: 'ios-backup-forensic', scope: 'UDID', version: '0.1.0' }, evidence: [{ field: 'domain', value: 'HomeDomain', location: { Path: 'HomeDomain' } }], subjects: [], context: {} }];
    rpc.audit.mockResolvedValue(findings);
    await mount({ ...metadata, locked: false, observed_encrypted: true });
    button('检查备份').click(); await settle();
    expect(host.textContent).toContain('未评级');
    expect(host.textContent).toContain('observed absence');
    expect(host.querySelector('input')).toBeNull();
    button('导出 JSON').click(); await settle();
    expect(JSON.parse(rpc.export.mock.calls[0][0].content)).toMatchObject({
      data: findings, meta: { image_id: 'a', evidence_path: 'D:/backup', backup: { unique_identifier: 'UDID' } },
    });
  });

  it('warns about unreadable rows and unresolved file lengths', async () => {
    await mount({ ...metadata, unreadable_rows: 2, unresolved_sizes: 1 });
    expect(host.textContent).toContain('2 条记录无法识别');
    expect(host.textContent).toContain('1 个文件未记录长度');
  });
});
