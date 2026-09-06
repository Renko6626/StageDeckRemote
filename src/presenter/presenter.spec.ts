import html from './presenter.html?raw';
import script from './presenter.js?raw';
import { afterEach, expect, it, vi } from 'vitest';

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

function mountPresenter(
  slideResponse: () => Promise<Response>,
  statusResponse = () => Promise.resolve(new Response(JSON.stringify({
    remote_enabled: true, control_locked: false, presenter_unlocked: true,
    is_controller: true,
  }), { status: 200 })),
) {
  document.documentElement.innerHTML = html;
  vi.stubGlobal('fetch', vi.fn((url: string) => {
    if (url.includes('slide_state')) return slideResponse();
    return statusResponse();
  }));
  window.eval(script);
}

it('keeps the heartbeat online while PowerPoint status is blocked', async () => {
  vi.useFakeTimers();
  mountPresenter(() => new Promise(() => {}));
  await vi.advanceTimersByTimeAsync(100);
  expect(document.getElementById('conn-text')?.textContent).toBe('LINK');
});

it('aborts a hung status heartbeat after 5 seconds', async () => {
  vi.useFakeTimers();
  let signal: AbortSignal | undefined;
  document.documentElement.innerHTML = html;
  vi.stubGlobal('fetch', vi.fn((url: string, options: RequestInit) => {
    if (url.includes('slide_state')) {
      return Promise.resolve(new Response(JSON.stringify({ available: false, actions: [] })));
    }
    if (!signal) signal = options.signal as AbortSignal;
    return new Promise<Response>(() => {});
  }));
  window.eval(script);
  await vi.advanceTimersByTimeAsync(4900);
  expect(signal?.aborted).toBe(false);
  await vi.advanceTimersByTimeAsync(200);
  expect(signal?.aborted).toBe(true);
});

// ── 三档连接状态(spec 2026-09-07 §三) ──
function okStatus() {
  return Promise.resolve(new Response(JSON.stringify({
    remote_enabled: true, control_locked: false, presenter_unlocked: true, is_controller: true,
  }), { status: 200 }));
}
function okSlide() {
  return Promise.resolve(new Response(JSON.stringify({ available: true, in_show: true, current: 3, total: 9, actions: [] })));
}
/** 先成功一次,然后 status 全部挂起(模拟坏网)。 */
async function mountThenHang(baseHref?: string) {
  vi.useFakeTimers();
  document.documentElement.innerHTML = html;
  if (baseHref) (document.querySelector('base') as HTMLBaseElement).href = baseHref;
  let hang = false;
  vi.stubGlobal('fetch', vi.fn((url: string, options: RequestInit) => {
    if (url.includes('slide_state')) return okSlide();
    if (hang) return hangUntilAbort(options.signal as AbortSignal);
    return okStatus();
  }));
  window.eval(script);
  await vi.advanceTimersByTimeAsync(50);
  expect(connText()).toBe('LINK');
  hang = true;
  return { nextBtn: document.querySelector('.d-next') as HTMLButtonElement };
}
const connText = () => document.getElementById('conn-text')?.textContent;
/** 真实 fetch 在 AbortController 超时时 reject;桩必须一样,否则重试永远不会触发。 */
function hangUntilAbort(signal?: AbortSignal) {
  return new Promise<Response>((_, reject) => {
    signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
  });
}

it('degrades LINK → 弱 → DOWN on LAN (8s) and keeps buttons usable while 弱', async () => {
  const { nextBtn } = await mountThenHang();
  expect(nextBtn.disabled).toBe(false);
  await vi.advanceTimersByTimeAsync(3600);
  expect(connText()).toMatch(/^弱 · \ds$/);
  expect(document.getElementById('conn-dot')?.classList.contains('weak')).toBe(true);
  expect(nextBtn.disabled).toBe(false); // 弱档仍可操作,靠重试兜底
  await vi.advanceTimersByTimeAsync(5000); // 累计 >8s
  expect(connText()).toBe('DOWN');
  expect(nextBtn.disabled).toBe(true);
  expect(document.getElementById('lock-overlay')?.textContent).toBe('连接已断开');
});

it('waits 15s before DOWN when served through the relay', async () => {
  await mountThenHang('/r/PROM-2026/presenter/');
  await vi.advanceTimersByTimeAsync(9000);
  expect(connText()).toMatch(/^弱/);
  await vi.advanceTimersByTimeAsync(7000); // >15s
  expect(connText()).toBe('DOWN');
});

it('recovers to LINK on the next successful heartbeat without re-arming', async () => {
  vi.useFakeTimers();
  document.documentElement.innerHTML = html;
  let hang = false;
  vi.stubGlobal('fetch', vi.fn((url: string) => {
    if (url.includes('slide_state')) return okSlide();
    if (hang) return Promise.reject(new TypeError('network'));
    return okStatus();
  }));
  window.eval(script);
  await vi.advanceTimersByTimeAsync(50);
  hang = true;
  await vi.advanceTimersByTimeAsync(9000);
  expect(connText()).toBe('DOWN');
  hang = false;
  await vi.advanceTimersByTimeAsync(1000);
  expect(connText()).toBe('LINK');
  expect((document.querySelector('.d-next') as HTMLButtonElement).disabled).toBe(false);
});

