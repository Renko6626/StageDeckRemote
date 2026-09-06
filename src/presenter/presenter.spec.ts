import html from './presenter.html?raw';
import script from './presenter.js?raw';
import { afterEach, expect, it, vi } from 'vitest';

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

function mountPresenter(slideResponse: () => Promise<Response>) {
  document.documentElement.innerHTML = html;
  vi.stubGlobal('fetch', vi.fn((url: string) => {
    if (url.includes('slide_state')) return slideResponse();
    return Promise.resolve(new Response(JSON.stringify({
      remote_enabled: true, control_locked: false, presenter_unlocked: true,
      is_controller: true,
    }), { status: 200 }));
  }));
  window.eval(script);
}

it('keeps the heartbeat online while PowerPoint status is blocked', async () => {
  vi.useFakeTimers();
  mountPresenter(() => new Promise(() => {}));
  await vi.advanceTimersByTimeAsync(100);
  expect(document.getElementById('conn-text')?.textContent).toBe('LINK');
});

it('distinguishes a busy PowerPoint from a lost connection', async () => {
  vi.useFakeTimers();
  mountPresenter(() => Promise.resolve(new Response(JSON.stringify({
    available: false, busy: true, in_show: false, actions: [],
  }))));
  await vi.advanceTimersByTimeAsync(100);
  expect(document.getElementById('conn-text')?.textContent).toBe('LINK');
  expect(document.getElementById('com-unavail')?.textContent).toContain('忙');
});

it('shows a macro failure instead of silently swallowing HTTP 500', async () => {
  vi.useFakeTimers();
  mountPresenter(() => Promise.resolve(new Response(JSON.stringify({ available: true, actions: [] }))));
  await vi.advanceTimersByTimeAsync(100);
  document.getElementById('actions')!.innerHTML = '<button class="act" data-id="Answer">Answer</button>';
  vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response(JSON.stringify({ reason: 'action_failed' }), { status: 500 }))));
  (document.querySelector('.act') as HTMLElement).click();
  await vi.advanceTimersByTimeAsync(100);
  expect(document.getElementById('dir-banner-text')?.textContent).toContain('失败');
});
