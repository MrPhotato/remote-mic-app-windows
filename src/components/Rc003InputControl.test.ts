import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import Rc003InputControl from "./Rc003InputControl.vue";
import { getRc003InputStatus, rc003InputAvailable, startRc003Input, stopRc003Input, stoppedRc003InputStatus, type Rc003InputStatus } from "../lib/rc003-input";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";

vi.mock("../lib/rc003-input", async (original) => ({
  ...await original<typeof import("../lib/rc003-input")>(),
  getRc003InputStatus: vi.fn(), rc003InputAvailable: vi.fn(), startRc003Input: vi.fn(), stopRc003Input: vi.fn(),
}));
vi.mock("../lib/frontend-diagnostics", () => ({ reportFrontendEvent: vi.fn() }));

let wrapper: VueWrapper | undefined;
const hosts: HTMLElement[] = [];
const originalShowModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal");
const originalClose = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "close");
const showModal = vi.fn(function (this: HTMLDialogElement) {
  if (!this.isConnected) throw new DOMException("Dialog is detached", "InvalidStateError");
  this.open = true;
});
const closeDialog = vi.fn(function (this: HTMLDialogElement) {
  if (!this.open) return;
  this.open = false;
  this.dispatchEvent(new Event("close"));
});
let nextNeutralGeneration = 100;
const snapshot = (phase: Rc003InputStatus["phase"], generation = 1): Rc003InputStatus => ({ ...stoppedRc003InputStatus(), phase, generation });
const neutralSnapshot = (generation: number): Rc003InputStatus => ({ ...snapshot("waiting", generation), lastError: "awaiting_neutral" });
async function render(remoteModel = "rc003", connected = true): Promise<VueWrapper> {
  const host = document.createElement("div");
  document.body.appendChild(host);
  hosts.push(host);
  wrapper = mount(Rc003InputControl, { props: { remoteModel, connected }, attachTo: host });
  await flushPromises();
  return wrapper;
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  showModal.mockImplementation(function (this: HTMLDialogElement) {
    if (!this.isConnected) throw new DOMException("Dialog is detached", "InvalidStateError");
    this.open = true;
  });
  closeDialog.mockImplementation(function (this: HTMLDialogElement) {
    if (!this.open) return;
    this.open = false;
    this.dispatchEvent(new Event("close"));
  });
  Object.defineProperty(HTMLDialogElement.prototype, "showModal", { configurable: true, writable: true, value: showModal });
  Object.defineProperty(HTMLDialogElement.prototype, "close", { configurable: true, writable: true, value: closeDialog });
  vi.mocked(rc003InputAvailable).mockReturnValue(true);
  vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot("stopped"));
  vi.mocked(startRc003Input).mockResolvedValue(snapshot("waiting", 2));
  vi.mocked(stopRc003Input).mockResolvedValue(snapshot("stopped", 3));
});
afterEach(() => {
  wrapper?.unmount();
  wrapper = undefined;
  for (const host of hosts.splice(0)) host.remove();
  if (originalShowModal) Object.defineProperty(HTMLDialogElement.prototype, "showModal", originalShowModal);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
  if (originalClose) Object.defineProperty(HTMLDialogElement.prototype, "close", originalClose);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, "close");
  vi.useRealTimers();
});

