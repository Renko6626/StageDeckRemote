import { createPinia, setActivePinia } from 'pinia';
import { expect, it, vi } from 'vitest';
import { api } from '../utils/api';
import { useAppState } from './appState';

it('refreshes logs even while another status IPC is blocked', async () => {
  setActivePinia(createPinia());
  vi.spyOn(api, 'getStatus').mockImplementation(() => new Promise(() => {}));
  vi.spyOn(api, 'getRelayStatus').mockResolvedValue('connected');
  vi.spyOn(api, 'getLogs').mockResolvedValue([{ time_ms: 1, message: '宏 Answer 开始' }]);
  const app = useAppState();
  void app.refresh();
  await new Promise(resolve => setTimeout(resolve, 0));
  expect(app.logs[0]?.message).toBe('宏 Answer 开始');
  vi.restoreAllMocks();
});
