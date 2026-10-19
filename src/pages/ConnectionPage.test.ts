import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AudioEndpoint, AudioSnapshot, ConnectionSnapshot, KeyChord, MicrophoneEndpoint, RuntimeSnapshot } from "../lib/bridge";
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
const cableMicrophone: MicrophoneEndpoint = {
  id: "cable-output",
  name: "CABLE Output (2- VB-Audio Virtual Cable)",
  isVirtualCableCandidate: true,
  isDefault: true,
  isDefaultCommunications: false,
  renderEndpointIds: [cableEndpoint.id],
};
const physicalMicrophone: MicrophoneEndpoint = {
  id: "microphone-array",
  name: "Microphone Array (Realtek Audio)",
  isVirtualCableCandidate: false,
  isDefault: false,
  isDefaultCommunications: true,
  renderEndpointIds: [],
};

const mocks = vi.hoisted(() => ({
  endpoints: [] as AudioEndpoint[],
  microphones: [] as MicrophoneEndpoint[],
  getConnectionSnapshot: vi.fn(),
  getAudioSnapshot: vi.fn(),
  listAudioEndpoints: vi.fn(),
  listMicrophones: vi.fn(),
  retryAudioEndpoint: vi.fn(),
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
    listMicrophones: mocks.listMicrophones,
    retryAudioEndpoint: mocks.retryAudioEndpoint,
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
    mocks.microphones = [];
    mocks.getConnectionSnapshot.mockResolvedValue(emptyConnection);
    mocks.getAudioSnapshot.mockResolvedValue(emptyAudio);
    mocks.listAudioEndpoints.mockImplementation(async () => mocks.endpoints);
    mocks.listMicrophones.mockImplementation(async () => mocks.microphones);
    mocks.retryAudioEndpoint.mockResolvedValue(emptyAudio);
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
    vi.useRealTimers();
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

  it("selects one virtual microphone even when its cable has two render interfaces", async () => {
    const cableSpeakers = { ...cableEndpoint, id: "cable-speakers", name: "Speakers (2- VB-Audio Virtual Cable)" };
    mocks.endpoints = [cableSpeakers, cableEndpoint, speakerEndpoint];
    mocks.microphones = [{ ...cableMicrophone, renderEndpointIds: [cableEndpoint.id, cableSpeakers.id] }, physicalMicrophone];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();

    expect(mocks.selectAudioEndpoint).toHaveBeenCalledExactlyOnceWith(cableEndpoint.id);
    const card = wrapper.get(".audio-settings-card");
    expect(card.get(".microphone-status").text()).toContain("已就绪");
    expect(card.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(card.text()).toContain("目标虚拟麦克风");
    expect(card.text()).toContain("在 Codex 或语音输入法中选择这个麦克风");
    expect(card.get(".microphone-target").text()).not.toContain("CABLE Input");
    expect(card.get(".microphone-target").text()).not.toContain("Speakers");
    expect(card.get(".audio-route-details").attributes("open")).toBeUndefined();
    expect(card.text()).not.toContain("高级设置");

    const rows = card.findAll(".endpoint-list li");
    expect(rows).toHaveLength(1);
    expect(rows[0].text()).toContain(physicalMicrophone.name);
    expect(rows[0].text()).toContain("在目标应用中选择");
    expect(rows[0].find("button").exists()).toBe(false);
    expect(card.get(".endpoint-list").text()).not.toContain(cableMicrophone.name);
    expect(card.get(".audio-route-details").text()).toContain("默认通信设备");
    expect(card.get(".endpoint-list").text()).not.toContain("Speakers");
    expect(card.get(".endpoint-list").text()).not.toContain("CABLE Input");
    wrapper.unmount();
  });

  it("does not infer an installed microphone from render names or offer a speaker as a microphone", async () => {
    mocks.endpoints = [cableEndpoint, speakerEndpoint];
    mocks.microphones = [physicalMicrophone];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const card = wrapper.get(".audio-settings-card");
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(card.get(".microphone-name").text()).toBe("尚未选择");
    expect(card.text()).not.toContain("需要安装 VB-CABLE");
    expect(card.text()).toContain("未检测到可用的 VB-CABLE 麦克风");
    await card.findAll("button").find((button) => button.text() === "刷新设备列表")!.trigger("click");
    await flushPromises();
    expect(card.get(".endpoint-list").text()).toContain(physicalMicrophone.name);
    expect(card.get(".endpoint-list").text()).not.toContain(speakerEndpoint.name);
    expect(card.get(".endpoint-list").find("button").exists()).toBe(false);
    wrapper.unmount();
  });

  it("keeps distinct virtual microphones unselected until the user chooses one", async () => {
    const otherRender = { ...cableEndpoint, id: "other-input", name: "Other cable input" };
    const otherMicrophone = { ...cableMicrophone, id: "other-output", name: "Other cable microphone", renderEndpointIds: [otherRender.id] };
    mocks.endpoints = [cableEndpoint, otherRender];
    mocks.microphones = [cableMicrophone, otherMicrophone];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    await wrapper.findAll(".endpoint-list li")[1].get("button").trigger("click");
    await flushPromises();
    expect(mocks.selectAudioEndpoint).toHaveBeenCalledExactlyOnceWith(otherRender.id);
    expect(wrapper.get(".audio-settings-card .microphone-name").text()).toBe(otherMicrophone.name);
    expect(wrapper.get(".endpoint-list").text()).toContain(cableMicrophone.name);
    expect(wrapper.get(".endpoint-list").text()).not.toContain(otherMicrophone.name);
    expect(wrapper.get(".endpoint-list button").text()).toBe("选择");
    wrapper.unmount();
  });

  it("does not overwrite settings when their initial read fails", async () => {
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone];
    mocks.getAudioSnapshot.mockRejectedValueOnce(new Error("设置暂时无法读取"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(mocks.listMicrophones).toHaveBeenCalledOnce();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("preserves an unavailable saved route until explicit recovery and then shows the actual paired microphone", async () => {
    const failedAudio: AudioSnapshot = { ...emptyAudio, phase: "failed", selectedEndpointId: "absent-cable-input", selectedEndpointName: "Old cable input" };
    let resolveAudio: ((snapshot: AudioSnapshot) => void) | undefined;
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone];
    mocks.getAudioSnapshot.mockImplementationOnce(() => new Promise((resolve) => { resolveAudio = resolve; }));
    mocks.retryAudioEndpoint.mockResolvedValue({ ...failedAudio, phase: "ready", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(mocks.listMicrophones).not.toHaveBeenCalled();
    resolveAudio?.(failedAudio);
    await flushPromises();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    const card = wrapper.get(".audio-settings-card");
    expect(card.get(".microphone-name").text()).toBe("尚未选择");
    expect(card.get(".microphone-status").text()).not.toContain("已就绪");
    await card.findAll("button").find((button) => button.text() === "重试恢复语音线路")!.trigger("click");
    await flushPromises();
    expect(mocks.retryAudioEndpoint).toHaveBeenCalledOnce();
    expect(card.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(card.get(".microphone-status").text()).toContain("已就绪");
    expect(card.text()).not.toContain("重试恢复语音线路");
    wrapper.unmount();
  });

  it("allows reselection of the same microphone when the saved stream failed", async () => {
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone];
    mocks.getAudioSnapshot.mockResolvedValue({ ...emptyAudio, phase: "failed", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const retry = wrapper.get(".selected-microphone-row button");
    expect(retry.text()).toBe("重试");
    expect(retry.attributes("disabled")).toBeUndefined();
    await retry.trigger("click");
    await flushPromises();
    expect(mocks.selectAudioEndpoint).toHaveBeenCalledExactlyOnceWith(cableEndpoint.id);
    expect(wrapper.get(".microphone-status").text()).toContain("已就绪");
    wrapper.unmount();
  });

  it("shows capture enumeration errors instead of suggesting a driver install, and refresh recovers", async () => {
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone];
    mocks.getAudioSnapshot.mockResolvedValue({ ...emptyAudio, phase: "ready", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name });
    mocks.listMicrophones.mockRejectedValueOnce(new Error("音频服务暂时不可用"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const card = wrapper.get(".audio-settings-card");
    expect(card.get('[role="alert"]').text()).toContain("正在自动重试");
    expect(card.get(".audio-route-details").text()).toContain("麦克风读取失败：音频服务暂时不可用");
    expect(card.find(".vb-cable-callout").exists()).toBe(false);
    expect(card.get(".microphone-status").text()).not.toContain("已就绪");
    await card.findAll("button").find((button) => button.text() === "刷新设备列表")!.trigger("click");
    await flushPromises();
    expect(card.find('[role="alert"]').exists()).toBe(false);
    expect(card.get(".microphone-status").text()).toContain("已就绪");
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("keeps route activation failure visible and allows a manual retry", async () => {
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone];
    mocks.selectAudioEndpoint.mockRejectedValueOnce(new Error("设备正忙"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const card = wrapper.get(".audio-settings-card");
    expect(card.get('[role="alert"]').text()).toContain("麦克风启用失败");
    await card.get(".endpoint-list li button").trigger("click");
    await flushPromises();
    expect(card.find('[role="alert"]').exists()).toBe(false);
    expect(card.get(".microphone-status").text()).toContain("已就绪");
    expect(mocks.selectAudioEndpoint).toHaveBeenCalledTimes(2);
    wrapper.unmount();
  });

  it("does not claim VB-CABLE is uninstalled when no active device was returned", async () => {
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const card = wrapper.get(".audio-settings-card");
    expect(card.text()).toContain("未检测到可用的 VB-CABLE 麦克风");
    expect(card.text()).not.toContain("需要安装 VB-CABLE");
    expect(card.text()).not.toContain("重启电脑");
    await card.get(".audio-route-details button").trigger("click");
    await flushPromises();
    expect(mocks.openVbCableDownloadPage).toHaveBeenCalledOnce();
    wrapper.unmount();
  });

  it("preserves a selection failure while the changed failed state refreshes inventory", async () => {
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone];
    mocks.selectAudioEndpoint.mockRejectedValueOnce(new Error("设备正忙"));
    mocks.getAudioSnapshot.mockResolvedValueOnce(emptyAudio).mockResolvedValue({
      ...emptyAudio, phase: "failed", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name,
    });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(2);
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toContain("麦克风启用失败");
    wrapper.unmount();
  });

  it("refreshes microphone identity when background recovery selects a replacement route", async () => {
    vi.useFakeTimers();
    const replacementRender = { ...cableEndpoint, id: "replacement-input" };
    const replacementMic = { ...cableMicrophone, id: "replacement-output", name: "Recovered cable microphone", renderEndpointIds: [replacementRender.id] };
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone];
    mocks.getAudioSnapshot.mockResolvedValue({ ...emptyAudio, phase: "failed", selectedEndpointId: "old-input", selectedEndpointName: "Old cable" });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    mocks.endpoints = [replacementRender];
    mocks.microphones = [replacementMic];
    mocks.getAudioSnapshot.mockResolvedValue({ ...emptyAudio, phase: "ready", selectedEndpointId: replacementRender.id, selectedEndpointName: replacementRender.name });
    await vi.advanceTimersByTimeAsync(1000);
    await flushPromises();
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(2);
    expect(wrapper.get(".audio-settings-card .microphone-name").text()).toBe(replacementMic.name);
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("refreshes the capture list when a parent runtime snapshot announces recovery", async () => {
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [];
    mocks.getAudioSnapshot.mockResolvedValue({ ...emptyAudio, phase: "failed", selectedEndpointId: cableEndpoint.id });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    mocks.microphones = [cableMicrophone];
    await wrapper.setProps({ runtime: { ...runtime, platform: { ...runtime.platform, audio: {
      ...emptyAudio, phase: "ready", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name,
    } } } });
    await flushPromises();
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(2);
    expect(wrapper.get(".audio-settings-card .microphone-name").text()).toBe(cableMicrophone.name);
    wrapper.unmount();
  });

  it("updates default roles and capture-only changes every ten seconds without reopening a ready route", async () => {
    vi.useFakeTimers();
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone, physicalMicrophone];
    mocks.getAudioSnapshot.mockResolvedValue({ ...emptyAudio, phase: "ready", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    mocks.microphones = [{ ...cableMicrophone, isDefault: false }, { ...physicalMicrophone, isDefault: true }];
    await vi.advanceTimersByTimeAsync(9000);
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1000);
    await flushPromises();
    const roles = wrapper.get(".audio-route-details");
    expect(roles.text()).toContain(`${physicalMicrophone.name}：Windows 默认录音设备`);
    expect(roles.text()).not.toContain(`${cableMicrophone.name}：Windows 默认录音设备`);
    expect(wrapper.get(".endpoint-list").text()).toContain(physicalMicrophone.name);
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(2);
    mocks.microphones = [physicalMicrophone];
    await vi.advanceTimersByTimeAsync(10000);
    await flushPromises();
    expect(wrapper.get(".audio-settings-card .microphone-name").text()).toBe("尚未选择");
    expect(wrapper.get(".microphone-status").text()).not.toContain("已就绪");
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(mocks.retryAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
    await vi.advanceTimersByTimeAsync(30000);
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(3);
    expect(mocks.getAudioSnapshot).toHaveBeenCalledTimes(21);
  });

  it("keeps known names visible but disables stale choices until the next automatic refresh", async () => {
    vi.useFakeTimers();
    const otherRender = { ...cableEndpoint, id: "other-input" };
    const otherMicrophone = { ...cableMicrophone, id: "other-output", name: "Other cable microphone", renderEndpointIds: [otherRender.id] };
    mocks.endpoints = [cableEndpoint, otherRender];
    mocks.microphones = [cableMicrophone, otherMicrophone, physicalMicrophone];
    mocks.getAudioSnapshot.mockResolvedValue({ ...emptyAudio, phase: "ready", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".endpoint-list button").attributes("disabled")).toBeUndefined();

    mocks.listMicrophones.mockRejectedValueOnce(new Error("temporary capture enumeration failure"));
    await vi.advanceTimersByTimeAsync(10000);
    await flushPromises();
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("等待刷新");
    expect(wrapper.get(".endpoint-list").text()).toContain(otherMicrophone.name);
    expect(wrapper.get(".endpoint-list").text()).toContain(physicalMicrophone.name);
    expect(wrapper.get(".endpoint-list button").attributes("disabled")).toBeDefined();
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toBe("麦克风列表暂时无法更新，正在自动重试。");
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).not.toContain("temporary capture");
    await wrapper.get(".endpoint-list button").trigger("click");
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(10000);
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(wrapper.get(".endpoint-list button").attributes("disabled")).toBeUndefined();
    expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(3);
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(mocks.retryAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("defers the periodic microphone refresh until an active voice session has ended", async () => {
    vi.useFakeTimers();
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone];
    const streaming: AudioSnapshot = { ...emptyAudio, phase: "streaming", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name };
    mocks.getAudioSnapshot.mockResolvedValue(streaming);
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await vi.advanceTimersByTimeAsync(10000);
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(1);
    mocks.getAudioSnapshot.mockResolvedValue({ ...streaming, phase: "ready" });
    await vi.advanceTimersByTimeAsync(2000);
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(2);
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("retries a failed initial microphone enumeration when runtime recovery changes the route", async () => {
    vi.useFakeTimers();
    mocks.endpoints = [cableEndpoint];
    mocks.microphones = [cableMicrophone];
    mocks.listMicrophones.mockRejectedValueOnce(new Error("音频服务暂时不可用"));
    mocks.getAudioSnapshot.mockResolvedValue({ ...emptyAudio, phase: "failed", selectedEndpointId: "old-route" });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toContain("正在自动重试");
    mocks.getAudioSnapshot.mockResolvedValue({ ...emptyAudio, phase: "ready", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name });
    await vi.advanceTimersByTimeAsync(1000);
    await flushPromises();
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(2);
    expect(wrapper.get(".audio-settings-card .microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("does not auto-select or schedule more inventory work after unmount during the initial read", async () => {
    vi.useFakeTimers();
    mocks.endpoints = [cableEndpoint];
    let resolveMicrophones!: (microphones: MicrophoneEndpoint[]) => void;
    mocks.listMicrophones.mockImplementationOnce(() => new Promise((resolve) => { resolveMicrophones = resolve; }));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    wrapper.unmount();
    resolveMicrophones([cableMicrophone]);
    await flushPromises();
    await vi.advanceTimersByTimeAsync(20000);
    expect(mocks.listMicrophones).toHaveBeenCalledOnce();
    expect(mocks.getAudioSnapshot).toHaveBeenCalledOnce();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
  });

  it.each([false, true])("distinguishes an enabled route with unsaved settings and retries saving (alternate interface: %s)", async (useAlternate) => {
    vi.useFakeTimers();
    const alternate = { ...cableEndpoint, id: "cable-speakers", name: "Speakers (VB-Audio Virtual Cable)" };
    mocks.endpoints = [cableEndpoint, alternate];
    mocks.microphones = [{ ...cableMicrophone, renderEndpointIds: [cableEndpoint.id, alternate.id] }];
    const opened = useAlternate ? alternate : cableEndpoint;
    const ready: AudioSnapshot = { ...emptyAudio, phase: "ready", selectedEndpointId: opened.id, selectedEndpointName: opened.name };
    mocks.selectAudioEndpoint.mockRejectedValueOnce(new Error("保存设置失败"));
    mocks.getAudioSnapshot.mockResolvedValueOnce(emptyAudio).mockResolvedValue(ready);
    mocks.retryAudioEndpoint.mockResolvedValue(ready);
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toContain("已启用，但设置未保存");
    expect(wrapper.get(".audio-settings-card").text()).not.toContain("线路启用失败");
    await vi.advanceTimersByTimeAsync(10000);
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toContain("已启用，但设置未保存");
    await wrapper.findAll("button").find((button) => button.text() === "重试保存设置")!.trigger("click");
    await flushPromises();
    expect(mocks.retryAudioEndpoint).toHaveBeenCalledOnce();
    expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
    expect(wrapper.get(".microphone-status").text()).toContain("已就绪");
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
