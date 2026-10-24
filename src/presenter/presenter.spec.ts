import html from './presenter.html?raw';
import script from './presenter.js?raw';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const initialUrl = window.location.href;
const diagnosticListeners: Array<[string, EventListenerOrEventListenerObject]> = [];
beforeEach(() => {
  const add = window.addEventListener.bind(window);
  vi.spyOn(window, 'addEventListener').mockImplementation((type, listener, options) => {
    if (type === 'error' || type === 'unhandledrejection') diagnosticListeners.push([type, listener]);
    add(type, listener, options);
  });
});
afterEach(() => {
  for (const [type, listener] of diagnosticListeners.splice(0)) window.removeEventListener(type, listener);
  vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks();
  window.history.replaceState(null, '', initialUrl);
});

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

it('refreshes control permissions between slower slide scans', async () => {
  vi.useFakeTimers();
  let current = 0;
  let enabled = true;
  mountPresenter(() => Promise.resolve(new Response(JSON.stringify({
    available: true, in_show: true, current: ++current, total: 9, actions: [],
  }))), () => Promise.resolve(new Response(JSON.stringify({
    remote_enabled: enabled, control_locked: false, presenter_unlocked: true, is_controller: true,
  }))));
  await vi.advanceTimersByTimeAsync(100);
  expect(document.getElementById('slide-pos')?.textContent).toBe('1/9');
  enabled = false;
  await vi.advanceTimersByTimeAsync(1000);
  expect((document.querySelector('.d-next') as HTMLButtonElement).disabled).toBe(true);
  expect(document.getElementById('slide-pos')?.textContent).toBe('1/9');
  await vi.advanceTimersByTimeAsync(2000);
  expect(document.getElementById('slide-pos')?.textContent).toBe('2/9');
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
async function mountForOps(answers: Array<'hang' | 'neterr' | number | { status: number; reason: string }>, baseHref = '/presenter/', diagnosticFails = false) {
  vi.useFakeTimers();
  document.documentElement.innerHTML = html;
  document.querySelector('base')!.setAttribute('href', baseHref);
  const calls: Call[] = [];
  const diagnostics: Call[] = [];
  vi.stubGlobal('fetch', vi.fn((url: string, options: RequestInit) => {
    if (url.includes('slide_state')) return okSlide();
    if (url.includes('status')) return okStatus();
    if (url.includes('diagnostics')) {
      diagnostics.push({ url, body: JSON.parse(options.body as string), signal: options.signal as AbortSignal });
      return diagnosticFails ? Promise.reject(new TypeError('offline')) : Promise.resolve(new Response('{}'));
    }
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
  return { calls, diagnostics, nextBtn, banner: () => document.getElementById('dir-banner-text')?.textContent };
}

it.each([
  ['/presenter/', '/presenter/api/next', false],
  ['/r/PROM-2026/presenter/', '/r/PROM-2026/presenter/api/next', true],
] as const)('sends POST authentication for %s', async (baseHref, expectedPath, relay) => {
  const token = 'token+a/b?c=d&中文';
  window.history.replaceState(null, '', '/?token=' + encodeURIComponent(token));
  const { calls, nextBtn } = await mountForOps([200], baseHref);
  nextBtn.click();
  await vi.advanceTimersByTimeAsync(100);
  expect(calls).toHaveLength(1);
  const url = new URL(calls[0].url, document.baseURI);
  expect(url.pathname).toBe(expectedPath);
  expect(url.searchParams.get('token')).toBe(relay ? token : null);
  expect(calls[0].body.token).toBe(token);
});

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
  expect((document.querySelector('.d-next') as HTMLButtonElement).disabled).toBe(false);
  expect((document.querySelector('[data-action="prev"]') as HTMLButtonElement).disabled).toBe(false);
  expect((document.querySelector('[data-action="black"]') as HTMLButtonElement).disabled).toBe(true);
  expect((document.querySelector('[data-action="restore"]') as HTMLButtonElement).disabled).toBe(true);
  const preview = document.getElementById('preview')!;
  vi.spyOn(preview, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0, width: 100, height: 100 } as DOMRect);
  preview.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: 50, clientY: 50 }));
  await vi.advanceTimersByTimeAsync(50);
  expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).includes('api/click'))).toBe(true);
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


it('reports an operation failure with its request identity and no request contents', async () => {
  const { calls, diagnostics, nextBtn } = await mountForOps([500]);
  expect(diagnostics).toHaveLength(0); // Healthy polling sends no diagnostics.
  nextBtn.click();
  await vi.advanceTimersByTimeAsync(100);
  expect(diagnostics).toHaveLength(1);
  expect(diagnostics[0].body).toMatchObject({ kind: 'request' });
  expect(diagnostics[0].body.message).toContain('POST api/next duration_ms=');
  expect(diagnostics[0].body.message).toContain('op_id=' + calls[0].body.op_id + ' attempt=1 status=500');
  expect(diagnostics[0].body.message).not.toContain('token=');
});

it('ignores reporting failures without recursion or changing operation retries', async () => {
  const { calls, diagnostics, nextBtn, banner } = await mountForOps(['neterr', 'neterr', 200], '/presenter/', true);
  nextBtn.click();
  await vi.advanceTimersByTimeAsync(100);
  expect(calls).toHaveLength(3);
  expect(diagnostics).toHaveLength(1); // Repeated network errors are suppressed for 30 seconds.
  expect(nextBtn.disabled).toBe(false);
  expect(banner()).toBe('');
});

it('redacts raw and encoded tokens and page URLs in script errors and rejections', async () => {
  const token = 'secret+中文/token';
  window.history.replaceState(null, '', '/?token=' + encodeURIComponent(token));
  const { diagnostics } = await mountForOps([], '/r/PROM-2026/presenter/');
  window.dispatchEvent(new ErrorEvent('error', {
    message: 'Failure ' + token + ' ' + encodeURIComponent(token) + ' https://host/presenter?token=' + token,
    filename: window.location.href,
  }));
  const rejection = new Event('unhandledrejection');
  Object.defineProperty(rejection, 'reason', { value: new Error('Rejected ' + token) });
  window.dispatchEvent(rejection);
  await vi.advanceTimersByTimeAsync(10);
  expect(diagnostics).toHaveLength(2);
  for (const report of diagnostics) {
    expect(report.body.kind).toBe('script');
    expect(report.body.message).toContain('[redacted]');
    expect(report.body.message).not.toContain(token);
    expect(report.body.message).not.toContain(encodeURIComponent(token));
    expect(report.body.message).not.toContain('https://host');
    expect(report.body.message).not.toContain('?token');
    expect(new URL(report.url, document.baseURI).searchParams.get('token')).toBe(token);
  }
});