// ── 幂等重试(spec §二) ──
type Call = { url: string; body: any; signal: AbortSignal };
/** 挂载并让 status/slide 正常;返回对 /api/next 的调用记录与可编程的应答队列。 */
async function mountForOps(answers: Array<'hang' | 'neterr' | number | { status: number; reason: string }>) {
  vi.useFakeTimers();
  document.documentElement.innerHTML = html;
  const calls: Call[] = [];
  vi.stubGlobal('fetch', vi.fn((url: string, options: RequestInit) => {
    if (url.includes('slide_state')) return okSlide();
    if (url.includes('status')) return okStatus();
    calls.push({ url, body: JSON.parse(options.body as string), signal: options.signal as AbortSignal });
    const a = answers.shift();
    if (a === 'hang') return hangUntilAbort(options.signal as AbortSignal);
    if (a === 'neterr') return Promise.reject(new TypeError('network'));
    if (typeof a === 'number') return Promise.resolve(new Response(JSON.stringify({ ok: a === 200 }), { status: a }));
    if (a) return Promise.resolve(new Response(JSON.stringify({ ok: false, reason: a.reason }), { status: a.status }));
    return Promise.resolve(new Response('{"ok":true}', { status: 200 }));
  }));
  window.eval(script);
  await vi.advanceTimersByTimeAsync(50);
  const nextBtn = document.querySelector('.d-next') as HTMLButtonElement;
  expect(nextBtn.disabled).toBe(false);
  return { calls, nextBtn, banner: () => document.getElementById('dir-banner-text')?.textContent };
}

it('retries a hung page-turn twice with the same op_id, then succeeds silently', async () => {
  const { calls, nextBtn, banner } = await mountForOps(['hang', 'hang', 200]);
  nextBtn.click();
  await vi.advanceTimersByTimeAsync(10);
  expect(nextBtn.disabled).toBe(true); // pending 守卫
  expect(nextBtn.classList.contains('sending')).toBe(true);
  await vi.advanceTimersByTimeAsync(1600); // 第 1 次 1.5s 超时
  expect(calls.length).toBe(2);
  expect(calls[0].signal.aborted).toBe(true);
  await vi.advanceTimersByTimeAsync(1600); // 第 2 次超时 → 第 3 次立刻成功
  expect(calls.length).toBe(3);
  expect(new Set(calls.map(c => c.body.op_id)).size).toBe(1);
  expect(calls[0].body.op_id).toMatch(/-1$/);
  await vi.advanceTimersByTimeAsync(50);
  expect(nextBtn.disabled).toBe(false);
  expect(nextBtn.classList.contains('sending')).toBe(false);
  expect(document.getElementById('dir-banner')?.hidden).toBe(true);
  expect(banner()).toBe('');
});

it('reports unknown result after three failed attempts', async () => {
  const { calls, banner } = await mountForOps(['neterr', 'neterr', 'hang']);
  (document.querySelector('.d-next') as HTMLElement).click();
  await vi.advanceTimersByTimeAsync(100);
  expect(calls.length).toBe(3); // 两次快失败立刻重发
  await vi.advanceTimersByTimeAsync(5100); // 最后一次等完整 5s
  expect(banner()).toContain('结果未知');
});

it('retries immediately on 503 from the relay but not on 4xx', async () => {
  const { calls, banner } = await mountForOps([503, 403]);
  (document.querySelector('.d-next') as HTMLElement).click();
  await vi.advanceTimersByTimeAsync(100);
  expect(calls.length).toBe(2);
  expect(banner()).toContain('锁定');
});

it('stops retrying and says delivered on 409 op_in_flight', async () => {
  const { calls, banner } = await mountForOps([{ status: 409, reason: 'op_in_flight' }]);
  (document.querySelector('.d-next') as HTMLElement).click();
  await vi.advanceTimersByTimeAsync(100);
  expect(calls.length).toBe(1);
  expect(banner()).toContain('已送达');
});

it('uses a new op_id for each press and disables macro buttons while a page-turn is pending', async () => {
  const { calls, nextBtn } = await mountForOps(['hang', 200, 200]);
  document.getElementById('actions')!.innerHTML = '<button class="act" data-id="Answer">Answer</button>';
  nextBtn.click();
  await vi.advanceTimersByTimeAsync(10);
  const act = document.querySelector('.act') as HTMLButtonElement;
  // render() 把 pending 传播到宏按钮
  expect(act.disabled).toBe(true);
  await vi.advanceTimersByTimeAsync(1600); // 重试并成功
  await vi.advanceTimersByTimeAsync(50);
  expect(nextBtn.disabled).toBe(false);
  nextBtn.click();
  await vi.advanceTimersByTimeAsync(50);
  const ids = calls.map(c => c.body.op_id);
  expect(ids[0]).toBe(ids[1]); // 同一次按下的重试
  expect(ids[2]).not.toBe(ids[0]); // 第二次按下换 id
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
