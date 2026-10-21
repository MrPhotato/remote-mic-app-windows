import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import ButtonsPage from "./ButtonsPage.vue";

type EdgeHandler = (edge: { button: string; isPressed: boolean }) => void;
type GestureHandler = (gesture: { button: string; trigger: string }) => void;
type ShortcutCaptureHandler = (edge: { key: string; isPressed: boolean }) => void;

let edgeHandler: EdgeHandler | null = null;
let gestureHandler: GestureHandler | null = null;
let shortcutCaptureHandler: ShortcutCaptureHandler | null = null;

vi.mock("../lib/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/bridge")>();
  return {
    ...actual,
    listPresetApps: vi.fn(async () => []),
    getButtonMappings: vi.fn(async () => ({
      enabled: true,
      actions: {
        ok: {
          single: { type: "shortcut", chord: { keys: ["enter"] } },
          double: { type: "disabled" },
          long: { type: "disabled" },
        },
      },
    })),
    getButtonMappingSnapshot: vi.fn(async () => ({
      enabled: true,
      gateActive: true,
      listenerActive: true,
      swallowedEdges: 3,
      leakedDowns: 0,
      firedGestures: 1,
      lastFired: null,
      lastError: null,
    })),
    saveButtonMappings: vi.fn(async (mappings: unknown) => mappings),
    exportButtonMappingConfiguration: vi.fn(async () => true),
    importButtonMappingConfiguration: vi.fn(async () => ({
      enabled: false,
      actions: {
        power: {
          single: { type: "shortcut", chord: { keys: ["escape"] } },
          double: { type: "disabled" },
          long: { type: "disabled" },
        },
      },
    })),
    resetButtonMappings: vi.fn(async () => ({
      enabled: true,
      actions: { back: { single: { type: "normal_backspace" }, double: { type: "disabled" }, long: { type: "disabled" } } },
    })),
    scanRegisteredApps: vi.fn(async () => [
      { name: "Registered Example", path: "shell:AppsFolder\\Example!App" },
    ]),
    testButtonMapping: vi.fn(async () => ({
      available: true,
      submittedBatches: 1,
      submittedEvents: 2,
      lastError: null,
    })),
    subscribeButtonEdges: vi.fn(async (handler: EdgeHandler) => {
      edgeHandler = handler;
      return () => {};
    }),
    subscribeButtonGestures: vi.fn(async (handler: GestureHandler) => {
      gestureHandler = handler;
      return () => {};
    }),
    startShortcutCapture: vi.fn(async () => undefined),
    stopShortcutCapture: vi.fn(async () => undefined),
    subscribeShortcutCaptureEdges: vi.fn(async (handler: ShortcutCaptureHandler) => {
      shortcutCaptureHandler = handler;
      return () => {};
    }),
  };
});

import {
  exportButtonMappingConfiguration,
  getButtonMappings,
  listPresetApps,
  importButtonMappingConfiguration,
  subscribeButtonEdges,
  subscribeButtonGestures,
  saveButtonMappings,
  startShortcutCapture,
  stopShortcutCapture,
  testButtonMapping,
} from "../lib/bridge";
import type { ButtonAction, ButtonMappings, RuntimeSnapshot } from "../lib/bridge";
import { ACTION_LIBRARY_STORAGE_KEY, readActionLibrary } from "../lib/action-library";

const runtime: RuntimeSnapshot = {
  appVersion: "0.1.0",
  platform: {
    platform: "windows",
    windowsApiAvailable: true,
    bleScanAvailable: true,
    bleVoiceReady: true,
    wasapiReady: false,
    rawInputReady: true,
    sendInputReady: true,
    verificationStatus: "测试",
    connection: {
      phase: "ready",
      remoteName: "小米蓝牙语音遥控器",
      remoteModel: "rc003",
      capabilities: null,
      voiceState: "idle",
      decodedSamples: 0,
      generation: 0,
      reconnectAttempt: 0,
      powerNotificationsAvailable: false,
      lastError: null,
    },
    audio: {
      phase: "unsupported",
      selectedEndpointId: null,
      selectedEndpointName: null,
      queuedSamples: 0,
      submittedSamples: 0,
      generation: 0,
      lastError: null,
    },
    rawInput: {
      phase: "ready",
      matchedDeviceCount: 1,
      rawEventCount: 0,
      semanticEdgeCount: 0,
      lastButton: null,
      lastIsPressed: null,
      activeButtons: [],
      lastError: null,
    },
    buttonMapping: {
      enabled: true,
      gateActive: true,
      listenerActive: true,
      swallowedEdges: 3,
      leakedDowns: 0,
      firedGestures: 1,
      lastFired: null,
      lastError: null,
    },
  },
};

async function mountPage(model: "rc001" | "rc003" | "unknown" = "rc003"): Promise<VueWrapper> {
  const snapshot =
    model === "rc003"
      ? runtime
      : {
          ...runtime,
          platform: {
            ...runtime.platform,
            connection: { ...runtime.platform.connection, remoteModel: model },
          },
        };
  const wrapper = mount(ButtonsPage, { props: { runtime: snapshot } });
  await vi.waitFor(() => {
    if (!edgeHandler || !gestureHandler) throw new Error("事件订阅未完成");
  });
  return wrapper;
}

beforeEach(() => {
  localStorage.clear();
  edgeHandler = null;
  gestureHandler = null;
  shortcutCaptureHandler = null;
  vi.mocked(getButtonMappings).mockClear();
  vi.mocked(subscribeButtonEdges).mockClear();
  vi.mocked(subscribeButtonGestures).mockClear();
  vi.mocked(saveButtonMappings).mockClear();
  vi.mocked(exportButtonMappingConfiguration).mockClear();
  vi.mocked(importButtonMappingConfiguration).mockClear();
  vi.mocked(startShortcutCapture).mockClear();
  vi.mocked(stopShortcutCapture).mockClear();
  vi.mocked(testButtonMapping).mockClear();
});

