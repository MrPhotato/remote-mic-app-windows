// @vitest-environment jsdom
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriRuntime } from "../lib/bridge";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";
import WindowTitlebar from "./WindowTitlebar.vue";

const native = vi.hoisted(() => ({
  minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn(),
  isMaximized: vi.fn(), isFocused: vi.fn(), onResized: vi.fn(), onFocusChanged: vi.fn(),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn(() => native) }));
vi.mock("../lib/bridge", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("../lib/frontend-diagnostics", () => ({ reportFrontendEvent: vi.fn() }));
let wrapper: VueWrapper | undefined;
let resize: () => void;
let focus: (event: { payload: boolean }) => void;
const unlistenResize = vi.fn();
const unlistenFocus = vi.fn();

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(isTauriRuntime).mockReturnValue(true);
  vi.mocked(getCurrentWindow).mockReturnValue(native as unknown as ReturnType<typeof getCurrentWindow>);
  native.isMaximized.mockResolvedValue(false);
  native.isFocused.mockResolvedValue(true);
  native.onResized.mockImplementation(async (callback) => { resize = callback; return unlistenResize; });
  native.onFocusChanged.mockImplementation(async (callback) => { focus = callback; return unlistenFocus; });
});
afterEach(() => { wrapper?.unmount(); wrapper = undefined; });

describe("液态玻璃窗口控制", () => {
  it("最小化与关闭走原生请求，关闭仍交给现有托盘处理", async () => {
    wrapper = mount(WindowTitlebar); await flushPromises();
    await wrapper.get('[aria-label="最小化"]').trigger("click"); await flushPromises();
    await wrapper.get('[aria-label="关闭窗口"]').trigger("click"); await flushPromises();
    expect(native.minimize).toHaveBeenCalledOnce();
    expect(native.close).toHaveBeenCalledOnce();
    expect(wrapper.get('[aria-label="关闭窗口"]').attributes("title")).toBe("关闭到托盘");
  });
  it("最大化与外部缩放事件都读取实际状态", async () => {
    wrapper = mount(WindowTitlebar); await flushPromises();
    native.isMaximized.mockResolvedValue(true);
    await wrapper.get('[aria-label="最大化"]').trigger("click"); await flushPromises();
    expect(native.toggleMaximize).toHaveBeenCalledOnce();
    expect(wrapper.find('[aria-label="还原窗口"]').exists()).toBe(true);
    native.isMaximized.mockResolvedValue(false);
    resize(); await flushPromises();
    expect(wrapper.find('[aria-label="最大化"]').exists()).toBe(true);
  });
  it("原生请求失败明确提示并允许重试", async () => {
    native.minimize.mockRejectedValueOnce(new Error("failure"));
    wrapper = mount(WindowTitlebar); await flushPromises();
    await wrapper.get('[aria-label="最小化"]').trigger("click"); await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("窗口操作未完成");
    expect(reportFrontendEvent).toHaveBeenLastCalledWith(expect.objectContaining({ phase: "minimize", result: "failed" }));
    await wrapper.get('[aria-label="最小化"]').trigger("click"); await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
  });
  it("未返回的初始焦点查询不能覆盖更新的焦点事件", async () => {
    let resolve!: (focused: boolean) => void;
    native.isFocused.mockReturnValue(new Promise(r => { resolve = r; }));
    wrapper = mount(WindowTitlebar); await flushPromises();
    focus({ payload: false });
    resolve(true); await flushPromises();
    expect(wrapper.classes()).toContain("inactive");
  });
  it("卸载清理已注册和迟到的事件监听", async () => {
    let finish!: (callback: () => void) => void;
    native.onResized.mockReturnValue(new Promise(r => { finish = r; }));
    wrapper = mount(WindowTitlebar); await flushPromises();
    wrapper.unmount(); wrapper = undefined;
    finish(unlistenResize); await flushPromises();
    expect(unlistenResize).toHaveBeenCalledOnce();
    expect(unlistenFocus).toHaveBeenCalledOnce();
  });
  it("浏览器预览不操作真实窗口", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    wrapper = mount(WindowTitlebar); await flushPromises();
    expect(getCurrentWindow).not.toHaveBeenCalled();
    expect(wrapper.findAll("button").every(button => button.attributes("disabled") !== undefined)).toBe(true);
  });
  it("拖动区域不包含按钮，使用 Tauri 自带的双击逻辑", () => {
    wrapper = mount(WindowTitlebar);
    expect(wrapper.find(".window-drag-area[data-tauri-drag-region]").exists()).toBe(true);
    expect(wrapper.find("[data-tauri-drag-region] button").exists()).toBe(false);
  });
});
