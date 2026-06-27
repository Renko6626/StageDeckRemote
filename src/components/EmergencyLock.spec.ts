import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";

import EmergencyLock from "./EmergencyLock.vue";
import { useAppState } from "../stores/appState";

function mountWithPinia(props: { size: "small" | "large" }, initialLocked = false) {
  const pinia = createTestingPinia({
    createSpy: vi.fn,
    initialState: {
      app: {
        controlLocked: initialLocked,
      },
    },
    stubActions: true,
  });
  return mount(EmergencyLock, { props, global: { plugins: [pinia] } });
}

describe("EmergencyLock", () => {
  it("renders 紧急锁定 when not locked", () => {
    const wrapper = mountWithPinia({ size: "small" });
    expect(wrapper.text()).toContain("紧急锁定");
    expect(wrapper.text()).not.toContain("点击释放");
  });

  it("renders 已紧急锁定 when locked", () => {
    const wrapper = mountWithPinia({ size: "small" }, true);
    expect(wrapper.text()).toContain("已紧急锁定");
    expect(wrapper.text()).toContain("点击释放");
  });

  it("calls app.forceLock(true) when clicked from unlocked", async () => {
    const wrapper = mountWithPinia({ size: "small" }, false);
    const app = useAppState();
    await wrapper.find("button").trigger("click");
    expect(app.forceLock).toHaveBeenCalledWith(true);
  });

  it("calls app.forceLock(false) when clicked from locked", async () => {
    const wrapper = mountWithPinia({ size: "small" }, true);
    const app = useAppState();
    await wrapper.find("button").trigger("click");
    expect(app.forceLock).toHaveBeenCalledWith(false);
  });

  it("applies data-size='small' to button when size=small", () => {
    const wrapper = mountWithPinia({ size: "small" });
    expect(wrapper.find("button").attributes("data-size")).toBe("small");
  });

  it("applies data-size='large' to button when size=large", () => {
    const wrapper = mountWithPinia({ size: "large" });
    expect(wrapper.find("button").attributes("data-size")).toBe("large");
  });
});
