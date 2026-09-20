import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import Rc003InputControl from "./Rc003InputControl.vue";
import { getRc003InputStatus, rc003InputAvailable, startRc003Input, stopRc003Input, stoppedRc003InputStatus, type Rc003InputStatus } from "../lib/rc003-input";

vi.mock("../lib/rc003-input", async (original) => ({
  ...await original<typeof import("../lib/rc003-input")>(),
  getRc003InputStatus: vi.fn(), rc003InputAvailable: vi.fn(), startRc003Input: vi.fn(), stopRc003Input: vi.fn(),
}));
vi.mock("../lib/frontend-diagnostics", () => ({ reportFrontendEvent: vi.fn() }));

let wrapper: VueWrapper | undefined;
const snapshot = (phase: Rc003InputStatus["phase"], generation = 1): Rc003InputStatus => ({ ...stoppedRc003InputStatus(), phase, generation });
async function render(remoteModel = "rc003", connected = true): Promise<VueWrapper> {
  wrapper = mount(Rc003InputControl, { props: { remoteModel, connected } });
  await flushPromises();
  return wrapper;
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  vi.mocked(rc003InputAvailable).mockReturnValue(true);
  vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot("stopped"));
  vi.mocked(startRc003Input).mockResolvedValue(snapshot("waiting", 2));
  vi.mocked(stopRc003Input).mockResolvedValue(snapshot("stopped", 3));
});
afterEach(() => {
  wrapper?.unmount();
  wrapper = undefined;
  vi.useRealTimers();
});

