// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue';
import ConfirmDialogHost from '../../components/feedback/ConfirmDialogHost.vue';
import { useConfirmHost } from '../../composables/useConfirm';
import type { RecentCase } from '../../features/cases/api';
import WelcomeScreen from './WelcomeScreen.vue';

let app: App;
let host: HTMLElement;
const busy = ref(false);
const recentCases = ref<RecentCase[]>([]);
const cleared = vi.fn();
const entry = { dir: '/cases/example', name: '示例案件', openedAt: '2025-12-31T23:59:00' };

async function settle() {
  for (let index = 0; index < 4; index++) await nextTick();
}

async function mount() {
  host = document.createElement('div');
  document.body.append(host);
  app = createApp(defineComponent({
    setup() {
      return () => [
        h(WelcomeScreen, {
          busy: busy.value,
          dragging: false,
          loadLabel: '正在打开案件…',
          recentCases: recentCases.value,
          onClearRecent: () => {
            cleared();
            recentCases.value = [];
          },
        }),
        h(ConfirmDialogHost),
      ];
    },
  }));
  app.mount(host);
  await settle();
}

function clearButton() {
  return host.querySelector<HTMLButtonElement>('[title="清空最近案件记录"]')!;
}

function dialogButton(label: string) {
  return [...document.querySelectorAll<HTMLButtonElement>('.confirm-modal button')]
    .find(button => button.textContent === label)!;
}

beforeEach(() => {
  busy.value = false;
  recentCases.value = [{ ...entry }];
  cleared.mockReset();
});

afterEach(() => {
  useConfirmHost().onCancel();
  app?.unmount();
  host?.remove();
});

describe('welcome recent cases', () => {
  it('keeps history until confirmation and returns focus after cancellation', async () => {
    await mount();
    clearButton().click();
    await settle();
    expect(document.querySelector('.confirm-modal')?.textContent).toContain('案件目录和案件登记均会保留');
    expect(clearButton().disabled).toBe(true);
    expect(cleared).not.toHaveBeenCalled();

    dialogButton('取消').click();
    await settle();
    expect(recentCases.value).toEqual([entry]);
    expect(cleared).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(clearButton());
  });

  it('clears after acceptance and leaves keyboard focus on the recent cases heading', async () => {
    await mount();
    clearButton().click();
    await settle();
    dialogButton('清空记录').click();
    await settle();
    expect(cleared).toHaveBeenCalledTimes(1);
    expect(recentCases.value).toEqual([]);
    expect(host.textContent).toContain('还没有最近打开的案件');
    expect(document.activeElement).toBe(host.querySelector('#recent-title'));
  });

  it('keeps history if another operation becomes busy while confirmation is pending', async () => {
    await mount();
    clearButton().click();
    await settle();
    busy.value = true;
    await settle();
    dialogButton('清空记录').click();
    await settle();
    expect(recentCases.value).toEqual([entry]);
    expect(cleared).not.toHaveBeenCalled();
  });

  it('shows the year and local time label, and gives invalid timestamps an explicit fallback', async () => {
    recentCases.value.push({ dir: '/cases/unknown', name: '时间异常案件', openedAt: 'invalid-date' });
    await mount();
    expect([...host.querySelectorAll('time')].map(element => element.textContent)).toEqual([
      '2025-12-31 23:59:00（本地）',
      '时间未知',
    ]);
  });
});
