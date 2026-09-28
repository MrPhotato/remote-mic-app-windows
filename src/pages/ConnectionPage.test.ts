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
const speakerEndpoint: AudioEndpoint = {
  id: "saved-speaker",
  name: "已保存的扬声器",
  isVirtualCableCandidate: false,
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
      selectedEndpointName: mocks.endpoints.find((endpoint) => endpoint.id === endpointId)?.name ?? cableEndpoint.name,
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
    expect(wrapper.get(".audio-settings-card").text()).toContain("声音传送已就绪");
    expect(wrapper.text()).not.toContain("CABLE Input");
    expect(wrapper.text()).not.toContain("需要安装 VB-CABLE");
    expect(wrapper.text()).not.toContain("系统语音输入");
    wrapper.unmount();
  });

  it("shows the remote microphone and target microphone with internal routing collapsed by default", async () => {
    mocks.endpoints = [cableEndpoint, speakerEndpoint];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();

    const audioCard = wrapper.get(".audio-settings-card");
    expect(audioCard.get(".status-heading strong").text()).toBe("遥控器麦克风");
    expect(audioCard.text()).toContain("语音来源");
    expect(audioCard.get(".setting-row").text()).toContain("目标应用使用的麦克风");
    expect(audioCard.get(".setting-row span").text()).toBe("CABLE Output");
    expect(audioCard.text()).toContain("此处仅提示设置");
    expect(audioCard.text()).toContain("电脑扬声器保持原有设置");
    expect(audioCard.text()).not.toContain("CABLE Input");
    expect(audioCard.text()).not.toContain(speakerEndpoint.name);
    expect(audioCard.find("#advanced-audio-settings").exists()).toBe(false);
    const advanced = audioCard.get('[aria-controls="advanced-audio-settings"]');
    expect(advanced.attributes("aria-expanded")).toBe("false");

    await advanced.trigger("click");
    expect(advanced.attributes("aria-expanded")).toBe("true");
    expect(audioCard.get("#advanced-audio-settings").text()).toContain("已自动选择 CABLE Input");
    expect(audioCard.get("#advanced-audio-settings").text()).toContain("声音传送设备");
    expect(audioCard.get("#advanced-audio-settings").text()).toContain("不会修改 Windows 默认扬声器或麦克风");

    await advanced.trigger("click");
    expect(audioCard.find("#advanced-audio-settings").exists()).toBe(false);
    expect(mocks.selectAudioEndpoint).toHaveBeenCalledExactlyOnceWith(cableEndpoint.id);
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "audio_advanced_settings", reason: "collapsed" }));
    wrapper.unmount();
  });

  it("never selects a generic speaker automatically but preserves explicit advanced routing", async () => {
    mocks.endpoints = [speakerEndpoint];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(wrapper.text()).not.toContain(speakerEndpoint.name);

    await wrapper.get('[aria-controls="advanced-audio-settings"]').trigger("click");
    await wrapper.get('[aria-controls="audio-endpoint-list"]').trigger("click");
    const speakerRow = wrapper.get(".endpoint-list li");
    expect(speakerRow.text()).toContain(speakerEndpoint.name);
    expect(speakerRow.text()).toContain("仅用于试听或自定义传送");
    await speakerRow.get("button").trigger("click");
    await flushPromises();

    expect(mocks.selectAudioEndpoint).toHaveBeenCalledExactlyOnceWith(speakerEndpoint.id);
    expect(wrapper.get("#advanced-audio-settings").text()).toContain(speakerEndpoint.name);
    expect(wrapper.get(".audio-settings-card .status-heading strong").text()).toBe("遥控器麦克风");
    expect(wrapper.get(".audio-settings-card .status-panel").text()).toContain("已保留原有声音传送设置");
    expect(wrapper.get(".audio-settings-card").text()).not.toContain("声音传送已就绪");
    wrapper.unmount();
  });

  it("keeps multiple virtual routes unselected until the user chooses one in advanced settings", async () => {
    const secondCable = { ...cableEndpoint, id: "second-cable-input", name: "CABLE Input (第二个虚拟声卡)" };
    mocks.endpoints = [cableEndpoint, secondCable];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(wrapper.get(".audio-settings-card").text()).toContain("请在高级设置中确认声音传送设备");

    await wrapper.get('[aria-controls="advanced-audio-settings"]').trigger("click");
    await wrapper.get('[aria-controls="audio-endpoint-list"]').trigger("click");
    await wrapper.findAll(".endpoint-list li")[1].get("button").trigger("click");
    await flushPromises();

    expect(mocks.selectAudioEndpoint).toHaveBeenCalledExactlyOnceWith(secondCable.id);
    expect(wrapper.get("#advanced-audio-settings").text()).toContain(secondCable.name);
    expect(wrapper.find(".endpoint-list").exists()).toBe(false);
    wrapper.unmount();
  });

  it("does not overwrite saved settings when their initial read fails", async () => {
    mocks.endpoints = [cableEndpoint];
    mocks.getAudioSnapshot.mockRejectedValueOnce(new Error("设置暂时无法读取"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();

    expect(mocks.listAudioEndpoints).toHaveBeenCalledOnce();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
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
    expect(wrapper.get(".audio-settings-card .status-panel").text()).toContain("已保留原有声音传送设置");
    expect(wrapper.get(".audio-settings-card").text()).not.toContain("声音传送已就绪");
    await wrapper.get('[aria-controls="advanced-audio-settings"]').trigger("click");
    expect(wrapper.get("#advanced-audio-settings").text()).toContain(savedAudio.selectedEndpointName);
    await wrapper.findAll("button").find((button) => button.text() === "刷新设备列表")!.trigger("click");
    await flushPromises();
    expect(mocks.listAudioEndpoints).toHaveBeenCalledTimes(2);
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(wrapper.get("#advanced-audio-settings").text()).toContain(savedAudio.selectedEndpointName);
    wrapper.unmount();
  });

  it("shows enumeration failure in the main view instead of driver installation advice and can refresh", async () => {
    mocks.endpoints = [cableEndpoint];
    mocks.getAudioSnapshot.mockResolvedValue({
      ...emptyAudio,
      phase: "ready",
      selectedEndpointId: cableEndpoint.id,
      selectedEndpointName: cableEndpoint.name,
    });
    mocks.listAudioEndpoints.mockRejectedValueOnce(new Error("音频服务暂时不可用"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();

    const audioCard = wrapper.get(".audio-settings-card");
    expect(audioCard.get('[role="alert"]').text()).toContain("声音传送设备读取失败：音频服务暂时不可用");
    expect(audioCard.find("#advanced-audio-settings").exists()).toBe(false);
    expect(audioCard.find(".vb-cable-callout").exists()).toBe(false);
    expect(audioCard.text()).not.toContain("需要安装 VB-CABLE");
    expect(audioCard.text()).not.toContain("正在准备声音传送");
    expect(audioCard.text()).not.toContain("声音传送已就绪");
    expect(audioCard.get(".status-dot").classes()).toContain("error");
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();

    await audioCard.get('[aria-controls="advanced-audio-settings"]').trigger("click");
    await audioCard.findAll("button").find((button) => button.text() === "刷新设备列表")!.trigger("click");
    await flushPromises();

    expect(audioCard.find('[role="alert"]').exists()).toBe(false);
    expect(audioCard.text()).toContain("声音传送已就绪");
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "audio_route_detection", result: "failed", reason: "endpoint_enumeration_failed" }));
    wrapper.unmount();
  });

  it("shows automatic route selection failure outside advanced settings and allows a manual retry", async () => {
    mocks.endpoints = [cableEndpoint];
    mocks.selectAudioEndpoint.mockRejectedValueOnce(new Error("设备正忙"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();

    const audioCard = wrapper.get(".audio-settings-card");
    expect(audioCard.get('[role="alert"]').text()).toContain("声音传送设备启用失败：设备正忙");
    expect(audioCard.find("#advanced-audio-settings").exists()).toBe(false);
    expect(audioCard.text()).not.toContain("正在准备声音传送");
    expect(audioCard.text()).not.toContain("需要安装 VB-CABLE");
    expect(audioCard.get(".status-dot").classes()).toContain("error");
    expect(mocks.selectAudioEndpoint).toHaveBeenCalledExactlyOnceWith(cableEndpoint.id);

    await audioCard.get('[aria-controls="advanced-audio-settings"]').trigger("click");
    await audioCard.get('[aria-controls="audio-endpoint-list"]').trigger("click");
    await audioCard.get(".endpoint-list li button").trigger("click");
    await flushPromises();

    expect(audioCard.find('[role="alert"]').exists()).toBe(false);
    expect(audioCard.text()).toContain("声音传送已就绪");
    expect(mocks.selectAudioEndpoint).toHaveBeenCalledTimes(2);
    expect(mocks.selectAudioEndpoint).toHaveBeenLastCalledWith(cableEndpoint.id);
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
    expect(guide.text()).toContain("遥控器麦克风");
    expect(guide.text()).not.toContain("CABLE Input");
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
