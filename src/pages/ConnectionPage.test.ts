import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AudioEndpoint, AudioSnapshot, ConnectionSnapshot, KeyChord, RuntimeSnapshot } from "../lib/bridge";
import ConnectionPage from "./ConnectionPage.vue";

const emptyConnection: ConnectionSnapshot = {
  phase: "idle",
  remoteName: null,
  remoteModel: "unknown",
  capabilities: null,
  voiceState: "idle",
  decodedSamples: 0,
  generation: 0,
  reconnectAttempt: 0,
  powerNotificationsAvailable: false,
  lastError: null,
};

const emptyAudio: AudioSnapshot = {
  phase: "unconfigured",
  selectedEndpointId: null,
  selectedEndpointName: null,
  queuedSamples: 0,
  submittedSamples: 0,
  generation: 0,
  lastError: null,
};

const runtime: RuntimeSnapshot = {
  appVersion: "0.1.0",
  platform: {
    platform: "windows",
    windowsApiAvailable: true,
    bleScanAvailable: true,
    bleVoiceReady: false,
    wasapiReady: false,
    rawInputReady: false,
    sendInputReady: true,
    verificationStatus: "测试",
    connection: emptyConnection,
    audio: emptyAudio,
    rawInput: {
      phase: "stopped",
      matchedDeviceCount: 0,
      rawEventCount: 0,
      semanticEdgeCount: 0,
      lastButton: null,
      lastIsPressed: null,
      activeButtons: [],
      lastError: null,
    },
    buttonMapping: {
      enabled: true,
      gateActive: false,
      listenerActive: false,
      swallowedEdges: 0,
      leakedDowns: 0,
      firedGestures: 0,
      lastFired: null,
      lastError: null,
    },
  },
};

const cableEndpoint: AudioEndpoint = {
  id: "cable-input",
  name: "CABLE Input (VB-Audio Virtual Cable)",
  isVirtualCableCandidate: true,
};

const mocks = vi.hoisted(() => ({
  endpoints: [] as AudioEndpoint[],
  getConnectionSnapshot: vi.fn(),
  getAudioSnapshot: vi.fn(),
  listAudioEndpoints: vi.fn(),
  selectAudioEndpoint: vi.fn(),
  openVbCableDownloadPage: vi.fn(),
  getVoiceHoldHotkey: vi.fn(),
  setVoiceHoldHotkey: vi.fn(),
  reportFrontendEvent: vi.fn(),
}));

vi.mock("../lib/bridge", async (importOriginal) => {
  const original = await importOriginal<typeof import("../lib/bridge")>();
  return {
    ...original,
    getConnectionSnapshot: mocks.getConnectionSnapshot,
    getAudioSnapshot: mocks.getAudioSnapshot,
    listAudioEndpoints: mocks.listAudioEndpoints,
    selectAudioEndpoint: mocks.selectAudioEndpoint,
    openVbCableDownloadPage: mocks.openVbCableDownloadPage,
    getVoiceHoldHotkey: mocks.getVoiceHoldHotkey,
    setVoiceHoldHotkey: mocks.setVoiceHoldHotkey,
  };
});
vi.mock("../lib/frontend-diagnostics", () => ({ reportFrontendEvent: mocks.reportFrontendEvent }));

const wetypeChord: KeyChord = { keys: ["left_control", "left_windows"] };
const codexChord: KeyChord = { keys: ["left_control", "left_shift", "d"] };