describe("RC003 enhancement control", () => {
  it.each(["rc001", "unknown"])("is hidden and does not poll for %s", async (model) => {
    const view = await render(model);
    await vi.advanceTimersByTimeAsync(3000);
    expect(view.find("section").exists()).toBe(false);
    expect(getRc003InputStatus).not.toHaveBeenCalled();
    expect(startRc003Input).not.toHaveBeenCalled();
  });

  it("reads status without starting and requires a connected real runtime", async () => {
    const view = await render("rc003", false);
    const control = view.get('[role="switch"]');
    expect(control.element.tagName).toBe("BUTTON");
    expect(control.attributes("type")).toBe("button");
    expect(control.attributes("aria-label")).toBe("补齐返回、音量＋/－按键");
    expect(control.attributes("aria-checked")).toBe("false");
    const description = view.get(`[id="${control.attributes("aria-describedby")}"]`);
    expect(description.text()).toContain("需要管理员权限");
    expect(startRc003Input).not.toHaveBeenCalled();
    expect(view.get("button").attributes("disabled")).toBeDefined();
    await view.setProps({ connected: true });
    expect(view.get("button").attributes("disabled")).toBeUndefined();
    expect(view.text()).toContain("主程序保持普通权限");
    expect(view.text()).toContain("未配置动作的增强按键只显示高亮");
    expect(view.text()).toContain("按键动作以当前映射为准");
    expect(view.text()).not.toContain("原始行为");
  });

  it("does not poll or start from browser preview or simulation", async () => {
    vi.mocked(rc003InputAvailable).mockReturnValue(false);
    const view = await render();
    await view.get("button").trigger("click");
    await vi.advanceTimersByTimeAsync(3000);
    expect(view.text()).toContain("浏览器预览和仿真模式无法启用");
    expect(getRc003InputStatus).not.toHaveBeenCalled();
    expect(startRc003Input).not.toHaveBeenCalled();
  });

  it("starts only once per click sequence, explains neutral initialization, then polls and stops", async () => {
    let resolve!: (status: Rc003InputStatus) => void;
    vi.mocked(startRc003Input).mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const view = await render();
    const label = view.get('[role="switch"]').attributes("aria-label");
    await view.get("button").trigger("click");
    await view.get("button").trigger("click");
    expect(startRc003Input).toHaveBeenCalledTimes(1);
    expect(view.get("button").attributes("disabled")).toBeDefined();
    expect(view.get('[role="switch"]').attributes("aria-busy")).toBe("true");
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("false");
    resolve({ ...snapshot("waiting", 2), lastError: "awaiting_neutral" });
    await flushPromises();
    expect(view.text()).toContain("请按一下方向键并松开，完成首次初始化");
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("true");
    expect(view.get('[role="switch"]').attributes("aria-label")).toBe(label);
    expect(view.get('[role="status"]').text()).toBe("正在准备…");
    vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot("ready", 2));
    await vi.advanceTimersByTimeAsync(1000);
    expect(view.get('[role="status"]').text()).toBe("增强已就绪");
    await view.setProps({ connected: false });
    expect(view.get("button").attributes("disabled")).toBeUndefined();
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("true");
    expect(view.get('[role="status"]').text()).toBe("等待连接恢复");
    expect(view.text()).toContain("正在等待遥控器连接恢复");
    expect(view.text()).not.toContain("请先在「连接与语音」中连接");
    await view.get("button").trigger("click");
    await flushPromises();
    expect(stopRc003Input).toHaveBeenCalledTimes(1);
    expect(view.get('[role="status"]').text()).toBe("未启用");
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("false");
    expect(view.get('[role="switch"]').attributes("aria-label")).toBe(label);
  });

  it("ignores a late status read that predates a user command", async () => {
    const view = await render();
    let resolveRead!: (status: Rc003InputStatus) => void;
    vi.mocked(getRc003InputStatus).mockReturnValueOnce(new Promise((done) => { resolveRead = done; }));
    await vi.advanceTimersByTimeAsync(1000);
    await view.get("button").trigger("click");
    await flushPromises();
    resolveRead(snapshot("stopped"));
    await flushPromises();
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("true");
  });

  it("recovers from read failures without overlapping polls or starting automatically", async () => {
    vi.mocked(getRc003InputStatus).mockRejectedValueOnce(new Error("status unavailable"));
    const view = await render();
    expect(view.get('[role="alert"]').text()).toContain("正在重试");
    expect(view.get("button").attributes("disabled")).toBeDefined();
    let resolveRead!: (status: Rc003InputStatus) => void;
    vi.mocked(getRc003InputStatus).mockReturnValueOnce(new Promise((done) => { resolveRead = done; }));
    await vi.advanceTimersByTimeAsync(3000);
    expect(getRc003InputStatus).toHaveBeenCalledTimes(2);
    resolveRead(snapshot("stopped"));
    await flushPromises();
    expect(view.find('[role="alert"]').exists()).toBe(false);
    expect(view.get("button").attributes("disabled")).toBeUndefined();
    expect(startRc003Input).not.toHaveBeenCalled();
  });

  it("shows cancelled authorization as retryable without repeating the request", async () => {
    vi.mocked(startRc003Input).mockRejectedValueOnce(new Error("elevation_cancelled"));
    const view = await render();
    await view.get("button").trigger("click");
    await flushPromises();
    expect(view.get('[role="alert"]').text()).toContain("未获得 Windows 管理员授权");
    expect(view.get("button").attributes("disabled")).toBeUndefined();
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("false");
    await vi.advanceTimersByTimeAsync(2000);
    expect(startRc003Input).toHaveBeenCalledTimes(1);
  });

  it("keeps the switch on until normal stop completes and coalesces repeated clicks", async () => {
    vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot("ready"));
    let resolveStop!: (status: Rc003InputStatus) => void;
    vi.mocked(stopRc003Input).mockReturnValueOnce(new Promise(done => { resolveStop = done; }));
    const view = await render();
    await view.get('[role="switch"]').trigger("click");
    await view.get('[role="switch"]').trigger("click");
    await vi.advanceTimersByTimeAsync(2000);
    expect(stopRc003Input).toHaveBeenCalledTimes(1);
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("true");
    expect(view.get('[role="switch"]').attributes("disabled")).toBeDefined();
    expect(view.get('[role="switch"]').text()).toBe("正在关闭…");
    expect(getRc003InputStatus).toHaveBeenCalledTimes(1);
    resolveStop(snapshot("stopped", 2));
    await flushPromises();
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("false");
    expect(view.get('[role="switch"]').attributes("aria-busy")).toBe("false");
    expect(startRc003Input).not.toHaveBeenCalled();
  });

  it("shows a failed phase as off with a retryable error without restarting", async () => {
    vi.mocked(getRc003InputStatus).mockResolvedValue({ ...snapshot("failed"), lastError: "helper_exited" });
    const view = await render();
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("false");
    expect(view.get('[role="switch"]').attributes("disabled")).toBeUndefined();
    expect(view.get('[role="alert"]').text()).toContain("三键增强已退出");
    await vi.advanceTimersByTimeAsync(2000);
    expect(startRc003Input).not.toHaveBeenCalled();
  });

  it("allows stopping a starting enhancement without claiming it is ready", async () => {
    vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot("starting"));
    const view = await render();
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("true");
    expect(view.get('[role="status"]').text()).toBe("正在启用…");
    await view.get('[role="switch"]').trigger("click");
    await flushPromises();
    expect(stopRc003Input).toHaveBeenCalledTimes(1);
    expect(startRc003Input).not.toHaveBeenCalled();
  });

  it("cleans up polling on navigation without stopping the user's enhancement", async () => {
    vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot("ready"));
    await render();
    wrapper!.unmount();
    wrapper = undefined;
    await vi.advanceTimersByTimeAsync(3000);
    expect(getRc003InputStatus).toHaveBeenCalledTimes(1);
    expect(stopRc003Input).not.toHaveBeenCalled();
  });
});
