import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import App from "./App.vue";
import { getButtonMappings, type ButtonMappings } from "./lib/bridge";

vi.mock("./lib/bridge", async (original) => ({
  ...await original<typeof import("./lib/bridge")>(),
  getRuntimeSnapshot: vi.fn(async () => null),
  getButtonMappings: vi.fn(),
}));

const saved: ButtonMappings = { enabled: true, actions: { ok: {
  single: { type: "shortcut", chord: { keys: ["enter"] } },
  double: { type: "disabled" }, long: { type: "disabled" },
} } };
let wrapper: VueWrapper | undefined;
function mountApp() {
  wrapper = mount(App, { global: { stubs: {
    CodingPage: { template: '<section data-page="coding">Current page</section>' },
    AboutPage: { template: '<section data-page="about">About</section>' },
    ButtonsPage: { props: ["initialMappings"], template: '<section data-page="buttons">{{ initialMappings.actions.ok.single.chord.keys[0] }}</section>' },
  } } });
  return wrapper;
}
function select(page: string) {
  return wrapper!.findAll(".nav-item").find(button => button.text() === page)!.trigger("click");
}
function deferRead() {
  let resolve!: (value: ButtonMappings) => void;
  vi.mocked(getButtonMappings).mockImplementationOnce(() => new Promise(r => { resolve = r; }));
  return (value = saved) => resolve(value);
}
beforeEach(() => {
  localStorage.clear();
  vi.mocked(getButtonMappings).mockReset().mockResolvedValue(saved);
});
afterEach(() => { wrapper?.unmount(); wrapper = undefined; });

describe("prepared mapping page navigation", () => {
  it("keeps the current page until a complete configuration can be shown", async () => {
    const resolve = deferRead();
    mountApp();
    await select("按键");
    await flushPromises();
    expect(wrapper!.find('[data-page="coding"]').exists()).toBe(true);
    expect(wrapper!.find('[data-page="buttons"]').exists()).toBe(false);
    await select("按键");
    expect(getButtonMappings).toHaveBeenCalledOnce();
    resolve();
    await flushPromises();
    expect(wrapper!.get('[data-page="buttons"]').text()).toBe("enter");
    expect(wrapper!.find('[data-page="coding"]').exists()).toBe(false);
  });

  it("ignores a late read after the user selects a different page", async () => {
    const resolve = deferRead();
    mountApp();
    await select("按键");
    await select("关于");
    resolve();
    await flushPromises();
    expect(wrapper!.find('[data-page="about"]').exists()).toBe(true);
    expect(wrapper!.find('[data-page="buttons"]').exists()).toBe(false);
    expect(localStorage.getItem("sayall.activePage")).toBe("about");
  });

  it("reads fresh settings each time the user returns to the page", async () => {
    mountApp();
    await select("按键"); await flushPromises();
    await select("关于");
    const changed = structuredClone(saved);
    changed.actions.ok!.single = { type: "shortcut", chord: { keys: ["escape"] } };
    vi.mocked(getButtonMappings).mockResolvedValueOnce(changed);
    await select("按键"); await flushPromises();
    expect(wrapper!.get('[data-page="buttons"]').text()).toBe("escape");
    expect(getButtonMappings).toHaveBeenCalledTimes(2);
  });

  it("keeps the previous page on failure and allows retry", async () => {
    vi.mocked(getButtonMappings).mockRejectedValueOnce(new Error("failed"));
    mountApp();
    await select("按键"); await flushPromises();
    expect(wrapper!.find('[data-page="coding"]').exists()).toBe(true);
    expect(wrapper!.get('[role="alert"]').text()).toContain("按键配置读取失败");
    await wrapper!.get('[role="alert"] button').trigger("click"); await flushPromises();
    expect(wrapper!.get('[data-page="buttons"]').text()).toBe("enter");
  });

  it("restores a saved buttons page without rendering an empty configuration", async () => {
    localStorage.setItem("sayall.activePage", "buttons");
    const resolve = deferRead();
    mountApp(); await flushPromises();
    expect(wrapper!.find('[data-page="buttons"]').exists()).toBe(false);
    expect(wrapper!.find('[data-page="coding"]').exists()).toBe(false);
    resolve(); await flushPromises();
    expect(wrapper!.get('[data-page="buttons"]').text()).toBe("enter");
  });

  it("does not navigate or persist a result after the app is unmounted", async () => {
    const resolve = deferRead();
    mountApp(); await select("按键");
    wrapper!.unmount(); wrapper = undefined;
    resolve(); await flushPromises();
    expect(localStorage.getItem("sayall.activePage")).toBeNull();
  });
});
