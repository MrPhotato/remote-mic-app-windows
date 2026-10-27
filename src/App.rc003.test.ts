import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import App from "./App.vue";
import Rc003InputControl from "./components/Rc003InputControl.vue";
import { getButtonMappings, getRuntimeSnapshot, type RuntimeSnapshot } from "./lib/bridge";
import { getRc003InputStatus, startRc003Input, stopRc003Input, stoppedRc003InputStatus, type Rc003InputStatus } from "./lib/rc003-input";

vi.mock("./lib/bridge", async original => ({
  ...await original<typeof import("./lib/bridge")>(),
  getRuntimeSnapshot: vi.fn(),
  getButtonMappings: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));
vi.mock("./lib/rc003-input", async original => ({
  ...await original<typeof import("./lib/rc003-input")>(),
  getRc003InputStatus: vi.fn(), startRc003Input: vi.fn(), stopRc003Input: vi.fn(),
  rc003InputAvailable: vi.fn(() => true),
}));
vi.mock("./lib/release-check-state", () => ({ useReleaseCheck: () => ({ result: { value: null }, checkAtStartup: vi.fn() }) }));
vi.mock("./lib/frontend-diagnostics", () => ({ reportFrontendEvent: vi.fn() }));

let wrapper: VueWrapper | undefined;
let host: HTMLElement | undefined;
const originalShowModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal");
const originalClose = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "close");
const showModal = vi.fn(function (this: HTMLDialogElement) { this.open = true; });
const snapshot = (phase: Rc003InputStatus["phase"], generation = 1): Rc003InputStatus => ({
  ...stoppedRc003InputStatus(), phase, generation,
  lastError: phase === "waiting" ? "awaiting_neutral" : null,
});

async function render() {
  host = document.createElement("div");
  document.body.appendChild(host);
  wrapper = mount(App, {
    attachTo: host,
    global: { stubs: {
      Transition: false,
      WindowTitlebar: true,
      CodingPage: { template: '<section data-page="coding">Coding</section>' },
      AboutPage: { template: '<section data-page="about">About</section>' },
      ButtonsPage: {
        props: ["initialMappings", "runtime"],
        components: { Rc003InputControl },
        template: '<section data-page="buttons"><Rc003InputControl remote-model="rc003" :connected="true" /></section>',
      },
    } },
  });
  await flushPromises();
  return wrapper;
}

async function navigate(label: string) {
  await wrapper!.findAll(".nav-item").find(item => item.text() === label)!.trigger("click");
  await flushPromises();
  await vi.advanceTimersByTimeAsync(80);
  await flushPromises();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  localStorage.clear();
  Object.defineProperty(HTMLDialogElement.prototype, "showModal", { configurable: true, value: showModal });
  Object.defineProperty(HTMLDialogElement.prototype, "close", { configurable: true, value: function (this: HTMLDialogElement) { this.open = false; } });
  vi.mocked(getRuntimeSnapshot).mockResolvedValue({
    appVersion: "0.3.14",
    platform: { connection: { remoteModel: "rc003", phase: "ready", generation: 1, voiceState: "idle" } },
  } as RuntimeSnapshot);
  vi.mocked(getButtonMappings).mockResolvedValue({ enabled: true, actions: {} });
  vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot("stopped"));
});

afterEach(() => {
  wrapper?.unmount();
  wrapper = undefined;
  host?.remove();
  if (originalShowModal) Object.defineProperty(HTMLDialogElement.prototype, "showModal", originalShowModal);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
  if (originalClose) Object.defineProperty(HTMLDialogElement.prototype, "close", originalClose);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, "close");
  vi.useRealTimers();
});

describe("application-wide RC003 reminder", () => {
  it("receives authorization after leaving Buttons, keeps one monitor and closes on ready", async () => {
    let resolveStart!: (next: Rc003InputStatus) => void;
    vi.mocked(startRc003Input).mockReturnValueOnce(new Promise(resolve => { resolveStart = resolve; }));
    const view = await render();
    await navigate("按键");
    expect(view.find('[data-page="buttons"]').exists()).toBe(true);
    await view.get('[role="switch"]').trigger("click");
    await navigate("关于");
    expect(view.find('[data-page="buttons"]').exists()).toBe(false);
    resolveStart(snapshot("waiting", 2));
    await flushPromises();
    const dialog = view.get(".neutral-dialog").element as HTMLDialogElement;
    expect(dialog.open).toBe(true);
    expect(view.findAll(".neutral-dialog")).toHaveLength(1);
    vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot("ready", 2));
    const reads = vi.mocked(getRc003InputStatus).mock.calls.length;
    await vi.advanceTimersByTimeAsync(1000);
    expect(getRc003InputStatus).toHaveBeenCalledTimes(reads + 1);
    expect(dialog.open).toBe(false);
    await navigate("按键");
    expect(view.get('[role="status"]').text()).toBe("增强已就绪");
    expect(showModal).toHaveBeenCalledOnce();
    expect(getButtonMappings).toHaveBeenCalledTimes(2);
    expect(startRc003Input).toHaveBeenCalledOnce();
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it("shows a waiting reminder outside Buttons and hides when runtime cannot be read", async () => {
    vi.mocked(getRc003InputStatus).mockResolvedValue(snapshot("waiting", 3));
    const view = await render();
    const dialog = view.get(".neutral-dialog").element as HTMLDialogElement;
    expect(view.find('[data-page="coding"]').exists()).toBe(true);
    expect(view.find('[role="switch"]').exists()).toBe(false);
    expect(dialog.open).toBe(true);
    vi.mocked(getRuntimeSnapshot).mockRejectedValueOnce(new Error("unavailable"));
    await vi.advanceTimersByTimeAsync(1000);
    expect(dialog.open).toBe(false);
    await vi.advanceTimersByTimeAsync(1000);
    expect(showModal).toHaveBeenCalledOnce();
    expect(startRc003Input).not.toHaveBeenCalled();
    expect(stopRc003Input).not.toHaveBeenCalled();
  });

  it("keeps the latest prepared Buttons page when returning during another page's leave", async () => {
    const view = await render();
    await navigate("按键");
    await view.findAll(".nav-item").find(item => item.text() === "关于")!.trigger("click");
    await view.findAll(".nav-item").find(item => item.text() === "按键")!.trigger("click");
    await flushPromises();
    await vi.advanceTimersByTimeAsync(100);
    await flushPromises();
    expect(view.find('[data-page="buttons"]').exists()).toBe(true);
    expect(view.find('[data-page="about"]').exists()).toBe(false);
    expect(getButtonMappings).toHaveBeenCalledTimes(2);
    expect(view.findAll(".neutral-dialog")).toHaveLength(1);
    expect(startRc003Input).not.toHaveBeenCalled();
    expect(stopRc003Input).not.toHaveBeenCalled();
  });
});