describe("Connection and voice settings", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.endpoints = [];
    mocks.getConnectionSnapshot.mockResolvedValue(emptyConnection);
    mocks.getAudioSnapshot.mockResolvedValue(emptyAudio);
    mocks.listAudioEndpoints.mockImplementation(async () => mocks.endpoints);
    mocks.selectAudioEndpoint.mockImplementation(async (endpointId: string) => ({
      ...emptyAudio,
      phase: "ready",
      selectedEndpointId: endpointId,
      selectedEndpointName: cableEndpoint.name,
    }));
    mocks.openVbCableDownloadPage.mockResolvedValue(undefined);
    mocks.getVoiceHoldHotkey.mockResolvedValue(wetypeChord);
    mocks.setVoiceHoldHotkey.mockImplementation(async (chord: KeyChord | null) => chord);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("groups each status dot with its heading for vertical alignment", async () => {
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();

    const headings = wrapper.findAll(".status-heading");
    expect(headings).toHaveLength(2);
    for (const heading of headings) {
      expect(heading.find(".status-dot").exists()).toBe(true);
      expect(heading.find("strong").exists()).toBe(true);
    }
    wrapper.unmount();
  });

  it("automatically selects the only VB-CABLE endpoint when no endpoint was configured", async () => {
    mocks.endpoints = [cableEndpoint];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();

    expect(mocks.selectAudioEndpoint).toHaveBeenCalledOnce();
    expect(mocks.selectAudioEndpoint).toHaveBeenCalledWith(cableEndpoint.id);
    expect(wrapper.text()).toContain("已自动选择 CABLE Input");
    expect(wrapper.text()).not.toContain("需要安装 VB-CABLE");
    expect(wrapper.text()).not.toContain("系统语音输入");
    wrapper.unmount();
  });

  it("waits for the saved endpoint and does not replace an existing selection", async () => {
    const savedAudio: AudioSnapshot = {
      ...emptyAudio,
      phase: "ready",
      selectedEndpointId: "saved-speaker",
      selectedEndpointName: "已保存的扬声器",
    };
    let resolveAudio: ((snapshot: AudioSnapshot) => void) | undefined;
    mocks.endpoints = [cableEndpoint];
    mocks.getAudioSnapshot.mockImplementationOnce(
      () =>
        new Promise<AudioSnapshot>((resolve) => {
          resolveAudio = resolve;
        }),
    );

    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(mocks.listAudioEndpoints).not.toHaveBeenCalled();

    resolveAudio?.(savedAudio);
    await flushPromises();

    expect(mocks.listAudioEndpoints).toHaveBeenCalledOnce();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("shows the official installation action when VB-CABLE is unavailable", async () => {
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();

    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("需要安装 VB-CABLE");
    expect(wrapper.text()).toContain("完成后需重启电脑");

    await wrapper.get(".vb-cable-callout .primary-button").trigger("click");
    await flushPromises();
    expect(mocks.openVbCableDownloadPage).toHaveBeenCalledOnce();
    wrapper.unmount();
  });

  it.each([
    [wetypeChord, "微信输入法（默认）"],
    [{ keys: ["d", "left_shift", "left_control"] }, "Codex 听写 · Ctrl + Shift + D"],
    [null, "关闭"],
    [{ keys: ["right_alt"] }, null],
  ] as const)("restores the saved hotkey %j without overwriting it", async (chord, selectedLabel) => {
    mocks.getVoiceHoldHotkey.mockResolvedValue(chord);
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();

    const selected = wrapper.findAll('.voice-hotkey-presets button[aria-pressed="true"]');
    expect(selected.map((button) => button.text())).toEqual(selectedLabel ? [selectedLabel] : []);
    expect(mocks.setVoiceHoldHotkey).not.toHaveBeenCalled();
    expect(wrapper.text()).not.toContain("尚未读取快捷键设置");
    wrapper.unmount();
  });

  it("waits for loading before marking or changing a voice preset", async () => {
    let resolveLoad!: (value: KeyChord | null) => void;
    mocks.getVoiceHoldHotkey.mockImplementationOnce(() => new Promise((resolve) => { resolveLoad = resolve; }));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const presets = wrapper.findAll(".voice-hotkey-presets button");
    expect(presets).toHaveLength(3);
    expect(presets.every((button) => button.attributes("disabled") !== undefined)).toBe(true);
    expect(presets.every((button) => button.attributes("aria-pressed") === "false")).toBe(true);
    resolveLoad(codexChord);
    await flushPromises();
    expect(wrapper.get('.voice-hotkey-presets button[aria-pressed="true"]').text()).toContain("Codex 听写");
    wrapper.unmount();
  });

  it("persists the Codex hold chord and shows its instructions only after saving", async () => {
    let resolveSave!: (value: KeyChord) => void;
    mocks.setVoiceHoldHotkey.mockImplementationOnce(() => new Promise((resolve) => { resolveSave = resolve; }));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const codex = wrapper.findAll(".voice-hotkey-presets button").find((button) => button.text().startsWith("Codex"))!;
    await codex.trigger("click");
    await flushPromises();

    expect(mocks.setVoiceHoldHotkey).toHaveBeenCalledExactlyOnceWith(codexChord);
    expect(wrapper.text()).toContain("正在保存按住说话快捷键");
    expect(wrapper.text()).not.toContain("按住说话快捷键已设为");
    expect(wrapper.findAll(".voice-hotkey-presets button").every((button) => button.attributes("disabled") !== undefined)).toBe(true);
    resolveSave(codexChord);
    await flushPromises();

    expect(codex.attributes("aria-pressed")).toBe("true");
    expect(wrapper.get('.scan-summary[role="status"]').text()).toContain("按住说话快捷键已设为");
    const guide = wrapper.get(".usage-hint-details");
    expect(guide.text()).toContain("将 Codex 切到前台，点击输入框");
    expect(guide.text()).toContain("CABLE Input");
    expect(guide.text()).toContain("CABLE Output");
    expect(guide.text()).toContain("不需要微信输入法");
    expect(guide.text()).toContain("仍需要 VB-CABLE");
    expect(guide.text()).toContain("本程序不会自动发送");
    expect(wrapper.text()).not.toContain("微信输入法使用步骤");
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "voice_hotkey_preset_save", result: "passed", reason: "preset_codex_saved" }));
    wrapper.unmount();
  });

  it.each([
    ["微信输入法（默认）", wetypeChord],
    ["关闭", null],
  ] as const)("can persist %s after Codex dictation was selected", async (label, expected) => {
    mocks.getVoiceHoldHotkey.mockResolvedValue(codexChord);
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const preset = wrapper.findAll(".voice-hotkey-presets button").find((button) => button.text() === label)!;
    await preset.trigger("click");
    await flushPromises();

    expect(mocks.setVoiceHoldHotkey).toHaveBeenCalledExactlyOnceWith(expected);
    expect(preset.attributes("aria-pressed")).toBe("true");
    expect(wrapper.text()).not.toContain("Codex 听写使用步骤");
    wrapper.unmount();
  });

  it("reports save failure and rereads the persisted selection", async () => {
    mocks.setVoiceHoldHotkey.mockRejectedValueOnce(new Error("保存暂时不可用"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await wrapper.findAll(".voice-hotkey-presets button").find((button) => button.text().startsWith("Codex"))!.trigger("click");
    await flushPromises();

    expect(mocks.getVoiceHoldHotkey).toHaveBeenCalledTimes(2);
    expect(wrapper.get('[role="alert"]').text()).toContain("保存失败：保存暂时不可用");
    expect(wrapper.get('.voice-hotkey-presets button[aria-pressed="true"]').text()).toBe("微信输入法（默认）");
    expect(wrapper.text()).not.toContain("按住说话快捷键已设为");
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "voice_hotkey_preset_save", result: "failed", reason: "settings_save_failed" }));
    wrapper.unmount();
  });

  it("uses readback after an uncertain save and keeps the failure visible", async () => {
    mocks.getVoiceHoldHotkey.mockResolvedValueOnce(wetypeChord).mockResolvedValueOnce(codexChord);
    mocks.setVoiceHoldHotkey.mockRejectedValueOnce(new Error("响应中断"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await wrapper.findAll(".voice-hotkey-presets button").find((button) => button.text().startsWith("Codex"))!.trigger("click");
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain("响应中断");
    expect(wrapper.get('.voice-hotkey-presets button[aria-pressed="true"]').text()).toContain("Codex");
    expect(wrapper.text()).not.toContain("按住说话快捷键已设为");
    wrapper.unmount();
  });

  it("shows a retry when reading fails without pretending the shortcut is off", async () => {
    mocks.getVoiceHoldHotkey.mockRejectedValueOnce(new Error("无法读取设置"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("无法读取设置");
    expect(wrapper.find('.voice-hotkey-presets button[aria-pressed="true"]').exists()).toBe(false);
    expect(wrapper.findAll(".voice-hotkey-presets button").every((button) => button.attributes("disabled") !== undefined)).toBe(true);

    await wrapper.findAll("button").find((button) => button.text() === "重新读取快捷键")!.trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.get('.voice-hotkey-presets button[aria-pressed="true"]').text()).toBe("微信输入法（默认）");
    wrapper.unmount();
  });
});
