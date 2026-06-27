import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";

import StatusBar from "./StatusBar.vue";
import { useAppState } from "../stores/appState";

function mountSb(
  props: { mode: "setup" | "live" },
  state: Partial<{
    remoteEnabled: boolean;
    controlLocked: boolean;
    presenterUnlocked: boolean;
    selectedWindow: { hwnd: number; title: string } | null;
    activeClientId: string | null;
    presenterOnline: boolean;
  }> = {},
  net: Partial<{ online: boolean }> = {}
) {
  const pinia = createTestingPinia({
    createSpy: vi.fn,
    initialState: {
      app: {
        remoteEnabled: false,
        controlLocked: false,
        presenterUnlocked: false,
        selectedWindow: null,
        activeClientId: null,
        presenterOnline: false,
        ...state,
      },
      network: { online: true, ...net },
    },
    stubActions: true,
  });
  return mount(StatusBar, { props, global: { plugins: [pinia] } });
}

describe("StatusBar — status display", () => {
  it("shows 运行中 when network online", () => {
    const wrapper = mountSb({ mode: "setup" }, {}, { online: true });
    expect(wrapper.text()).toContain("运行中");
  });

  it("shows 未连接 when network offline", () => {
    const wrapper = mountSb({ mode: "setup" }, {}, { online: false });
    expect(wrapper.text()).toContain("未连接");
  });

  it("shows 已选择 when selectedWindow set", () => {
    const wrapper = mountSb({ mode: "setup" }, { selectedWindow: { hwnd: 1, title: "PPT" } });
    expect(wrapper.text()).toContain("已选择");
  });

  it("shows 未选择 when selectedWindow null", () => {
    const wrapper = mountSb({ mode: "setup" }, { selectedWindow: null });
    expect(wrapper.text()).toContain("未选择");
  });

  it("shows activeClientId truncated to 6 chars", () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: "abcdef-123456-extra" });
    expect(wrapper.text()).toContain("abcdef");
  });

  it("shows — for empty activeClientId", () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: null });
    expect(wrapper.text()).toContain("—");
  });
});

describe("StatusBar — remote toggle", () => {
  it("renders 已关闭 when remoteEnabled false", () => {
    const wrapper = mountSb({ mode: "setup" }, { remoteEnabled: false });
    expect(wrapper.find("[data-test='remote-toggle']").text()).toContain("已关闭");
  });

  it("renders 已开启 when remoteEnabled true", () => {
    const wrapper = mountSb({ mode: "setup" }, { remoteEnabled: true });
    expect(wrapper.find("[data-test='remote-toggle']").text()).toContain("已开启");
  });

  it("calls app.setRemoteEnabled(true) when toggling from off", async () => {
    const wrapper = mountSb({ mode: "setup" }, { remoteEnabled: false });
    const app = useAppState();
    await wrapper.find("[data-test='remote-toggle']").trigger("click");
    expect(app.setRemoteEnabled).toHaveBeenCalledWith(true);
  });

  it("calls app.setRemoteEnabled(false) when toggling from on", async () => {
    const wrapper = mountSb({ mode: "setup" }, { remoteEnabled: true });
    const app = useAppState();
    await wrapper.find("[data-test='remote-toggle']").trigger("click");
    expect(app.setRemoteEnabled).toHaveBeenCalledWith(false);
  });

  it("toggle has data-mode='setup' when prop mode=setup", () => {
    const wrapper = mountSb({ mode: "setup" });
    expect(wrapper.find("[data-test='remote-toggle']").attributes("data-mode")).toBe("setup");
  });

  it("toggle has data-mode='live' when prop mode=live", () => {
    const wrapper = mountSb({ mode: "live" });
    expect(wrapper.find("[data-test='remote-toggle']").attributes("data-mode")).toBe("live");
  });
});

describe("StatusBar — release controller", () => {
  it("disables release button when no activeClientId", () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: null });
    expect(wrapper.find("[data-test='release']").attributes("disabled")).toBeDefined();
  });

  it("enables release button when activeClientId is set", () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: "abc" });
    expect(wrapper.find("[data-test='release']").attributes("disabled")).toBeUndefined();
  });

  it("calls app.releaseController() when release clicked", async () => {
    const wrapper = mountSb({ mode: "setup" }, { activeClientId: "abc" });
    const app = useAppState();
    await wrapper.find("[data-test='release']").trigger("click");
    expect(app.releaseController).toHaveBeenCalled();
  });
});
