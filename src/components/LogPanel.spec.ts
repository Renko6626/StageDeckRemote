import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";
import { invoke } from "@tauri-apps/api/core";

import LogPanel from "./LogPanel.vue";
import { useAppState } from "../stores/appState";

const status = { enabled: false, directory: "C:\\logs", error: null };

function mountPanel() {
  return mount(LogPanel, {
    global: { plugins: [createTestingPinia({ createSpy: vi.fn })] },
  });
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue(status);
});

describe("LogPanel diagnostics", () => {
  it("loads the directory and reflects a successful detailed-mode change", async () => {
    const wrapper = mountPanel();
    await flushPromises();
    expect(wrapper.text()).toContain("C:\\logs");

    vi.mocked(invoke).mockResolvedValueOnce({ ...status, enabled: true });
    await wrapper.get("input").setValue(true);
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith("set_detailed_diagnostics", { enabled: true });
    expect((wrapper.get("input").element as HTMLInputElement).checked).toBe(true);
  });

  it("keeps a failed change unchecked and displays the failure", async () => {
    const wrapper = mountPanel();
    await flushPromises();
    vi.mocked(invoke).mockRejectedValueOnce("mode unavailable");

    await wrapper.get("input").setValue(true);
    await flushPromises();

    expect((wrapper.get("input").element as HTMLInputElement).checked).toBe(false);
    expect(wrapper.text()).toContain("切换详细诊断失败：mode unavailable");
  });

  it("shows directory operation errors and updates persistent errors with logs", async () => {
    const wrapper = mountPanel();
    await flushPromises();
    vi.mocked(invoke).mockRejectedValueOnce("open failed");
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("open_logs_directory");
    expect(wrapper.text()).toContain("打开日志目录失败：open failed");

    vi.mocked(invoke).mockResolvedValueOnce({ ...status, error: "disk full" });
    useAppState().logs = [{ time_ms: 1, message: "新事件" }];
    await flushPromises();
    expect(wrapper.text()).toContain("日志文件不可用：disk full");
    expect(wrapper.text()).toContain("打开日志目录失败：open failed");
  });

  it("displays an initial status failure", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("status unavailable");
    const wrapper = mountPanel();
    await flushPromises();
    expect(wrapper.text()).toContain("读取诊断状态失败：status unavailable");
    expect(wrapper.get("input").attributes("disabled")).toBeDefined();
  });
});
