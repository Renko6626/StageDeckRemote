import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createTestingPinia } from "@pinia/testing";
import ConnectionPanel from "./ConnectionPanel.vue";
import { useAppState } from "../stores/appState";

beforeEach(() => {
  Object.defineProperty(HTMLDialogElement.prototype, "showModal", { configurable: true, value: vi.fn() });
  Object.defineProperty(HTMLDialogElement.prototype, "close", { configurable: true, value: vi.fn() });
});
afterEach(() => {
  Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
  Reflect.deleteProperty(HTMLDialogElement.prototype, "close");
});

async function mountPanel(mode: "lan" | "relay" = "relay") {
  const pinia = createTestingPinia({
    createSpy: vi.fn,
    initialState: { app: {
      relayConfig: { base_url: "ws://old:8080", session_id: "STAGE-A", secret: "secret" },
      relayStatus: "connected", relayAccessUrl: "http://old:8080/r/STAGE-A/presenter?token=old",
    } },
  });
  const wrapper = mount(ConnectionPanel, { props: { mode }, global: { plugins: [pinia] } });
  await flushPromises();
  return { wrapper, app: useAppState() };
}

it("saves the normalized draft without switching the running QR or connection", async () => {
  const { wrapper, app } = await mountPanel();
  await wrapper.get("#relay-server").setValue("https://new:443/");
  await wrapper.get(".primary").trigger("click");
  await flushPromises();
  expect(app.saveRelay).toHaveBeenCalledWith({ base_url: "ws://new:443", session_id: "STAGE-A", secret: "secret" });
  expect(app.relayAccessUrl).toBe("http://old:8080/r/STAGE-A/presenter?token=old");
  expect(app.relayStatus).toBe("connected");
  expect(wrapper.emitted("selectMode")).toBeUndefined();
  expect(wrapper.get('[role="status"]').text()).toContain("重启导播后生效");
  wrapper.unmount();
});

it("shows save failures and retains the draft for correction", async () => {
  const { wrapper, app } = await mountPanel();
  vi.mocked(app.saveRelay).mockRejectedValueOnce(new Error("磁盘不可写"));
  await wrapper.get(".primary").trigger("click");
  await flushPromises();
  expect(wrapper.get('[role="status"]').text()).toContain("磁盘不可写");
  expect((wrapper.get("#relay-session").element as HTMLInputElement).value).toBe("STAGE-A");
  expect(wrapper.emitted("close")).toBeUndefined();
  wrapper.unmount();
});

it("selects the LAN QR without disabling the relay configuration", async () => {
  const { wrapper, app } = await mountPanel("lan");
  await wrapper.get(".primary").trigger("click");
  expect(wrapper.emitted("selectMode")).toEqual([["lan"]]);
  expect(app.saveRelay).not.toHaveBeenCalled();
  wrapper.unmount();
});
