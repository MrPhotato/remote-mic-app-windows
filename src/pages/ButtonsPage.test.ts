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
  importButtonMappingConfiguration,
  subscribeButtonEdges,
  subscribeButtonGestures,
  saveButtonMappings,
  startShortcutCapture,
  stopShortcutCapture,
  testButtonMapping,
} from "../lib/bridge";
import type { ButtonMappings, RuntimeSnapshot } from "../lib/bridge";

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
  it("browses the separate Codex catalog without saving or sending shortcuts", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 0);
    expect(wrapper.get(".codex-shortcuts").text()).toContain("Codex 快捷键");
    expect(wrapper.get('[data-shortcut-id="previous-chat"]').text()).toContain("Ctrl + Page Up");
    await wrapper.get('select[aria-label="Codex 快捷键分类"]').setValue("input");
    expect(wrapper.get('[data-shortcut-id="approve"]').text()).toContain("输入框内 Enter 可能发送消息");
    expect(wrapper.get(".codex-shortcuts-footnote").text()).toContain("连接与语音");
    await wrapper.get('input[aria-label="搜索 Codex 快捷键"]').setValue("快捷键");
    expect(wrapper.get('[data-shortcut-id="keyboard-shortcuts"]').text()).toContain("查看快捷键");
    expect(wrapper.find('[data-shortcut-id="approve"]').exists()).toBe(false);
    expect(saveButtonMappings).not.toHaveBeenCalled();
    expect(testButtonMapping).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("saves a Codex option only to the selected gesture without executing it", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "电源", 1);
    await wrapper.get('[data-shortcut-id="next-chat"]').trigger("click");
    await flushPromises();
    const saved = vi.mocked(saveButtonMappings).mock.lastCall![0];
    expect(saved.actions.power?.double).toEqual({ type: "shortcut", chord: { keys: ["control", "page_down"] } });
    expect(saved.actions.ok?.single).toEqual({ type: "shortcut", chord: { keys: ["enter"] } });
    expect(wrapper.get('[data-shortcut-id="next-chat"]').attributes("aria-pressed")).toBe("true");
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
    const chips = editor.findAll(".chip");
    const escapeChip = chips.find((chip) => chip.text() === "Esc");
    await escapeChip!.trigger("click");
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
      expect(wrapper.text()).toContain("已录入 左 Win + L，松开全部按键后完成"),
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
  function chipState(wrapper: VueWrapper, label: string): boolean {
    const chip = wrapper
      .findAll(".mapping-editor .chip")
      .find((element) => element.text().includes(label));
    expect(chip, `未找到 chip：${label}`).toBeDefined();
    return (chip!.element as HTMLButtonElement).disabled;
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
    expect(chipState(wrapper, "Enter")).toBe(false);
    expect(chipState(wrapper, "Home")).toBe(false);
    expect(chipState(wrapper, "空格")).toBe(false);
    expect(chipState(wrapper, "粘贴")).toBe(false);
    expect(chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(chipState(wrapper, "＋ 添加应用")).toBe(false);
    // 武装族按键显示冷首按原生副作用提示（信息性，不门控）。
    expect(wrapper.find(".mapping-editor").text()).toContain("原生按键动作");
  });

  it("全开放：确定·双击与 TV 所有操作可配 + 各自的单响应提示", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "确定", 1);
    expect(chipState(wrapper, "Enter")).toBe(false);
    expect(chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(chipState(wrapper, "＋ 添加应用")).toBe(false);
    expect(wrapper.find(".mapping-editor").text()).toContain("原生按键动作");

    await openCell(wrapper, "TV", 0);
    expect(chipState(wrapper, "Enter")).toBe(false);
    expect(chipState(wrapper, "静音")).toBe(false);
    expect(chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(chipState(wrapper, "＋ 添加应用")).toBe(false);
    expect(wrapper.find(".mapping-editor").text()).toContain("遥控器优先");
  });

  it("左键与其余方向键同样开放自定义并显示结构性泄漏提示", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "左", 0);
    expect(chipState(wrapper, "←")).toBe(false);
    expect(chipState(wrapper, "退格")).toBe(false);
    expect(chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(wrapper.find(".mapping-editor").text()).toContain("原生按键动作");

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
    expect(chipState(wrapper, "Esc")).toBe(false);
    expect(chipState(wrapper, "截图")).toBe(false);
    expect(chipState(wrapper, "录入自定义快捷键")).toBe(false);
    expect(chipState(wrapper, "＋ 添加应用")).toBe(false);
    expect(wrapper.find(".mapping-editor").text()).not.toContain("原生按键动作");
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
    const preset = wrapper.findAll(".codex-shortcut").find(b => b.text().includes("上一个会话或标签页"))!;
    await preset.trigger("click");
    await flushPromises();
    expect(vi.mocked(saveButtonMappings).mock.lastCall![0].actions.volume_up?.double).toEqual({ type: "shortcut", chord: { keys: ["control", "page_up"] } });
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
    expect(hold.text()).toContain("持续退格");
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

  it("saves Undo on Back double and updates its hint when changed or disabled without sending keys", async () => {
    const wrapper = await mountPage();
    await openCell(wrapper, "返回", 0);
    await wrapper.get(".backspace-actions .chip").trigger("click");
    await flushPromises();
    await openCell(wrapper, "返回", 1);
    await wrapper.findAll(".mapping-editor .chip").find(button => button.text() === "撤销")!.trigger("click");
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
    await wrapper.findAll(".mapping-editor .chip").find(button => button.text() === "粘贴")!.trigger("click");
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
    await wrapper.findAll(".mapping-editor .chip").find(button => button.text() === "撤销")!.trigger("click");
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
    await wrapper.findAll("button").find((button) => button.text() === "恢复基础配置")!.trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("返回键单按退格、按住连续删除，其余按键保持原样");
    const backCard = wrapper.findAll(".mapping-card").find((card) => card.find(".mapping-card-title strong").text() === "返回")!;
    expect(backCard.text()).toContain("退格（按住连续删除）");
    expect(backCard.findAll(".mapping-cell")[2]!.text()).toContain("持续退格");
    expect((backCard.findAll(".mapping-cell")[2]!.element as HTMLButtonElement).disabled).toBe(true);
    wrapper.unmount();
  });
});