describe("buttons mapping page", () => {
  async function recordChord(wrapper: VueWrapper, keys: string[]): Promise<void> {
    await wrapper.get(".custom-shortcut-row button").trigger("click");
    for (const key of keys) shortcutCaptureHandler!({ key, isPressed: true });
    for (const key of [...keys].reverse()) shortcutCaptureHandler!({ key, isPressed: false });
    await flushPromises();
  }

  it("recognizes a recorded known shortcut while preserving its physical modifier keys", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 2);
    await recordChord(wrapper, ["right_control", "enter"]);
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.power?.long).toEqual({ type: "shortcut", chord: { keys: ["right_control", "enter"] } });
    const power = wrapper.findAll(".mapping-card").find(card => card.get("strong").text() === "电源")!;
    expect(power.findAll(".mapping-cell")[2]!.text()).toContain("插队/排队");
    expect(wrapper.get('[data-action-id="queue-message"]').attributes("aria-pressed")).toBe("true");
    expect(readActionLibrary().state.custom).toHaveLength(0);
    expect(readActionLibrary().state.usage["queue-message"]?.count).toBe(1);
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("distinguishes recorded chat and tab navigation while retaining physical keys", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 1);
    await recordChord(wrapper, ["right_control", "left_shift", "bracket_right"]);
    const power = wrapper.findAll(".mapping-card").find(card => card.get("strong").text() === "电源")!;
    expect(power.findAll(".mapping-cell")[1]!.text()).toContain("下一个标签页");
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.power?.double).toEqual({ type: "shortcut", chord: { keys: ["right_control", "left_shift", "bracket_right"] } });
    await recordChord(wrapper, ["right_control", "left_alt", "bracket_right"]);
    expect(power.findAll(".mapping-cell")[1]!.text()).toContain("下一个聊天");
    expect(readActionLibrary().state.custom).toHaveLength(0);
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("remembers an unmatched recorded shortcut, renames it and reuses it after reopening", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    await recordChord(wrapper, ["right_alt", "f8"]);
    const original = readActionLibrary().state.custom[0]!;
    expect(original.action).toEqual({ type: "shortcut", chord: { keys: ["right_alt", "f8"] } });
    await wrapper.get("#custom-action-name").setValue("整理笔记");
    await wrapper.findAll("button").find(button => button.text() === "保存名称")!.trigger("click");
    expect(readActionLibrary().state.custom[0]!.label).toBe("整理笔记");
    const power = wrapper.findAll(".mapping-card").find(card => card.get("strong").text() === "电源")!;
    expect(power.findAll(".mapping-cell")[0]!.text()).toContain("整理笔记");
    expect(readActionLibrary().state.usage[original.id]!.count).toBe(1);
    wrapper.unmount();

    vi.mocked(saveButtonMappings).mockClear();
    const reopened = await mountPage();
    expect(saveButtonMappings).not.toHaveBeenCalled();
    await openCell(reopened, "菜单", 1);
    await chooseAction(reopened, "整理笔记");
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.menu?.double).toEqual(original.action);
    expect(readActionLibrary().state.custom).toHaveLength(1);
    expect(readActionLibrary().state.usage[original.id]!.count).toBe(2);
    reopened.unmount();
  });

  it("does not learn an action whose mapping save failed, including recorder completion", async () => {
    vi.mocked(saveButtonMappings).mockRejectedValueOnce(new Error("保存失败，请重试"));
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    await recordChord(wrapper, ["left_alt", "f8"]);
    expect(localStorage.getItem(ACTION_LIBRARY_STORAGE_KEY)).toBeNull();
    expect(wrapper.get(".mapping-status").text()).toContain("保存失败，请重试");
    expect(wrapper.find("#custom-action-name").exists()).toBe(false);
    expect(readActionLibrary().state.usage).toEqual({});
    wrapper.unmount();
  });

  it("learns a failed recording exactly once when Save configuration later succeeds", async () => {
    vi.mocked(saveButtonMappings).mockRejectedValueOnce(new Error("临时保存失败"));
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    await recordChord(wrapper, ["left_alt", "f8"]);
    expect(readActionLibrary().state.custom).toHaveLength(0);
    const save = wrapper.findAll(".mapping-footer button").find(button => button.text() === "保存配置")!;
    await save.trigger("click");
    await flushPromises();
    const remembered = readActionLibrary().state.custom[0]!;
    expect(remembered.action).toEqual({ type: "shortcut", chord: { keys: ["left_alt", "f8"] } });
    expect(readActionLibrary().state.usage[remembered.id]!.count).toBe(1);
    expect(wrapper.find("#custom-action-name").exists()).toBe(true);
    await save.trigger("click");
    await flushPromises();
    expect(readActionLibrary().state.usage[remembered.id]!.count).toBe(1);
    wrapper.unmount();
  });

  it("does not learn a failed choice replaced by a different successful assignment", async () => {
    vi.mocked(saveButtonMappings).mockRejectedValueOnce(new Error("临时保存失败"));
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    await wrapper.get('[data-action-id="next-chat"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-action-id="previous-chat"]').trigger("click");
    await flushPromises();
    expect(readActionLibrary().state.usage["next-chat"]).toBeUndefined();
    expect(readActionLibrary().state.usage["previous-chat"]!.count).toBe(1);
    wrapper.unmount();
  });

  it("waits for a cancelled pending capture to clean up before starting another", async () => {
    let completeFirstStart!: () => void;
    vi.mocked(startShortcutCapture).mockImplementationOnce(() => new Promise<void>(resolve => { completeFirstStart = resolve; }));
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    await wrapper.get(".custom-shortcut-row button").trigger("click");
    expect(wrapper.get('[data-action-id="next-chat"]').attributes("disabled")).toBeDefined();
    await wrapper.get('[data-action-id="next-chat"]').trigger("click");
    expect(saveButtonMappings).not.toHaveBeenCalled();
    await wrapper.findAll(".mapping-editor button").find(button => button.text() === "关闭")!.trigger("click");
    await openCell(wrapper, "菜单", 0);
    expect(wrapper.get(".custom-shortcut-row button").attributes("disabled")).toBeDefined();
    await wrapper.get(".custom-shortcut-row button").trigger("click");
    expect(startShortcutCapture).toHaveBeenCalledOnce();
    completeFirstStart();
    await flushPromises();
    expect(stopShortcutCapture).toHaveBeenCalledOnce();
    await wrapper.get(".custom-shortcut-row button").trigger("click");
    await flushPromises();
    expect(startShortcutCapture).toHaveBeenCalledTimes(2);
    shortcutCaptureHandler!({ key: "f8", isPressed: true });
    shortcutCaptureHandler!({ key: "f8", isPressed: false });
    await flushPromises();
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.menu?.single).toEqual({ type: "shortcut", chord: { keys: ["f8"] } });
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.power).toBeUndefined();
    wrapper.unmount();
  });

  it("keeps Common stable while editing and refreshes learned and pinned order on the next target", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    const ids = () => wrapper.findAll(".action-option-select").map(button => button.attributes("data-action-id"));
    const initial = ids();
    await wrapper.get('[data-action-id="next-chat"]').trigger("click");
    await flushPromises();
    expect(ids()).toEqual(initial);
    edgeHandler!({ button: "power", isPressed: true });
    gestureHandler!({ button: "power", trigger: "single" });
    await flushPromises();
    expect(readActionLibrary().state.usage["next-chat"]!.count).toBe(1);
    await openCell(wrapper, "电源", 1);
    expect(ids()[0]).toBe("next-chat");
    await wrapper.get('button[aria-label="置顶：撤销"]').trigger("click");
    expect(ids()[0]).toBe("next-chat");
    await openCell(wrapper, "菜单", 0);
    expect(ids()[0]).toBe("undo");
    expect(readActionLibrary().state.pinned).toContain("undo");
    wrapper.unmount();
  });

  it("keeps existing user mappings and single-key names without saving on mount", async () => {
    vi.mocked(getButtonMappings).mockResolvedValueOnce({ enabled: false, actions: {
      power: { single: { type: "shortcut", chord: { keys: ["escape"] } }, double: { type: "disabled" }, long: { type: "shortcut", chord: { keys: ["control", "shift", "bracket_right"] } } },
      back: { single: { type: "normal_backspace" }, double: { type: "disabled" }, long: { type: "disabled" } },
    } });
    const wrapper = await mountPage();
    const power = wrapper.findAll(".mapping-card").find(card => card.get("strong").text() === "电源")!;
    expect(power.findAll(".mapping-cell")[0]!.text()).toContain("Esc");
    expect(power.findAll(".mapping-cell")[2]!.text()).toContain("下一个标签页");
    const back = wrapper.findAll(".mapping-card").find(card => card.get("strong").text() === "返回")!;
    expect(back.findAll(".mapping-cell")[0]!.text()).toContain("Backspace");
    await openCell(wrapper, "电源", 2);
    await wrapper.get('#action-tab-view').trigger("click");
    expect(wrapper.get('[data-action-id="next-tab"]').attributes("aria-pressed")).toBe("true");
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(localStorage.getItem(ACTION_LIBRARY_STORAGE_KEY)).toBeNull();
    wrapper.unmount();
  });

  it("stops shortcut capture when the editor closes", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    await wrapper.get(".custom-shortcut-row button").trigger("click");
    await wrapper.findAll(".mapping-editor button").find(button => button.text() === "关闭")!.trigger("click");
    await flushPromises();
    expect(stopShortcutCapture).toHaveBeenCalledOnce();
    shortcutCaptureHandler!({ key: "f8", isPressed: true });
    expect(saveButtonMappings).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("browses tabs and searches every category without saving or executing", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    expect(wrapper.findAll('[role="tab"]').map(tab => tab.text())).toEqual(["常用", "聊天", "编辑", "视图", "系统"]);
    expect(wrapper.get(".action-sections").element.firstElementChild).toBe(wrapper.get(".shortcut-recorder").element);
    expect(wrapper.find('select').exists()).toBe(false);
    expect(wrapper.get('[data-action-id="previous-chat"]').text()).toContain("Ctrl + Alt + [");
    await wrapper.get('#action-tab-system').trigger("click");
    await wrapper.get('input[aria-label="搜索全部功能"]').setValue("插队");
    expect(wrapper.get('[data-action-id="queue-message"]').text()).toContain("Ctrl + Enter");
    expect(wrapper.get(".shortcut-help").text()).toContain("连接与语音");
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("keeps collapsed mouse and app tools available through every tab and search without saving", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    for (const tab of ["common", "chat", "edit", "view", "system"]) {
      await wrapper.get(`#action-tab-${tab}`).trigger("click");
      await wrapper.get('input[aria-label="搜索全部功能"]').setValue("没有匹配的功能");
      for (const [selector, title] of [[".mouse-tools", "鼠标控制"], [".app-tools", "打开应用"]]) {
        const tools = wrapper.get(selector!);
        expect(tools.element.tagName).toBe("DETAILS");
        expect(tools.get("summary").text()).toContain(title);
        expect((tools.element as HTMLDetailsElement).open).toBe(false);
      }
    }
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(testButtonMapping).not.toHaveBeenCalled();
    expect(localStorage.getItem(ACTION_LIBRARY_STORAGE_KEY)).toBeNull();
    wrapper.unmount();
  });

  it("preserves independent tool expansion when the functional tab or search changes", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    await expandTools(wrapper, ".mouse-tools");
    const mouse = wrapper.get(".mouse-tools").element as HTMLDetailsElement;
    const apps = wrapper.get(".app-tools").element as HTMLDetailsElement;
    for (const tab of ["chat", "edit", "view", "system", "common"]) {
      await wrapper.get(`#action-tab-${tab}`).trigger("click");
      await wrapper.get('input[aria-label="搜索全部功能"]').setValue("插队");
      expect(wrapper.get(".mouse-tools").element).toBe(mouse);
      expect(wrapper.get(".app-tools").element).toBe(apps);
      expect(mouse.open).toBe(true);
      expect(apps.open).toBe(false);
    }
    await wrapper.get(".mouse-tools summary").trigger("click");
    await expandTools(wrapper, ".app-tools");
    await wrapper.get("#action-tab-view").trigger("click");
    expect(mouse.open).toBe(false);
    expect(apps.open).toBe(true);
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it.each([
    { action: { type: "scroll", direction: "down", steps: 3 } as ButtonAction, label: "每次滚动格数", amount: "steps" },
    { action: { type: "mouse_move", direction: "right", distance: 30 } as ButtonAction, label: "每次移动像素", amount: "distance" },
  ])("keeps $label editable outside tool disclosures and independent of tabs or search", async ({ action, label, amount }) => {
    vi.mocked(getButtonMappings).mockResolvedValueOnce({ enabled: true, actions: {
      power: { single: action, double: { type: "disabled" }, long: { type: "disabled" } },
    } });
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    expect(saveButtonMappings).not.toHaveBeenCalled();
    for (const [index, tab] of ["common", "chat", "edit", "view", "system"].entries()) {
      await wrapper.get(`#action-tab-${tab}`).trigger("click");
      await wrapper.get('input[aria-label="搜索全部功能"]').setValue("插队");
      const input = wrapper.get(`input[aria-label="${label}"]`);
      expect(input.element.closest("details")).toBeNull();
      expect((wrapper.get(".mouse-tools").element as HTMLDetailsElement).open).toBe(false);
      await input.setValue(String(5 + index));
      await flushPromises();
      expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.power?.single).toEqual({ ...action, [amount]: 5 + index });
    }
    await expandTools(wrapper, ".mouse-tools");
    await wrapper.get(".mouse-tools summary").trigger("click");
    expect(wrapper.findAll(`input[aria-label="${label}"]`)).toHaveLength(1);
    expect(saveButtonMappings).toHaveBeenCalledTimes(5);
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("searches the app library independently from the functional list", async () => {
    vi.mocked(getButtonMappings).mockResolvedValueOnce({ enabled: true, actions: {}, applications: [
      { name: "Example Writer", path: "shell:AppsFolder\\ExampleWriter!App" },
      { name: "Example Reader", path: "shell:AppsFolder\\ExampleReader!App" },
    ] });
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    await wrapper.get('input[aria-label="搜索全部功能"]').setValue("插队");
    await expandTools(wrapper, ".app-tools");
    const appSearch = wrapper.get('input[aria-label="搜索应用"]');
    await appSearch.setValue("Writer");
    const appNames = () => wrapper.findAll(".app-tools .saved-app-grid button").map(button => button.text());
    expect(appNames()).toEqual(["Example Writer"]);
    expect(wrapper.get('[data-action-id="queue-message"]').text()).toContain("插队/排队");
    await wrapper.get("#action-tab-view").trigger("click");
    expect((appSearch.element as HTMLInputElement).value).toBe("Writer");
    expect(appNames()).toEqual(["Example Writer"]);
    await wrapper.get('input[aria-label="搜索全部功能"]').setValue("Reader");
    expect(wrapper.findAll(".action-option-select strong").map(label => label.text())).toContain("打开 Example Reader");
    const readerOption = wrapper.findAll(".action-option").find(option => option.get("strong").text() === "打开 Example Reader")!;
    const readerActionId = readerOption.get(".action-option-select").attributes("data-action-id");
    expect(readerOption.find(".action-pin").exists()).toBe(false);
    expect(appNames()).toEqual(["Example Writer"]);
    expect((appSearch.element as HTMLInputElement).value).toBe("Writer");
    await appSearch.setValue("");
    expect(appNames()).toEqual(["Example Writer", "Example Reader"]);
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(testButtonMapping).not.toHaveBeenCalled();
    expect(localStorage.getItem(ACTION_LIBRARY_STORAGE_KEY)).toBeNull();
    await readerOption.get(".action-option-select").trigger("click");
    await flushPromises();
    const rememberedReader = wrapper.findAll(".action-option").find(option => option.get(".action-option-select").attributes("data-action-id") === readerActionId)!;
    expect(rememberedReader.find(".action-pin").exists()).toBe(true);
    expect(saveButtonMappings).toHaveBeenCalledOnce();
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("saves a Codex option only to the selected gesture without executing it", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 1);
    await wrapper.get('[data-action-id="next-chat"]').trigger("click");
    await flushPromises();
    const saved = vi.mocked(saveButtonMappings).mock.lastCall![0];
    expect(saved.actions.power?.double).toEqual({ type: "shortcut", chord: { keys: ["control", "alt", "bracket_right"] } });
    expect(saved.actions.ok?.single).toEqual({ type: "shortcut", chord: { keys: ["enter"] } });
    expect(wrapper.get('[data-action-id="next-chat"]').attributes("aria-pressed")).toBe("true");
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("adds scanned apps to the library without changing button bindings", async () => {
    const wrapper = await mountPage();
    await flushPromises();
    const powerCard = wrapper
      .findAll(".mapping-card")
      .find((item) => item.find(".mapping-card-title strong").text() === "电源")!;
    await powerCard.findAll(".mapping-cell")[0]!.trigger("click");
    await expandTools(wrapper, ".app-tools");
    await wrapper
      .findAll("button")
      .find((button) => button.text() === "扫描本机应用")!
      .trigger("click");
    await flushPromises();
    await wrapper.get('input[aria-label="全选当前结果"]').setValue(true);
    await wrapper.get(".registered-apps-dialog .primary-button").trigger("click");
    await flushPromises();

    const saved = vi.mocked(saveButtonMappings).mock.lastCall![0];
    expect(saved.applications).toEqual([
      { name: "Registered Example", path: "shell:AppsFolder\\Example!App" },
    ]);
    expect(saved.actions.power).toBeUndefined();
    expect(saved.actions.ok?.single).toEqual({
      type: "shortcut",
      chord: { keys: ["enter"] },
    });
    wrapper.unmount();
  });

  it("renders the remote canvas with 12 button cards, the voice card and 36 trigger cells", async () => {
    const wrapper = await mountPage();
    expect(wrapper.findAll(".mapping-card")).toHaveLength(13);
    expect(wrapper.findAll(".mapping-cell")).toHaveLength(36);
    const voiceCard = wrapper.find(".voice-card");
    expect(voiceCard.text()).toContain("语音键");
    expect(voiceCard.text()).toContain("按住说话");
    expect(wrapper.get(".remote-photo img").attributes()).toMatchObject({
      src: "/RC003-remote-photo@2x.png",
      loading: "eager",
      fetchpriority: "high",
      width: "202",
      height: "410",
    });
    expect(wrapper.findAll(".mapping-connections path").every(path => path.attributes("pathLength") === "1")).toBe(true);
  });

  it("keeps prepared actions usable during entrance and immediately reveals them on pointer or focus", async () => {
    let now = 1_000_000;
    const clock = vi.spyOn(performance, "now").mockImplementation(() => now);
    const initialMappings = await getButtonMappings();
    let wrapper = mount(ButtonsPage, { props: { runtime, initialMappings } });
    try {
      expect(wrapper.get(".mapping-canvas").classes()).toContain("mapping-canvas-entering");
      const okCell = wrapper.findAll(".mapping-cell").find(cell => cell.text().includes("Enter"))!;
      expect(okCell.attributes("disabled")).toBeUndefined();
      await okCell.trigger("pointerdown");
      expect(wrapper.get(".mapping-canvas").classes()).not.toContain("mapping-canvas-entering");
      await okCell.trigger("click");
      expect(wrapper.get(".mapping-editor").text()).toContain("Enter");
      wrapper.unmount();
      now += 301;
      wrapper = mount(ButtonsPage, { props: { runtime, initialMappings } });
      expect(wrapper.get(".mapping-canvas").classes()).toContain("mapping-canvas-entering");
      await wrapper.get(".mapping-cell").trigger("focusin");
      expect(wrapper.get(".mapping-canvas").classes()).not.toContain("mapping-canvas-entering");
    } finally {
      clock.mockRestore();
      wrapper.unmount();
      await flushPromises();
    }
  });

  it("skips entrance on a rapid return and does not reintroduce it after completion or resize", async () => {
    let now = 2_000_000;
    const clock = vi.spyOn(performance, "now").mockImplementation(() => now);
    let notifyResize!: ResizeObserverCallback;
    vi.stubGlobal("ResizeObserver", class {
      constructor(callback: ResizeObserverCallback) { notifyResize = callback; }
      observe() {}
      disconnect() {}
    });
    const initialMappings = await getButtonMappings();
    let wrapper = mount(ButtonsPage, { props: { runtime, initialMappings } });
    try {
      expect(wrapper.get(".mapping-canvas").classes()).toContain("mapping-canvas-entering");
      await wrapper.get(".mapping-card").trigger("animationend");
      expect(wrapper.get(".mapping-canvas").classes()).not.toContain("mapping-canvas-entering");
      notifyResize([{ contentRect: { width: 1280 } } as ResizeObserverEntry], {} as ResizeObserver);
      await flushPromises();
      expect(wrapper.get(".remote-photo").attributes("style")).toContain("left: 539px");
      expect(wrapper.get(".mapping-canvas").classes()).not.toContain("mapping-canvas-entering");
      wrapper.unmount();
      now += 100;
      wrapper = mount(ButtonsPage, { props: { runtime, initialMappings } });
      expect(wrapper.get(".mapping-canvas").classes()).not.toContain("mapping-canvas-entering");
      wrapper.unmount();
      now += 301;
      wrapper = mount(ButtonsPage, { props: { runtime, initialMappings } });
      expect(wrapper.get(".mapping-canvas").classes()).toContain("mapping-canvas-entering");
    } finally {
      clock.mockRestore();
      wrapper.unmount();
      vi.unstubAllGlobals();
      await flushPromises();
    }
  });

  it.each([true, false])("honors reduced-motion=%s and finishes entrance when the preference changes", async (reduced) => {
    let motionChanged!: () => void;
    const removeListener = vi.fn();
    vi.stubGlobal("matchMedia", () => ({
      matches: reduced,
      addEventListener: (_type: string, listener: () => void) => { motionChanged = listener; },
      removeEventListener: removeListener,
    }));
    const clock = vi.spyOn(performance, "now").mockReturnValue(3_000_000);
    const initialMappings = await getButtonMappings();
    const wrapper = mount(ButtonsPage, { props: { runtime, initialMappings } });
    try {
      expect(wrapper.get(".mapping-canvas").classes().includes("mapping-canvas-entering")).toBe(!reduced);
      motionChanged();
      await flushPromises();
      expect(wrapper.get(".mapping-canvas").classes()).not.toContain("mapping-canvas-entering");
    } finally {
      clock.mockRestore();
      wrapper.unmount();
      vi.unstubAllGlobals();
      await flushPromises();
    }
    expect(removeListener).toHaveBeenCalledWith("change", motionChanged);
  });

  it("does not register listeners or polling after unmounting during initial load", async () => {
    let resolveMappings!: (value: Awaited<ReturnType<typeof getButtonMappings>>) => void;
    const pendingMappings = new Promise<Awaited<ReturnType<typeof getButtonMappings>>>(
      (resolve) => {
        resolveMappings = resolve;
      },
    );
    vi.mocked(getButtonMappings).mockImplementationOnce(() => pendingMappings);
    const intervalSpy = vi.spyOn(window, "setInterval");

    const wrapper = mount(ButtonsPage, { props: { runtime } });
    await flushPromises();
    wrapper.unmount();
    resolveMappings({ enabled: true, actions: {} });
    await flushPromises();

    expect(subscribeButtonEdges).not.toHaveBeenCalled();
    expect(subscribeButtonGestures).not.toHaveBeenCalled();
    expect(intervalSpy).not.toHaveBeenCalled();
    intervalSpy.mockRestore();
  });

  it("places and resizes the remote before backend loading finishes", async () => {
    let resolveMappings!: (value: ButtonMappings) => void;
    vi.mocked(getButtonMappings).mockImplementationOnce(() => new Promise((resolve) => {
      resolveMappings = resolve;
    }));
    const widthSpy = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(1280);
    let notifyResize!: ResizeObserverCallback;
    const disconnect = vi.fn();
    const observe = vi.fn();
    vi.stubGlobal("ResizeObserver", class {
      constructor(callback: ResizeObserverCallback) { notifyResize = callback; }
      observe = observe;
      disconnect = disconnect;
    });
    const wrapper = mount(ButtonsPage, { props: { runtime } });
    try {
      await flushPromises();
      expect(subscribeButtonEdges).not.toHaveBeenCalled();
      expect(wrapper.get(".mapping-connections").attributes("width")).toBe("1280");
      expect(wrapper.get(".remote-photo").attributes("style")).toContain("left: 539px");
      expect(observe).toHaveBeenCalledWith(wrapper.get(".mapping-canvas").element);
      notifyResize([{ contentRect: { width: 1000 } } as ResizeObserverEntry], {} as ResizeObserver);
      await flushPromises();
      expect(wrapper.get(".mapping-connections").attributes("width")).toBe("1000");
      expect(wrapper.get(".remote-photo").attributes("style")).toContain("left: 399px");
      notifyResize([{ contentRect: { width: 700 } } as ResizeObserverEntry], {} as ResizeObserver);
      await flushPromises();
      expect(wrapper.get(".mapping-connections").attributes("width")).toBe("800");
    } finally {
      wrapper.unmount();
      resolveMappings({ enabled: true, actions: {} });
      await flushPromises();
      widthSpy.mockRestore();
      vi.unstubAllGlobals();
    }
    expect(disconnect).toHaveBeenCalledOnce();
    expect(subscribeButtonEdges).not.toHaveBeenCalled();
  });

  it("does not display or save empty mappings while the configuration is loading", async () => {
    let resolveMappings!: (value: ButtonMappings) => void;
    vi.mocked(getButtonMappings).mockImplementationOnce(() => new Promise((resolve) => {
      resolveMappings = resolve;
    }));
    const wrapper = mount(ButtonsPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".mapping-canvas").attributes("aria-busy")).toBe("true");
    expect(wrapper.findAll(".mapping-cell-placeholder")).toHaveLength(36);
    expect(wrapper.get(".mapping-canvas").text()).not.toContain("未设置");
    expect(wrapper.findAll(".mapping-cell").every(cell => cell.attributes("disabled") !== undefined)).toBe(true);
    await wrapper.get(".mapping-cell").trigger("click");
    expect(wrapper.find(".mapping-editor").exists()).toBe(false);
    const save = wrapper.findAll(".mapping-footer button").find(button => button.text() === "保存配置")!;
    expect(save.attributes("disabled")).toBeDefined();
    await save.trigger("click");
    expect(saveButtonMappings).not.toHaveBeenCalled();
    resolveMappings({ enabled: true, actions: { ok: { single: { type: "shortcut", chord: { keys: ["enter"] } }, double: { type: "disabled" }, long: { type: "disabled" } } } });
    await flushPromises();
    expect(wrapper.findAll(".mapping-cell-placeholder")).toHaveLength(0);
    expect(wrapper.get(".mapping-canvas").text()).toContain("Enter");
    expect(wrapper.get(".mapping-canvas").text()).toContain("未设置");
    expect(wrapper.get(".mapping-canvas").attributes("aria-busy")).toBe("false");
    wrapper.unmount();
  });

  it("renders prepared settings in its first render without a duplicate backend read", async () => {
    const initialMappings: ButtonMappings = { enabled: true, actions: { ok: { single: { type: "shortcut", chord: { keys: ["enter"] } }, double: { type: "disabled" }, long: { type: "disabled" } } } };
    const wrapper = mount(ButtonsPage, { props: { runtime, initialMappings } });
    expect(wrapper.findAll(".mapping-cell-placeholder")).toHaveLength(0);
    expect(wrapper.get(".mapping-canvas").text()).toContain("Enter");
    await flushPromises();
    expect(getButtonMappings).not.toHaveBeenCalled();
    expect(subscribeButtonEdges).toHaveBeenCalledOnce();
    wrapper.unmount();
  });

  it("shows saved mappings without waiting for the installed app scan", async () => {
    let resolveApps!: (value: Awaited<ReturnType<typeof listPresetApps>>) => void;
    vi.mocked(listPresetApps).mockImplementationOnce(() => new Promise((resolve) => { resolveApps = resolve; }));
    const wrapper = await mountPage();
    expect(wrapper.get(".mapping-canvas").text()).toContain("Enter");
    expect(wrapper.findAll(".mapping-cell-placeholder")).toHaveLength(0);
    wrapper.unmount();
    resolveApps([]);
    await flushPromises();
  });

  it("offers a read retry instead of empty settings after a configuration failure", async () => {
    vi.mocked(getButtonMappings).mockRejectedValueOnce(new Error("unavailable"));
    const wrapper = mount(ButtonsPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".mapping-load-error").text()).toContain("按键配置读取失败");
    expect(wrapper.get(".mapping-canvas").text()).not.toContain("未设置");
    expect(subscribeButtonEdges).not.toHaveBeenCalled();
    await wrapper.get(".mapping-load-error button").trigger("click");
    await flushPromises();
    expect(wrapper.find(".mapping-load-error").exists()).toBe(false);
    expect(wrapper.get(".mapping-canvas").text()).toContain("Enter");
    expect(subscribeButtonEdges).toHaveBeenCalledOnce();
    wrapper.unmount();
  });

  it("immediately releases a listener that resolves after the page is unmounted", async () => {
    let resolveUnlisten!: (unlisten: () => void) => void;
    const pendingUnlisten = new Promise<() => void>((resolve) => {
      resolveUnlisten = resolve;
    });
    vi.mocked(subscribeButtonEdges).mockImplementationOnce(() => pendingUnlisten);
    const stopEdges = vi.fn();

    const wrapper = mount(ButtonsPage, { props: { runtime } });
    await vi.waitFor(() => expect(subscribeButtonEdges).toHaveBeenCalledOnce());
    const intervalSpy = vi.spyOn(window, "setInterval");
    wrapper.unmount();
    resolveUnlisten(stopEdges);
    await flushPromises();

    expect(stopEdges).toHaveBeenCalledOnce();
    expect(subscribeButtonGestures).not.toHaveBeenCalled();
    expect(intervalSpy).not.toHaveBeenCalled();
    intervalSpy.mockRestore();
  });

  it("saves, exports and imports a versioned mapping configuration from the footer", async () => {
    const wrapper = await mountPage();
    const button = (label: string) =>
      wrapper.findAll(".mapping-footer button").find((item) => item.text() === label)!;

    await button("保存配置").trigger("click");
    await vi.waitFor(() => expect(saveButtonMappings).toHaveBeenCalled());
    await flushPromises();
    expect(wrapper.text()).toContain("配置已保存并生效");

    await button("导出配置…").trigger("click");
    await vi.waitFor(() => expect(exportButtonMappingConfiguration).toHaveBeenCalledOnce());
    expect(wrapper.text()).toContain("按键映射配置已导出");

    await button("导入配置…").trigger("click");
    await vi.waitFor(() => expect(importButtonMappingConfiguration).toHaveBeenCalledOnce());
    expect(wrapper.text()).toContain("按键映射配置已导入并生效");
    const powerCard = wrapper
      .findAll(".mapping-card")
      .find((card) => card.text().includes("电源"))!;
    expect(powerCard.text()).toContain("Esc");
  });

  it("marks configured cells and opens the editor with the correct target", async () => {
    const wrapper = await mountPage();
    const okCard = wrapper
      .findAll(".mapping-card")
      .find((card) => card.text().includes("确定"));
    expect(okCard).toBeDefined();
    expect(okCard!.text()).toContain("Enter");

    const singleCell = okCard!.findAll(".mapping-cell")[0]!;
    expect(singleCell.classes()).toContain("set");
    await singleCell.trigger("click");
    expect(wrapper.find(".mapping-editor").text()).toContain("确定 · 单击");
  });

  it("applies a preset to the editing target and auto-persists (对齐 Mac 即时保存)", async () => {
    const wrapper = await mountPage();
    const powerCard = wrapper
      .findAll(".mapping-card")
      .find((card) => card.text().includes("电源"));
    await powerCard!.findAll(".mapping-cell")[2]!.trigger("click");

    const editor = wrapper.find(".mapping-editor");
    expect(editor.text()).toContain("电源 · 长按");
    // 点击 Esc 预设即自动保存（无需保存按钮）。
    await chooseAction(wrapper, "Esc");
    await vi.waitFor(() => {
      if (vi.mocked(saveButtonMappings).mock.calls.length === 0) {
        throw new Error("自动保存未触发");
      }
    });
    const saved = vi.mocked(saveButtonMappings).mock.calls[0]![0] as {
      actions: Record<string, { long: { type: string; chord?: { keys: string[] } } }>;
    };
    expect(saved.actions.power!.long.type).toBe("shortcut");
    expect(saved.actions.power!.long.chord!.keys).toEqual(["escape"]);
    await flushPromises();

    // 禁用按键按钮：禁用当前格并自动保存。
    const disableButton = wrapper
      .findAll("button")
      .find((button) => button.text() === "禁用按键");
    expect(disableButton).toBeDefined();
    await disableButton!.trigger("click");
    await vi.waitFor(() => {
      if (vi.mocked(saveButtonMappings).mock.calls.length < 2) {
        throw new Error("禁用后未自动保存");
      }
    });
    const disabledSaved = vi.mocked(saveButtonMappings).mock.calls[1]![0] as {
      actions: Record<string, { long: { type: string } }>;
    };
    expect(disabledSaved.actions.power!.long.type).toBe("disabled");
  });

  it("configures mouse actions with independent validated amounts", async () => {
    const wrapper = await mountPage();
    await flushPromises();
    const powerCard = wrapper
      .findAll(".mapping-card")
      .find((card) => card.text().includes("电源"))!;
    await powerCard.findAll(".mapping-cell")[0]!.trigger("click");
    const choose = async (label: string) => {
      await wrapper
        .findAll(".mapping-editor button")
        .find((button) => button.text() === label)!
        .trigger("click");
      await flushPromises();
    };

    await expandTools(wrapper, ".mouse-tools");
    await choose("滚轮向下");
    await wrapper.get('input[aria-label="每次滚动格数"]').setValue("5");
    await flushPromises();
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.power!.single).toEqual({
      type: "scroll",
      direction: "down",
      steps: 5,
    });

    const saveCount = vi.mocked(saveButtonMappings).mock.calls.length;
    await wrapper.get('input[aria-label="每次滚动格数"]').setValue("101");
    await flushPromises();
    expect(vi.mocked(saveButtonMappings).mock.calls).toHaveLength(saveCount);
    expect(wrapper.text()).toContain("请输入 1 到 100 之间的整数");

    await choose("左键双击");
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.power!.single).toEqual({
      type: "mouse_click",
      kind: "double_left",
    });
    wrapper.unmount();
  });

  it("records a physical Win+L chord directly by default", async () => {
    const wrapper = await mountPage();
    const powerCard = wrapper
      .findAll(".mapping-card")
      .find((card) => card.text().includes("电源"))!;
    await powerCard.findAll(".mapping-cell")[0]!.trigger("click");
    const captureButton = wrapper
      .findAll(".mapping-editor .chip")
      .find((button) => button.text().includes("录入自定义快捷键"))!;
    await captureButton.trigger("click");

    shortcutCaptureHandler!({ key: "left_windows", isPressed: true });
    shortcutCaptureHandler!({ key: "l", isPressed: true });
    shortcutCaptureHandler!({ key: "l", isPressed: false });
    await flushPromises();
    expect(stopShortcutCapture).not.toHaveBeenCalled();
    shortcutCaptureHandler!({ key: "left_windows", isPressed: false });
    await vi.waitFor(() => expect(stopShortcutCapture).toHaveBeenCalledOnce());
    const saved = vi.mocked(saveButtonMappings).mock.calls.at(-1)?.[0] as ButtonMappings;
    expect(saved.actions.power?.single).toEqual({
      type: "shortcut",
      chord: { keys: ["left_windows", "l"] },
    });
  });

  it("records Win+L safely after the user enables fallback mode", async () => {
    const wrapper = await mountPage();
    const powerCard = wrapper
      .findAll(".mapping-card")
      .find((card) => card.text().includes("电源"))!;
    await powerCard.findAll(".mapping-cell")[0]!.trigger("click");
    const safeToggle = wrapper.find(".safe-capture-toggle input");
    const shortcutRow = wrapper.find(".custom-shortcut-row");
    const toggleRow = wrapper.find(".safe-capture-toggle");
    expect(shortcutRow.element.nextElementSibling).toBe(toggleRow.element);
    expect(safeToggle.classes()).toContain("toggle-input");
    expect(safeToggle.element.nextElementSibling?.textContent).toContain(
      "直接录入无法完成或会触发系统动作时再开启",
    );
    expect((safeToggle.element as HTMLInputElement).checked).toBe(false);
    await safeToggle.setValue(true);
    const captureButton = wrapper
      .findAll(".mapping-editor .chip")
      .find((button) => button.text().includes("录入自定义快捷键"))!;
    await captureButton.trigger("click");
    await vi.waitFor(() => expect(startShortcutCapture).toHaveBeenCalledOnce());

    const leftWin = wrapper
      .findAll(".capture-modifiers .chip")
      .find((button) => button.text() === "左 Win")!;
    await leftWin.trigger("click");
    shortcutCaptureHandler!({ key: "l", isPressed: true });
    await vi.waitFor(() => {
      const calls = vi.mocked(saveButtonMappings).mock.calls;
      const saved = calls.at(-1)?.[0] as ButtonMappings | undefined;
      const action = saved?.actions.power?.single;
      if (action?.type !== "shortcut" || action.chord.keys.join("+") !== "left_windows+l") {
        throw new Error("Win+L 未保存");
      }
    });
    await vi.waitFor(() =>
      expect(wrapper.text()).toContain("已录入 锁定电脑，松开全部按键后完成"),
    );
    shortcutCaptureHandler!({ key: "l", isPressed: false });
    await vi.waitFor(() => expect(stopShortcutCapture).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(wrapper.text()).toContain("快捷键已录入：左 Win + L"));
  });

  it("highlights the card for a pressed physical button and clears it on release", async () => {
    const wrapper = await mountPage();
    const upCard = () =>
      wrapper.findAll(".mapping-card").find((card) => card.text().includes("上"));
    expect(upCard()!.classes()).not.toContain("active");

    edgeHandler!({ button: "up", isPressed: true });
    await vi.waitFor(() => {
      if (!upCard()!.classes().includes("active")) throw new Error("未高亮");
    });
    edgeHandler!({ button: "up", isPressed: false });
    await vi.waitFor(() => {
      if (upCard()!.classes().includes("active")) throw new Error("未解除高亮");
    });
  });

  it("keeps the selection locked while pressing the remote unless unlocked", async () => {
    const wrapper = await mountPage();
    // 默认锁定：按下"返回"不改变当前选中（未选中任何键时仍为空）。
    edgeHandler!({ button: "back", isPressed: true });
    const backCard = () =>
      wrapper.findAll(".mapping-card").find((card) => card.text().includes("返回"));
    await vi.waitFor(() => {
      if (!backCard()!.classes().includes("active")) throw new Error("未高亮");
    });
    expect(backCard()!.classes()).not.toContain("selected");

    // 解锁后：按下即选中该键的编辑。
    const toggles = wrapper.findAll(".toggle-row");
    const lockToggle = toggles.find((row) => row.text().includes("锁定当前按键"));
    const input = lockToggle!.find("input");
    await input.setValue(false);
    edgeHandler!({ button: "back", isPressed: true });
    await vi.waitFor(() => {
      if (!backCard()!.classes().includes("selected")) throw new Error("未跟随选中");
    });
  });

  it("shows the fired gesture feedback from engine events", async () => {
    const wrapper = await mountPage();
    gestureHandler!({ button: "ok", trigger: "single" });
    await vi.waitFor(() => {
      // 手势反馈 = 对应格子出现闪烁态（flashed），600ms 后自动消失。
      if (!wrapper.find(".mapping-cell.flashed").exists()) {
        throw new Error("手势触发后格子未出现闪烁反馈");
      }
    });
  });

  /** 编辑器内按标签找 chip 并返回其禁用态。 */
  async function chooseAction(wrapper: VueWrapper, label: string): Promise<void> {
    await wrapper.get('input[aria-label="搜索全部功能"]').setValue(label);
    const option = wrapper.findAll(".action-option-select").find(button => button.get("strong").text() === label);
    expect(option, "未找到功能：" + label).toBeDefined();
    await option!.trigger("click");
    await flushPromises();
  }

  async function chipState(wrapper: VueWrapper, label: string): Promise<boolean> {
    if (label === "＋ 添加应用") {
      await expandTools(wrapper, ".app-tools");
      return (wrapper.get(".add-app").element as HTMLButtonElement).disabled;
    }
    if (label === "录入自定义快捷键") return (wrapper.get(".custom-shortcut-row button").element as HTMLButtonElement).disabled;
    await wrapper.get('input[aria-label="搜索全部功能"]').setValue(label);
    const option = wrapper.findAll(".action-option-select").find(button => button.get("strong").text() === label);
    expect(option, "未找到功能：" + label).toBeDefined();
    return (option!.element as HTMLButtonElement).disabled;
  }

  async function expandTools(wrapper: VueWrapper, selector: ".mouse-tools" | ".app-tools"): Promise<void> {
    const tools = wrapper.get(selector);
    if (!(tools.element as HTMLDetailsElement).open) await tools.get("summary").trigger("click");
    expect((tools.element as HTMLDetailsElement).open).toBe(true);
  }

  async function openCell(
    wrapper: VueWrapper,
    cardLabel: string,
    triggerIndex: number,
  ): Promise<void> {
    const card = wrapper.findAll(".mapping-card").find((c) => c.text().includes(cardLabel));
    expect(card, `未找到卡片：${cardLabel}`).toBeDefined();
    await card!.findAll(".mapping-cell")[triggerIndex]!.trigger("click");
    expect(wrapper.find(".mapping-editor").exists()).toBe(true);
  }

  it("全开放：确定·单击所有操作可配（注入链路已真机验证）+ 单响应提示", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "确定", 0);
    expect(await chipState(wrapper, "Enter")).toBe(false);
    expect(await chipState(wrapper, "Home")).toBe(false);
    expect(await chipState(wrapper, "Space")).toBe(false);
    expect(await chipState(wrapper, "粘贴")).toBe(false);
    expect(await chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(await chipState(wrapper, "＋ 添加应用")).toBe(false);
    // 武装族按键显示冷首按原生副作用提示（信息性，不门控）。
    expect(wrapper.find(".mapping-editor").text()).toContain("首次按下可能额外触发");
  });

  it("全开放：确定·双击与 TV 所有操作可配 + 各自的单响应提示", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "确定", 1);
    expect(await chipState(wrapper, "Enter")).toBe(false);
    expect(await chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(await chipState(wrapper, "＋ 添加应用")).toBe(false);
    expect(wrapper.find(".mapping-editor").text()).toContain("首次按下可能额外触发");

    await openCell(wrapper, "TV", 0);
    expect(await chipState(wrapper, "Enter")).toBe(false);
    expect(await chipState(wrapper, "静音")).toBe(false);
    expect(await chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(await chipState(wrapper, "＋ 添加应用")).toBe(false);
    expect(wrapper.find(".mapping-editor").text()).toContain("普通键盘不受此映射影响");
    expect(wrapper.find(".mapping-editor").text()).toContain("可能同时触发 Home 或输入反引号");
  });

  it("左键与其余方向键同样开放自定义并显示首次按键提示", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "左", 0);
    expect(await chipState(wrapper, "←")).toBe(false);
    expect(await chipState(wrapper, "Backspace")).toBe(false);
    expect(await chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(wrapper.find(".mapping-editor").text()).toContain("首次按下可能额外触发");

    // 与型号无关：RC001 上左键同样开放。
    const rc001 = await mountPage("rc001");
    const leftCellRc001 = rc001
      .findAll(".mapping-card")
      .find((c) => c.text().includes("左"))!
      .findAll(".mapping-cell")[0]!;
    expect((leftCellRc001.element as HTMLButtonElement).disabled).toBe(false);
  });

  it("电源（直接归因族）全开放且无单响应提示", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 2);
    expect(await chipState(wrapper, "Esc")).toBe(false);
    expect(await chipState(wrapper, "截图")).toBe(false);
    expect(await chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(await chipState(wrapper, "＋ 添加应用")).toBe(false);
    expect(wrapper.find(".mapping-editor").text()).not.toContain("首次按下可能额外触发");
  });

  it("opens every volume gesture on every model without claiming hardware support", async () => {
    for (const model of ["rc003", "rc001", "unknown"] as const) {
      const wrapper = await mountPage(model);
      for (const label of ["返回", "音量+", "音量−"]) {
        const card = wrapper.findAll(".mapping-card").find(c => c.find(".mapping-card-title strong").text() === label)!;
        for (const cell of card.findAll(".mapping-cell")) {
          expect((cell.element as HTMLButtonElement).disabled, `${model} ${label}`).toBe(false);
          await cell.trigger("click");
          expect(wrapper.get(".mapping-editor h2").text()).toContain(label);
        }
      }
      expect(wrapper.find(".back-hardware-note").exists()).toBe(model !== "rc001");
      wrapper.unmount();
    }
  });

  it.each(["missing", "disabled", "shortcut"] as const)("shows the configured volume summary for %s mappings", async (kind) => {
    const mappings: ButtonMappings = { enabled: true, actions: {} };
    for (const button of ["volume_up", "volume_down"] as const) {
      if (kind !== "missing") {
        mappings.actions[button] = {
          single: kind === "shortcut" ? { type: "shortcut", chord: { keys: [button] } } : { type: "disabled" },
          double: { type: "disabled" },
          long: { type: "disabled" },
        };
      }
    }
    vi.mocked(getButtonMappings).mockResolvedValueOnce(mappings);
    const wrapper = await mountPage();
    await flushPromises();
    for (const label of ["音量+", "音量−"]) {
      const card = wrapper.findAll(".mapping-card").find((item) => item.find(".mapping-card-title strong").text() === label)!;
      const cells = card.findAll(".mapping-cell");
      expect(cells[0]!.get("span").text()).toBe(kind === "shortcut" ? label : "未设置");
      expect(cells[1]!.get("span").text()).toBe("未设置");
      expect(cells[2]!.get("span").text()).toBe("未设置");
    }
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("saves a Codex shortcut from the formerly disabled volume editor without triggering it", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "音量+", 1);
    const preset = wrapper.get('[data-action-id="previous-chat"]');
    await preset.trigger("click");
    await flushPromises();
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.volume_up?.double).toEqual({ type: "shortcut", chord: { keys: ["control", "alt", "bracket_left"] } });
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("highlights enhanced back and volume edges without adding or executing mappings", async () => {
    const wrapper = await mountPage();
    await flushPromises();
    for (const [button, label] of [["back", "返回"], ["volume_up", "音量+"], ["volume_down", "音量−"]] as const) {
      const card = wrapper.findAll(".mapping-card").find((item) => item.find(".mapping-card-title strong").text() === label)!;
      edgeHandler!({ button, isPressed: true });
      await wrapper.vm.$nextTick();
      expect(card.classes()).toContain("active");
      edgeHandler!({ button, isPressed: false });
      await wrapper.vm.$nextTick();
      expect(card.classes()).not.toContain("active");
    }
    expect(wrapper.find(".rc003-input-control").exists()).toBe(true);
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("uses normal Backspace for single and held presses, keeping double deletion optional", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "返回", 0);
    await wrapper.get(".backspace-actions .chip").trigger("click");
    await flushPromises();
    let saved = vi.mocked(saveButtonMappings).mock.lastCall![0];
    expect(saved.actions.back).toEqual({
      single: { type: "normal_backspace" },
      double: { type: "disabled" },
      long: { type: "disabled" },
    });
    const backCard = wrapper.findAll(".mapping-card").find((card) => card.find(".mapping-card-title strong").text() === "返回")!;
    const hold = backCard.findAll(".mapping-cell")[2]!;
    expect((hold.element as HTMLButtonElement).disabled).toBe(true);
    expect(hold.text()).toContain("Backspace");
    expect(wrapper.text()).toContain("当前单按立即退格");
    expect(wrapper.find(".punctuation-note").exists()).toBe(false);
    expect(wrapper.get(".gesture-timing-note").text()).not.toContain("撤销");

    await backCard.findAll(".mapping-cell")[1]!.trigger("click");
    expect(wrapper.get(".backspace-actions").text()).toContain("保留标点");
    await wrapper.get(".backspace-actions .chip").trigger("click");
    await flushPromises();
    saved = vi.mocked(saveButtonMappings).mock.lastCall![0];
    expect(saved.actions.back?.double).toEqual({ type: "delete_to_punctuation" });
    expect(saved.actions.back?.single).toEqual({ type: "normal_backspace" });
    expect(wrapper.get(".punctuation-note").text()).toContain("保留标点");
    expect(wrapper.get(".gesture-timing-note").text()).toContain("第一击先普通退格，不等待双击判定窗口");
    expect(wrapper.get(".mapping-editor").text()).not.toContain("等待约 0.3 秒");
    expect(wrapper.get(".gesture-timing-note").text()).not.toContain("撤销");
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("places the single RC003 enhancement switch immediately above the remote diagram", async () => {
    const wrapper = await mountPage();
    const controls = wrapper.findAll(".rc003-input-control");
    expect(controls).toHaveLength(1);
    expect(wrapper.get(".mapping-canvas").element.previousElementSibling).toBe(controls[0]!.element);
    expect(controls[0]!.get('[role="switch"]').attributes("aria-label")).toBe("RC003 按键增强");
    expect(controls[0]!.text()).toContain("需要管理员权限");
    expect(wrapper.get(".back-hardware-note").text()).toContain("上方的按键增强");
    expect(wrapper.text()).not.toContain("下方启用按键增强");
    expect(wrapper.text()).not.toContain("系统音量原始行为");
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("does not offer the enhancement switch for confirmed RC001", async () => {
    const wrapper = await mountPage("rc001");
    expect(wrapper.find(".rc003-input-control").exists()).toBe(false);
    expect(wrapper.find('[role="switch"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("keeps the unknown model enhancement visible with a disabled switch", async () => {
    const wrapper = await mountPage("unknown");
    const control = wrapper.get(".rc003-input-control");
    expect(control.get('[role="status"]').text()).toBe("型号尚未确认");
    expect(control.get('[role="switch"]').attributes("disabled")).toBeDefined();
    expect(wrapper.get(".mapping-canvas").element.previousElementSibling).toBe(control.element);
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("saves Undo on Back double and updates its hint when changed or disabled without sending keys", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "返回", 0);
    await wrapper.get(".backspace-actions .chip").trigger("click");
    await flushPromises();
    await openCell(wrapper, "返回", 1);
    await chooseAction(wrapper, "撤销");
    await flushPromises();
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.back).toEqual({
      single: { type: "normal_backspace" },
      double: { type: "shortcut", chord: { keys: ["control", "z"] } },
      long: { type: "disabled" },
    });
    expect(wrapper.get(".gesture-timing-note").text()).toContain("第一击立即退格，双击时第二击发送一次 Ctrl + Z 撤销");
    expect(wrapper.get(".gesture-timing-note").text()).toContain("具体撤销内容由当前应用决定");
    expect(wrapper.get(".mapping-editor").text()).not.toContain("等待约 0.3 秒");
    expect(wrapper.find(".punctuation-note").exists()).toBe(false);
    await openCell(wrapper, "返回", 0);
    expect(wrapper.get(".gesture-timing-note").text()).toContain("第一击立即退格");

    await openCell(wrapper, "返回", 1);
    await chooseAction(wrapper, "粘贴");
    await flushPromises();
    expect(wrapper.get(".gesture-timing-note").text()).toContain("单击等待约 0.3 秒");
    expect(wrapper.get(".gesture-timing-note").text()).not.toContain("撤销");
    expect(wrapper.find(".punctuation-note").exists()).toBe(false);
    await wrapper.get(".editor-disable-btn").trigger("click");
    await flushPromises();
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.back?.double).toEqual({ type: "disabled" });
    expect(wrapper.get(".gesture-timing-note").text()).toContain("当前单按立即退格");
    expect(wrapper.get(".gesture-timing-note").text()).not.toContain("撤销");
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it.each([0, 1, 2])("freely assigns and disables Undo on another button's trigger %i", async triggerIndex => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", triggerIndex);
    await chooseAction(wrapper, "撤销");
    await flushPromises();
    const trigger = (["single", "double", "long"] as const)[triggerIndex]!;
    const saved = vi.mocked(saveButtonMappings).mock.lastCall![0];
    expect(saved.actions.power?.[trigger]).toEqual({ type: "shortcut", chord: { keys: ["control", "z"] } });
    expect(saved.actions.back).toBeUndefined();
    expect(wrapper.get(".gesture-timing-note").text()).not.toContain("第一击立即退格");
    if (trigger === "double") expect(wrapper.get(".gesture-timing-note").text()).toContain("双击判定窗口约 0.3 秒");
    await wrapper.get(".editor-disable-btn").trigger("click");
    await flushPromises();
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.power?.[trigger]).toEqual({ type: "disabled" });
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it.each([
    { keys: ["z", "left_control"], eager: true },
    { keys: ["right_control", "z"], eager: true },
    { keys: ["control", "shift", "z"], eager: false },
  ])("matches the current imported Back chord timing for $keys", async ({ keys, eager }) => {
    vi.mocked(getButtonMappings).mockResolvedValueOnce({
      enabled: true,
      actions: { back: {
        single: { type: "normal_backspace" },
        double: { type: "shortcut", chord: { keys } },
        long: { type: "disabled" },
      } },
    } satisfies ButtonMappings);
    const wrapper = await mountPage();
    await openCell(wrapper, "返回", 1);
    const hint = wrapper.get(".gesture-timing-note").text();
    expect(hint.includes("第一击立即退格")).toBe(eager);
    expect(hint.includes("等待约 0.3 秒")).toBe(!eager);
    expect(wrapper.find(".punctuation-note").exists()).toBe(false);
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("restores default Backspace and explains the default hold behavior", async () => {
    const wrapper = await mountPage();
    await wrapper.findAll("button").find((button) => button.text() === "恢复默认配置")!.trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("返回键按住连续删除，确定和电源键长按插队/排队");
    const backCard = wrapper.findAll(".mapping-card").find((card) => card.find(".mapping-card-title strong").text() === "返回")!;
    expect(backCard.text()).toContain("Backspace");
    expect(backCard.findAll(".mapping-cell")[2]!.text()).toContain("Backspace");
    expect((backCard.findAll(".mapping-cell")[2]!.element as HTMLButtonElement).disabled).toBe(true);
    wrapper.unmount();
  });
});