describe("RC003 enhancement control", () => {
  it("shows one accessible neutral reminder per generation instead of reopening on every poll", async () => {
    const generation = ++nextNeutralGeneration;
    vi.mocked(getRc003InputStatus).mockResolvedValue(neutralSnapshot(generation));
    const view = await render();
    const dialog = view.get(".neutral-dialog");
    expect((dialog.element as HTMLDialogElement).open).toBe(true);
    expect(showModal).toHaveBeenCalledOnce();
    expect(dialog.get(`[id="${dialog.attributes("aria-labelledby")}"]`).text()).toBe("按一下方向键");
    expect(dialog.get(`[id="${dialog.attributes("aria-describedby")}"]`).text()).toContain("按遥控器任意方向键，然后松开");
    expect(view.get(".neutral-inline").text()).toContain("请按一下方向键并松开");
    await vi.advanceTimersByTimeAsync(3000);
    expect(showModal).toHaveBeenCalledOnce();
    expect(startRc003Input).not.toHaveBeenCalled();
    expect(stopRc003Input).not.toHaveBeenCalled();
    expect(reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "rc003_neutral_prompt", phase: "shown", result: "passed" }));
  });

  it("dismisses with Later or Escape without stopping enhancement and allows manual reopening", async () => {
    vi.mocked(getRc003InputStatus).mockResolvedValue(neutralSnapshot(++nextNeutralGeneration));
    const view = await render();
    const dialog = view.get(".neutral-dialog").element as HTMLDialogElement;
    await view.get(".neutral-dialog button").trigger("click");
    expect(dialog.open).toBe(false);
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("true");
    await vi.advanceTimersByTimeAsync(3000);
    expect(showModal).toHaveBeenCalledOnce();
    await view.get(".neutral-inline button").trigger("click");
    expect(dialog.open).toBe(true);
    expect(showModal).toHaveBeenCalledTimes(2);
    const escape = new Event("cancel", { cancelable: true });
    dialog.dispatchEvent(escape);
    await flushPromises();
    expect(escape.defaultPrevented).toBe(true);
    expect(dialog.open).toBe(false);
    await vi.advanceTimersByTimeAsync(2000);
    expect(showModal).toHaveBeenCalledTimes(2);
    expect(startRc003Input).not.toHaveBeenCalled();
    expect(stopRc003Input).not.toHaveBeenCalled();
    expect(reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "rc003_neutral_prompt", phase: "hidden", result: "passed" }));
  });

  it("does not automatically repeat a reminder when navigating to another page with the same generation", async () => {
    vi.mocked(getRc003InputStatus).mockResolvedValue(neutralSnapshot(++nextNeutralGeneration));
    const first = await render();
    const oldDialog = first.get(".neutral-dialog").element as HTMLDialogElement;
    first.unmount();
    wrapper = undefined;
    expect(oldDialog.open).toBe(false);
    const nextPage = await render();
    expect((nextPage.get(".neutral-dialog").element as HTMLDialogElement).open).toBe(false);
    expect(nextPage.get(".neutral-inline").text()).toContain("查看提示");
    await vi.advanceTimersByTimeAsync(2000);
    expect(showModal).toHaveBeenCalledOnce();
    await nextPage.get(".neutral-inline button").trigger("click");
    expect(showModal).toHaveBeenCalledTimes(2);
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it("hides on read errors without repeating the same-generation reminder on recovery", async () => {
    const generation = ++nextNeutralGeneration;
    vi.mocked(getRc003InputStatus).mockResolvedValue(neutralSnapshot(generation));
    const view = await render();
    const dialog = view.get(".neutral-dialog").element as HTMLDialogElement;
    vi.mocked(getRc003InputStatus).mockRejectedValueOnce(new Error("status unavailable"));
    await vi.advanceTimersByTimeAsync(1000);
    expect(dialog.open).toBe(false);
    expect(view.get('[role="alert"]').text()).toContain("正在重试");
    await vi.advanceTimersByTimeAsync(2000);
    expect(view.find('[role="alert"]').exists()).toBe(false);
    expect(dialog.open).toBe(false);
    expect(showModal).toHaveBeenCalledOnce();
    vi.mocked(getRc003InputStatus).mockResolvedValue(neutralSnapshot(++nextNeutralGeneration));
    await vi.advanceTimersByTimeAsync(1000);
    expect(dialog.open).toBe(true);
    expect(showModal).toHaveBeenCalledTimes(2);
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it.each(["ready", "failed", "stopped"] as const)("hides when enhancement becomes %s and reopens for a new waiting generation", async phase => {
    const generation = ++nextNeutralGeneration;
    vi.mocked(getRc003InputStatus).mockResolvedValue(neutralSnapshot(generation));
    const view = await render();
    const dialog = view.get(".neutral-dialog").element as HTMLDialogElement;
    vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot(phase, generation));
    await vi.advanceTimersByTimeAsync(1000);
    expect(dialog.open).toBe(false);
    expect(view.find(".neutral-inline").exists()).toBe(false);
    vi.mocked(getRc003InputStatus).mockResolvedValue(neutralSnapshot(++nextNeutralGeneration));
    await vi.advanceTimersByTimeAsync(1000);
    expect(dialog.open).toBe(true);
    expect(showModal).toHaveBeenCalledTimes(2);
    expect(startRc003Input).not.toHaveBeenCalled();
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it.each([
    { label: "disconnect", props: { connected: false } },
    { label: "unknown model", props: { remoteModel: "unknown" } },
    { label: "RC001", props: { remoteModel: "rc001" } },
  ])("hides the neutral reminder after $label without controlling the helper", async ({ props }) => {
    vi.mocked(getRc003InputStatus).mockResolvedValue(neutralSnapshot(++nextNeutralGeneration));
    const view = await render();
    const dialog = view.get(".neutral-dialog").element as HTMLDialogElement;
    await view.setProps(props);
    expect(!dialog.isConnected || !dialog.open).toBe(true);
    await vi.advanceTimersByTimeAsync(2000);
    expect(showModal).toHaveBeenCalledOnce();
    expect(startRc003Input).not.toHaveBeenCalled();
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it("ignores a pending start result after the selected model changed away and back", async () => {
    let resolveStart!: (next: Rc003InputStatus) => void;
    vi.mocked(startRc003Input).mockReturnValueOnce(new Promise(resolve => { resolveStart = resolve; }));
    const view = await render();
    await view.get('[role="switch"]').trigger("click");
    await view.setProps({ remoteModel: "unknown" });
    await view.setProps({ remoteModel: "rc003" });
    resolveStart(neutralSnapshot(++nextNeutralGeneration));
    await flushPromises();
    expect(showModal).not.toHaveBeenCalled();
    expect(view.find(".neutral-inline").exists()).toBe(false);
    await vi.advanceTimersByTimeAsync(1000);
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("false");
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it("ignores an older poll even when it carries the same generation as a newer command", async () => {
    const generation = ++nextNeutralGeneration;
    const view = await render();
    let resolveRead!: (next: Rc003InputStatus) => void;
    vi.mocked(getRc003InputStatus).mockReturnValueOnce(new Promise(resolve => { resolveRead = resolve; }));
    await vi.advanceTimersByTimeAsync(1000);
    vi.mocked(startRc003Input).mockResolvedValueOnce(snapshot("ready", generation));
    await view.get('[role="switch"]').trigger("click");
    await flushPromises();
    resolveRead(neutralSnapshot(generation));
    await flushPromises();
    expect(view.get('[role="status"]').text()).toBe("增强已就绪");
    expect(showModal).not.toHaveBeenCalled();
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it("does not show a late initial result after unmounting", async () => {
    let resolveRead!: (next: Rc003InputStatus) => void;
    vi.mocked(getRc003InputStatus).mockReturnValueOnce(new Promise(resolve => { resolveRead = resolve; }));
    const view = await render();
    view.unmount();
    wrapper = undefined;
    resolveRead(neutralSnapshot(++nextNeutralGeneration));
    await flushPromises();
    await vi.advanceTimersByTimeAsync(2000);
    expect(showModal).not.toHaveBeenCalled();
    expect(getRc003InputStatus).toHaveBeenCalledOnce();
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it("keeps inline help usable when the host refuses a dialog and allows a manual retry", async () => {
    showModal.mockImplementationOnce(() => { throw new DOMException("Dialog unavailable", "InvalidStateError"); });
    vi.mocked(getRc003InputStatus).mockResolvedValue(neutralSnapshot(++nextNeutralGeneration));
    const view = await render();
    const dialog = view.get(".neutral-dialog").element as HTMLDialogElement;
    expect(dialog.open).toBe(false);
    expect(view.get(".neutral-inline").text()).toContain("请按一下方向键并松开");
    expect(reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "rc003_neutral_prompt", phase: "shown", result: "failed", reason: "dialog_unavailable" }));
    await vi.advanceTimersByTimeAsync(2000);
    expect(showModal).toHaveBeenCalledOnce();
    await view.get(".neutral-inline button").trigger("click");
    expect(dialog.open).toBe(true);
    expect(showModal).toHaveBeenCalledTimes(2);
    expect(startRc003Input).not.toHaveBeenCalled();
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it("is hidden and does not poll for confirmed RC001", async () => {
    const view = await render("rc001");
    await vi.advanceTimersByTimeAsync(3000);
    expect(view.find("section").exists()).toBe(false);
    expect(getRc003InputStatus).not.toHaveBeenCalled();
    expect(startRc003Input).not.toHaveBeenCalled();
  });

  it("keeps unknown models visible and disabled until RC003 is confirmed", async () => {
    const view = await render("unknown");
    expect(view.find("section").exists()).toBe(true);
    expect(view.get('[role="status"]').text()).toBe("型号尚未确认");
    expect(view.text()).toContain("型号确认后会自动更新");
    expect(view.get('[role="switch"]').attributes("disabled")).toBeDefined();
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("false");
    await view.get('[role="switch"]').trigger("click");
    await vi.advanceTimersByTimeAsync(3000);
    expect(getRc003InputStatus).not.toHaveBeenCalled();
    expect(startRc003Input).not.toHaveBeenCalled();
    await view.setProps({ remoteModel: "rc003" });
    await flushPromises();
    expect(getRc003InputStatus).toHaveBeenCalledTimes(1);
    expect(view.get('[role="switch"]').attributes("disabled")).toBeUndefined();
    expect(startRc003Input).not.toHaveBeenCalled();
  });

  it("does not treat generic or unknown devices as RC003 after a late status read", async () => {
    let resolve!: (status: Rc003InputStatus) => void;
    vi.mocked(getRc003InputStatus).mockReturnValueOnce(new Promise(done => { resolve = done; }));
    const view = await render("rc003");
    await view.setProps({ remoteModel: "unknown" });
    resolve(snapshot("ready", 3));
    await flushPromises();
    expect(view.get('[role="status"]').text()).toBe("型号尚未确认");
    expect(view.get('[role="switch"]').attributes("disabled")).toBeDefined();
    expect(view.get('[role="switch"]').attributes("aria-checked")).toBe("false");
    await vi.advanceTimersByTimeAsync(3000);
    expect(getRc003InputStatus).toHaveBeenCalledTimes(1);
    expect(startRc003Input).not.toHaveBeenCalled();
    await view.setProps({ remoteModel: "rc001" });
    expect(view.find("section").exists()).toBe(false);
  });

  it("reads status without starting and requires a connected real runtime", async () => {
    const view = await render("rc003", false);
    const control = view.get('[role="switch"]');
    expect(control.element.tagName).toBe("BUTTON");
    expect(control.attributes("type")).toBe("button");
    expect(control.attributes("aria-label")).toBe("RC003 按键增强");
    expect(control.attributes("aria-checked")).toBe("false");
    const description = view.get(`[id="${control.attributes("aria-describedby")}"]`);
    expect(description.text()).toContain("需要管理员权限");
    expect(startRc003Input).not.toHaveBeenCalled();
    expect(view.get("button").attributes("disabled")).toBeDefined();
    await view.setProps({ connected: true });
    expect(view.get("button").attributes("disabled")).toBeUndefined();
    expect(view.text()).toContain("需要管理员权限");
    expect(view.text()).toContain("每次启动无线麦后需手动开启");
    expect(view.text()).toContain("让确认键长按只执行设定动作");
    expect(view.text()).not.toContain("原始行为");
  });

  it("does not poll or start from browser preview or simulation", async () => {
    vi.mocked(rc003InputAvailable).mockReturnValue(false);
    const view = await render();
    await view.get("button").trigger("click");
    await vi.advanceTimersByTimeAsync(3000);
    expect(view.text()).toContain("请在 Windows 应用中启用按键增强");
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
    expect(view.text()).toContain("请按一下方向键并松开");
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
    expect(view.get('[role="alert"]').text()).toContain("按键增强已退出");
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
