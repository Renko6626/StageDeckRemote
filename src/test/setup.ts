// 全局测试 setup。所有 .spec.ts 自动执行,在 vite.config.ts 的
// test.setupFiles 配置。
import { vi } from "vitest";

// Tauri IPC 在测试环境下不可用,统一桩成 noop。后续每个测试可以
// 自己 vi.mocked(invoke).mockResolvedValue(...) 改返回。
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));
