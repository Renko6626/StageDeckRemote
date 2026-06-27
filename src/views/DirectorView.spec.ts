import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";
import { createRouter, createMemoryHistory } from "vue-router";

import DirectorView from "./DirectorView.vue";
import { useAppState } from "../stores/appState";

// 把所有外部 widget 桩成只渲染 tag name,避免 router/IPC/canvas 等
// 依赖在测试里炸。
vi.mock("../components/WindowPicker.vue", () => ({
  default: { name: "WindowPicker", template: "<div data-test='WindowPicker' />" },
}));
vi.mock("../components/StatusBar.vue", () => ({
  default: {
    name: "StatusBar",
    props: ["mode"],
    template: "<div data-test='StatusBar' :data-mode='mode' />",
  },
}));
vi.mock("../components/EmergencyLock.vue", () => ({
  default: {
    name: "EmergencyLock",
    props: ["size"],
    template: "<div data-test='EmergencyLock' :data-size='size' />",
  },
}));
vi.mock("../components/LocalNav.vue", () => ({
  default: { name: "LocalNav", template: "<div data-test='LocalNav' />" },
}));
vi.mock("../components/DirectorTimer.vue", () => ({
  default: { name: "DirectorTimer", template: "<div data-test='DirectorTimer' />" },
}));
vi.mock("../components/DirectorMessages.vue", () => ({
  default: { name: "DirectorMessages", template: "<div data-test='DirectorMessages' />" },
}));
vi.mock("../components/QrCodePanel.vue", () => ({
  default: { name: "QrCodePanel", template: "<div data-test='QrCodePanel' />" },
}));
vi.mock("../components/DirectorPreview.vue", () => ({
  default: { name: "DirectorPreview", template: "<div data-test='DirectorPreview' />" },
}));
vi.mock("../components/LogPanel.vue", () => ({
  default: { name: "LogPanel", template: "<div data-test='LogPanel' />" },
}));

function mountDv() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/", component: { template: "<div />" } }],
  });
  return mount(DirectorView, {
    global: {
      plugins: [
        createTestingPinia({
          createSpy: vi.fn,
          initialState: {
            app: {
              armed: false,
              remoteEnabled: false,
              controlLocked: false,
              presenterUnlocked: false,
            },
          },
          stubActions: true,
        }),
        router,
      ],
    },
  });
}

beforeEach(() => {
  localStorage.clear();
});

describe("DirectorView — 顶栏模式切换", () => {
  it("默认在 Setup 模式", () => {
    const wrapper = mountDv();
    const setupBtn = wrapper.find("[data-test='mode-setup']");
    expect(setupBtn.attributes("data-active")).toBe("true");
  });

  it("点 Live 后切到 Live 模式", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(wrapper.find("[data-test='mode-live']").attributes("data-active")).toBe("true");
    expect(wrapper.find("[data-test='mode-setup']").attributes("data-active")).toBe("false");
  });

  it("再点 Setup 切回去", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    await wrapper.find("[data-test='mode-setup']").trigger("click");
    expect(wrapper.find("[data-test='mode-setup']").attributes("data-active")).toBe("true");
  });
});

describe("DirectorView — Setup rail 内容", () => {
  it("Setup 模式渲染 StatusBar / WindowPicker / QrCodePanel / LocalNav / EmergencyLock(小)", () => {
    const wrapper = mountDv();
    expect(wrapper.find("[data-test='StatusBar']").exists()).toBe(true);
    expect(wrapper.find("[data-test='WindowPicker']").exists()).toBe(true);
    expect(wrapper.find("[data-test='QrCodePanel']").exists()).toBe(true);
    expect(wrapper.find("[data-test='LocalNav']").exists()).toBe(true);
    expect(wrapper.find("[data-test='EmergencyLock']").attributes("data-size")).toBe("small");
  });

  it("Setup 模式不渲染 Timer / Messages", () => {
    const wrapper = mountDv();
    expect(wrapper.find("[data-test='DirectorTimer']").exists()).toBe(false);
    expect(wrapper.find("[data-test='DirectorMessages']").exists()).toBe(false);
  });

  it("StatusBar 在 Setup 模式下 mode prop = 'setup'", () => {
    const wrapper = mountDv();
    expect(wrapper.find("[data-test='StatusBar']").attributes("data-mode")).toBe("setup");
  });
});

describe("DirectorView — Live rail 内容", () => {
  it("Live 模式渲染 StatusBar / EmergencyLock(大) / Timer / Messages / LocalNav", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(wrapper.find("[data-test='StatusBar']").exists()).toBe(true);
    expect(wrapper.find("[data-test='EmergencyLock']").attributes("data-size")).toBe("large");
    expect(wrapper.find("[data-test='DirectorTimer']").exists()).toBe(true);
    expect(wrapper.find("[data-test='DirectorMessages']").exists()).toBe(true);
    expect(wrapper.find("[data-test='LocalNav']").exists()).toBe(true);
  });

  it("Live 模式不渲染 WindowPicker / QrCodePanel", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(wrapper.find("[data-test='WindowPicker']").exists()).toBe(false);
    expect(wrapper.find("[data-test='QrCodePanel']").exists()).toBe(false);
  });

  it("StatusBar 在 Live 模式下 mode prop = 'live'", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(wrapper.find("[data-test='StatusBar']").attributes("data-mode")).toBe("live");
  });
});

describe("DirectorView — mode 切换不动业务状态(spec §4)", () => {
  it("切换 mode 三次,setRemoteEnabled / forceLock 都不被调", async () => {
    const wrapper = mountDv();
    const app = useAppState();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    await wrapper.find("[data-test='mode-setup']").trigger("click");
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(app.setRemoteEnabled).not.toHaveBeenCalled();
    expect(app.forceLock).not.toHaveBeenCalled();
    expect(app.releaseController).not.toHaveBeenCalled();
  });
});

describe("DirectorView — mode 不持久化(spec §2.1)", () => {
  it("切到 Live 之后不写 mode 到 localStorage", async () => {
    const wrapper = mountDv();
    await wrapper.find("[data-test='mode-live']").trigger("click");
    await flushPromises();
    expect(localStorage.getItem("sd.dir.mode")).toBeNull();
  });
});

describe("DirectorView — splitter localStorage 跨模式记住(spec §5.3)", () => {
  it("railW 和 logH 从 localStorage 读入并跨 Setup ↔ Live 模式持久", async () => {
    localStorage.setItem("sd.dir.railW", "420");
    localStorage.setItem("sd.dir.logH", "350");
    const wrapper = mountDv();
    // 初始 Setup 模式: rail-w / log-h 已应用
    expect(wrapper.find(".dashboard").attributes("style")).toContain("--rail-w: 420px");
    expect(wrapper.find(".dashboard").attributes("style")).toContain("--log-h: 350px");
    // 切到 Live
    await wrapper.find("[data-test='mode-live']").trigger("click");
    expect(wrapper.find(".dashboard").attributes("style")).toContain("--rail-w: 420px");
    expect(wrapper.find(".dashboard").attributes("style")).toContain("--log-h: 350px");
    // 切回 Setup
    await wrapper.find("[data-test='mode-setup']").trigger("click");
    expect(wrapper.find(".dashboard").attributes("style")).toContain("--rail-w: 420px");
    expect(wrapper.find(".dashboard").attributes("style")).toContain("--log-h: 350px");
  });
});
