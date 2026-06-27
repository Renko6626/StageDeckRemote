import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";

import LocalNav from "./LocalNav.vue";
import { api } from "../utils/api";

vi.mock("../utils/api", () => ({
  api: {
    localPrev: vi.fn().mockResolvedValue(undefined),
    localNext: vi.fn().mockResolvedValue(undefined),
  },
}));

function mountLn() {
  return mount(LocalNav, {
    global: { plugins: [createTestingPinia({ createSpy: vi.fn })] },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("LocalNav", () => {
  it("renders 上一页 and 下一页 buttons", () => {
    const wrapper = mountLn();
    expect(wrapper.text()).toContain("上一页");
    expect(wrapper.text()).toContain("下一页");
  });

  it("calls api.localPrev() when 上一页 clicked", async () => {
    const wrapper = mountLn();
    await wrapper.find("[data-test='prev']").trigger("click");
    expect(api.localPrev).toHaveBeenCalled();
  });

  it("calls api.localNext() when 下一页 clicked", async () => {
    const wrapper = mountLn();
    await wrapper.find("[data-test='next']").trigger("click");
    expect(api.localNext).toHaveBeenCalled();
  });

  it("shows error if IPC throws", async () => {
    vi.mocked(api.localNext).mockRejectedValueOnce(new Error("test error"));
    const wrapper = mountLn();
    await wrapper.find("[data-test='next']").trigger("click");
    await wrapper.vm.$nextTick();
    expect(wrapper.text()).toContain("test error");
  });
});
