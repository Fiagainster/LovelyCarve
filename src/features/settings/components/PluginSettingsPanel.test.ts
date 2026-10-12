// @vitest-environment happy-dom
import { createApp, nextTick, type App } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import PluginSettingsPanel from './PluginSettingsPanel.vue';
import type { PluginInfo } from '../../../lib/ipc/plugins';

const ipc = vi.hoisted(() => ({ list: vi.fn(), restart: vi.fn() }));
vi.mock('../../../lib/ipc/plugins', () => ({ pluginList: ipc.list, pluginRestart: ipc.restart }));
const plugin: PluginInfo = { id: 'demo', name: '示例插件', version: '1.0', directory: 'D:\\plugins\\demo', library: 'demo.dll', isolation: 'process', fingerprint: '123456', state: 'configured', error: null, permissions: [], capabilities: [] };
let app: App;
let host: HTMLElement;
async function settle() { for (let index = 0; index < 6; index++) await nextTick(); }
beforeEach(() => { vi.clearAllMocks(); ipc.list.mockResolvedValue([plugin]); ipc.restart.mockResolvedValue(undefined); host = document.createElement('div'); document.body.append(host); });
afterEach(() => { app?.unmount(); host.remove(); });
async function mount() { app = createApp(PluginSettingsPanel); app.mount(host); await settle(); }
function button(label: string) { return [...host.querySelectorAll<HTMLButtonElement>('button')].find(element => element.textContent?.trim() === label)!; }

describe('compact plugin settings', () => {
  it('keeps technical details folded and shows reset progress and success in the card', async () => {
    let finish!: () => void;
    ipc.restart.mockReturnValueOnce(new Promise<void>(resolve => { finish = resolve; }));
    await mount(); expect(host.querySelector('details')?.open).toBe(false);
    button('重置').click(); await settle();
    expect(button('重置中').disabled).toBe(true); expect(button('刷新').disabled).toBe(true);
    finish(); await settle();
    expect(host.querySelector('[role="status"]')?.textContent).toContain('下次使用时启动');
    expect(button('重置').disabled).toBe(false);
  });

  it('shows reset failure inline and keeps the installed list available after a refresh failure', async () => {
    await mount(); ipc.restart.mockRejectedValueOnce(new Error('插件仍在执行'));
    button('重置').click(); await settle();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('插件仍在执行');
    ipc.list.mockRejectedValueOnce(new Error('目录不可读')); button('刷新').click(); await settle();
    expect(host.querySelector('.plugin-card')?.textContent).toContain('示例插件');
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('目录不可读');
  });

  it('does not reload plugins after a pending reset resolves in an unmounted panel', async () => {
    let finish!: () => void;
    ipc.restart.mockReturnValueOnce(new Promise<void>(resolve => { finish = resolve; }));
    await mount(); button('重置').click(); await settle(); app.unmount();
    finish(); await settle(); expect(ipc.list).toHaveBeenCalledOnce();
  });
});
