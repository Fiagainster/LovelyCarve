// @vitest-environment happy-dom
import { createApp, h, nextTick, ref, type App } from 'vue';
import { afterEach, describe, expect, it } from 'vitest';
import UiSelect, { type UiSelectOption } from './UiSelect.vue';

const OPTIONS: UiSelectOption<string>[] = [
  { value: 'android', label: 'Android' },
  { value: 'ios', label: 'iOS' },
  { value: 'linux', label: 'Linux', disabled: true },
  { value: 'macos', label: 'macOS' },
  { value: 'windows', label: 'Windows' },
];

let app: App | null = null;
let host: HTMLElement;
const value = ref('ios');
const events: string[] = [];
const outerKeys: string[] = [];

function mount(extra: Record<string, unknown> = {}) {
  value.value = 'ios';
  events.length = 0;
  outerKeys.length = 0;
  host = document.createElement('div');
  host.addEventListener('keydown', event => outerKeys.push(event.key));
  document.body.append(host);
  app = createApp({
    render: () => h(UiSelect, {
      modelValue: value.value,
      'onUpdate:modelValue': (next: string) => { events.push(`update:${next}`); value.value = next; },
      onChange: (next: string) => events.push(`change:${next}`),
      options: OPTIONS,
      'aria-label': '平台',
      ...extra,
    }),
  });
  app.mount(host);
}

async function settle() {
  await nextTick();
  await nextTick();
}

const trigger = () => host.querySelector<HTMLButtonElement>('button[role=combobox]')!;
const listbox = () => document.querySelector<HTMLElement>('[role=listbox]');
const option = (label: string) => [...document.querySelectorAll<HTMLElement>('[role=option]')].find(item => item.textContent === label)!;
const highlightedLabel = () => document.querySelector('[role=option][data-highlighted]')?.textContent;

async function press(key: string) {
  trigger().dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
  await settle();
}

afterEach(() => {
  app?.unmount();
  app = null;
  document.body.replaceChildren();
});

describe('UiSelect', () => {
  it('shows the current label and keeps every label for the field width', () => {
    mount();
    const labels = [...trigger().querySelectorAll('.ui-select-value > span')];
    expect(labels.map(label => label.textContent)).toEqual(OPTIONS.map(item => item.label));
    expect(labels.filter(label => !label.hasAttribute('aria-hidden')).map(label => label.textContent)).toEqual(['iOS']);
    expect(trigger().getAttribute('aria-expanded')).toBe('false');
    expect(trigger().classList.contains('ui-select')).toBe(true);
  });

  it('opens on the current value and picks with a click, updating before change', async () => {
    mount();
    trigger().click();
    await settle();
    expect(trigger().getAttribute('aria-expanded')).toBe('true');
    expect(trigger().getAttribute('aria-activedescendant')).toBe(option('iOS').id);
    expect(option('iOS').getAttribute('aria-selected')).toBe('true');

    option('Windows').click();
    await settle();
    expect(events).toEqual(['update:windows', 'change:windows']);
    expect(listbox()).toBeNull();
    expect(document.activeElement).toBe(trigger());
  });

  it('moves with the arrow keys past disabled options and commits with Enter', async () => {
    mount();
    await press('ArrowDown');
    expect(highlightedLabel()).toBe('iOS');
    await press('ArrowDown');
    expect(highlightedLabel()).toBe('macOS');
    await press('End');
    expect(highlightedLabel()).toBe('Windows');
    await press('Home');
    expect(highlightedLabel()).toBe('Android');
    await press('Enter');
    expect(events).toEqual(['update:android', 'change:android']);
    expect(listbox()).toBeNull();
  });

  it('ignores clicks on disabled options', async () => {
    mount();
    trigger().click();
    await settle();
    expect(option('Linux').getAttribute('aria-disabled')).toBe('true');
    option('Linux').click();
    await settle();
    expect(events).toEqual([]);
    expect(listbox()).not.toBeNull();
  });

  it('jumps to an option by its first letters', async () => {
    mount();
    await press('w');
    expect(highlightedLabel()).toBe('Windows');
    await press('Enter');
    expect(value.value).toBe('windows');
  });

  it('closes only the list on Escape, then lets Escape reach the dialog', async () => {
    mount();
    await press('ArrowDown');
    await press('Escape');
    expect(listbox()).toBeNull();
    expect(outerKeys).not.toContain('Escape');
    await press('Escape');
    expect(outerKeys).toContain('Escape');
    expect(events).toEqual([]);
  });

  it('closes on a pointer outside without changing the value', async () => {
    mount();
    trigger().click();
    await settle();
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    await settle();
    expect(listbox()).toBeNull();
    expect(events).toEqual([]);
  });

  it('does not report a change when the current value is picked again', async () => {
    mount();
    trigger().click();
    await settle();
    option('iOS').click();
    await settle();
    expect(events).toEqual([]);
  });

  it('stays closed while disabled', async () => {
    mount({ disabled: true });
    expect(trigger().disabled).toBe(true);
    await press('ArrowDown');
    expect(listbox()).toBeNull();
  });
});
