import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AudioEndpoint, AudioSnapshot, ConnectionSnapshot, KeyChord, MicrophoneEndpoint, MicrophoneSelectionSnapshot, RuntimeSnapshot } from "../lib/bridge";
import ConnectionPage from "./ConnectionPage.vue";
import { runMicrophoneJourney } from "../runtime-simulation";

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

const emptyMicrophoneSelection: MicrophoneSelectionSnapshot = {
  targetMicrophoneId: null, targetMicrophoneName: null, currentMicrophoneId: null, currentMicrophoneName: null,
  phase: "unconfigured", lastError: null, restorePending: false,
};

function selected(microphone: MicrophoneEndpoint): MicrophoneSelectionSnapshot {
  return { targetMicrophoneId: microphone.id, targetMicrophoneName: microphone.name,
    currentMicrophoneId: microphone.id, currentMicrophoneName: microphone.name,
    phase: "ready", lastError: null, restorePending: true };
}

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
  audio: {} as AudioSnapshot,
  selection: {} as MicrophoneSelectionSnapshot,
  microphones: [] as MicrophoneEndpoint[],
  getConnectionSnapshot: vi.fn(),
  getAudioSnapshot: vi.fn(),
  getMicrophoneSelection: vi.fn(),
  selectMicrophone: vi.fn(),
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
    getMicrophoneSelection: mocks.getMicrophoneSelection,
    selectMicrophone: mocks.selectMicrophone,
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
    mocks.audio = { ...emptyAudio };
    mocks.selection = { ...emptyMicrophoneSelection };
    mocks.microphones = [];
    mocks.getConnectionSnapshot.mockResolvedValue(emptyConnection);
    mocks.getAudioSnapshot.mockImplementation(async () => ({ ...mocks.audio }));
    mocks.getMicrophoneSelection.mockImplementation(async () => ({ ...mocks.selection }));
    mocks.selectMicrophone.mockImplementation(async (microphoneId: string) => {
      const microphone = mocks.microphones.find((item) => item.id === microphoneId)!;
      mocks.selection = selected(microphone);
      if (microphone.isVirtualCableCandidate) {
        mocks.audio = { ...emptyAudio, phase: "ready", selectedEndpointId: microphone.renderEndpointIds[0] ?? null, selectedEndpointName: cableEndpoint.name };
      }
      return { ...mocks.selection };
    });
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

  function prepareCable() {
    mocks.microphones = [cableMicrophone, physicalMicrophone];
    mocks.selection = selected(cableMicrophone);
    mocks.audio = { ...emptyAudio, phase: "ready", selectedEndpointId: cableEndpoint.id, selectedEndpointName: cableEndpoint.name };
  }

  function microphoneButton(wrapper: ReturnType<typeof mount>, name: string) {
    return wrapper.findAll(".endpoint-list li").find((row) => row.text().includes(name))!.get("button");
  }

  it("runs the CI microphone journey against the current page, including both capture choices", async () => {
    prepareCable();
    mocks.endpoints = [cableEndpoint];
    const wrapper = mount(ConnectionPage, { props: { runtime }, attachTo: document.body });
    try {
      await flushPromises();
      const steps: string[] = [];
      await runMicrophoneJourney(steps);
      expect(steps).toHaveLength(2);
      expect(mocks.selectMicrophone.mock.calls).toEqual([[physicalMicrophone.id], [cableMicrophone.id]]);
      expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
      expect(wrapper.get(".audio-route-details").attributes("open")).toBeUndefined();
    } finally {
      wrapper.unmount();
      wrapper.element.remove();
    }
  });

  it("fails the CI journey when the IPC selection disagrees with the ready UI", async () => {
    prepareCable();
    mocks.endpoints = [cableEndpoint];
    const wrapper = mount(ConnectionPage, { props: { runtime }, attachTo: document.body });
    try {
      await flushPromises();
      mocks.getMicrophoneSelection.mockResolvedValue({ ...mocks.selection, currentMicrophoneId: physicalMicrophone.id });
      await expect(runMicrophoneJourney([])).rejects.toThrow("默认录音端点与目标不一致");
      expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    } finally {
      wrapper.unmount();
      wrapper.element.remove();
    }
  });

  it("shows the saved virtual microphone and all other capture choices without automatically selecting", async () => {
    prepareCable();
    mocks.microphones[0] = { ...cableMicrophone, renderEndpointIds: [cableEndpoint.id, "cable-speakers"] };
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const card = wrapper.get(".audio-settings-card");
    expect(card.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(card.get(".microphone-status").text()).toBe("已就绪");
    expect(card.text()).toContain("目标麦克风");
    expect(card.text()).toContain("启动时切换到所选麦克风");
    expect(card.get(".microphone-target-hint").text()).toContain("退出时优先恢复原麦克风；不可用时保留 Windows 当前选择");
    expect(card.get(".audio-route-details").text()).toContain("退出时优先恢复原麦克风；不可用时保留 Windows 当前选择");
    expect(card.text()).not.toContain("退出时恢复原设置");
    expect(card.text()).toContain("系统默认麦克风");
    expect(card.get(".endpoint-list").text()).not.toContain(cableMicrophone.name);
    expect(microphoneButton(wrapper, physicalMicrophone.name).text()).toBe("选择");
    expect(card.get(".microphone-target").text()).not.toContain("CABLE Input");
    expect(card.get(".audio-route-details").attributes("open")).toBeUndefined();
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(mocks.listAudioEndpoints).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("uses the capture selection command for physical microphones without opening a render route", async () => {
    prepareCable();
    mocks.audio.phase = "failed";
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    expect(mocks.selectMicrophone).toHaveBeenCalledExactlyOnceWith(physicalMicrophone.id);
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(mocks.retryAudioEndpoint).not.toHaveBeenCalled();
    expect(wrapper.get(".microphone-name").text()).toBe(physicalMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
    expect(wrapper.get(".endpoint-list").text()).toContain(cableMicrophone.name);
    wrapper.unmount();
  });

  it("selects a virtual microphone by its capture ID and lets the host prepare its route", async () => {
    prepareCable();
    mocks.selection = selected(physicalMicrophone);
    mocks.audio = { ...emptyAudio };
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, cableMicrophone.name).trigger("click");
    await flushPromises();
    expect(mocks.selectMicrophone).toHaveBeenCalledExactlyOnceWith(cableMicrophone.id);
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    wrapper.unmount();
  });

  it("does not infer microphones from render endpoints or select a first device on page mount", async () => {
    mocks.endpoints = [cableEndpoint, speakerEndpoint];
    mocks.microphones = [physicalMicrophone];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-name").text()).toBe("尚未选择");
    expect(wrapper.get(".endpoint-list").text()).not.toContain(speakerEndpoint.name);
    expect(wrapper.get(".endpoint-list").text()).not.toContain(cableEndpoint.name);
    expect(microphoneButton(wrapper, physicalMicrophone.name).attributes("disabled")).toBeUndefined();
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("keeps an unpaired virtual microphone visible and reports a host rejection without claiming it is usable", async () => {
    mocks.microphones = [{ ...cableMicrophone, renderEndpointIds: [] }, physicalMicrophone];
    mocks.selection = selected(physicalMicrophone);
    mocks.selectMicrophone.mockRejectedValueOnce(new Error("microphone_render_route_unavailable"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".endpoint-list").text()).toContain("遥控器传声线路暂不可用");
    await microphoneButton(wrapper, cableMicrophone.name).trigger("click");
    await flushPromises();
    expect(mocks.selectMicrophone).toHaveBeenCalledExactlyOnceWith(cableMicrophone.id);
    expect(wrapper.get(".microphone-name").text()).toBe(physicalMicrophone.name);
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toContain("麦克风切换失败");
    expect(wrapper.get(".audio-route-details").text()).toContain("microphone_render_route_unavailable");
    wrapper.unmount();
  });

  it("keeps the saved target separate from an externally changed system default until the user reapplies it", async () => {
    prepareCable();
    mocks.selection = { ...mocks.selection, phase: "external", currentMicrophoneId: physicalMicrophone.id, currentMicrophoneName: physicalMicrophone.name };
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-current").text()).toContain(physicalMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("系统麦克风已更改");
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    await wrapper.get(".selected-microphone-row button").trigger("click");
    await flushPromises();
    expect(mocks.selectMicrophone).toHaveBeenCalledExactlyOnceWith(cableMicrophone.id);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(wrapper.find(".microphone-current").exists()).toBe(false);
    wrapper.unmount();
  });

  it("reports an externally changed capture role without repeating an unchanged Console device name", async () => {
    prepareCable();
    mocks.selection.phase = "external";
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("系统麦克风已更改");
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.find(".microphone-current").exists()).toBe(false);
    expect(wrapper.get(".selected-microphone-row button").text()).toBe("使用这个麦克风");
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("does not report readiness from a ready phase when the current and saved capture IDs differ", async () => {
    prepareCable();
    mocks.selection.currentMicrophoneId = physicalMicrophone.id;
    mocks.selection.currentMicrophoneName = physicalMicrophone.name;
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("系统麦克风已更改");
    wrapper.unmount();
  });

  it("does not name an unknown current device as another microphone", async () => {
    prepareCable();
    mocks.selection = { ...mocks.selection, phase: "external", currentMicrophoneId: null, currentMicrophoneName: null };
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("当前麦克风待确认");
    expect(wrapper.find(".microphone-current").exists()).toBe(false);
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it.each([false, true])("reports an empty capture inventory explicitly (saved target: %s)", async (savedTarget) => {
    if (savedTarget) {
      mocks.selection = { ...selected(cableMicrophone), phase: "external", currentMicrophoneId: null, currentMicrophoneName: null };
    }
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("没有可用麦克风");
    expect(wrapper.findAll(".endpoint-list li")).toHaveLength(0);
    expect(wrapper.find(".microphone-retry").exists()).toBe(false);
    expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("distinguishes an empty capture inventory from a pending or failed device read", async () => {
    let resolveInventory!: (microphones: MicrophoneEndpoint[]) => void;
    mocks.listMicrophones.mockImplementationOnce(() => new Promise((resolve) => { resolveInventory = resolve; }));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).not.toBe("没有可用麦克风");
    expect(wrapper.get(".microphone-empty").text()).toBe("正在读取…");
    resolveInventory([]);
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("没有可用麦克风");
    mocks.listMicrophones.mockRejectedValueOnce(new Error("capture enumeration failed"));
    await wrapper.findAll("button").find((button) => button.text() === "刷新设备列表")!.trigger("click");
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("等待刷新");
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toContain("麦克风列表暂时无法更新");
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("recovers an initial selection getter failure through polling without writing system defaults", async () => {
    vi.useFakeTimers();
    prepareCable();
    mocks.getMicrophoneSelection.mockRejectedValueOnce(new Error("read failed"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("状态待确认");
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toContain("正在重新读取");
    expect(microphoneButton(wrapper, physicalMicrophone.name).attributes("disabled")).toBeDefined();
    await vi.advanceTimersByTimeAsync(1000);
    await flushPromises();
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(microphoneButton(wrapper, physicalMicrophone.name).attributes("disabled")).toBeUndefined();
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("retains the target name but stops claiming current readiness after a polling getter failure", async () => {
    vi.useFakeTimers();
    prepareCable();
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    mocks.getMicrophoneSelection.mockRejectedValueOnce(new Error("poll failed"));
    await vi.advanceTimersByTimeAsync(1000);
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("状态待确认");
    await vi.advanceTimersByTimeAsync(1000);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("ignores a stale getter response arriving after a newer explicit selection", async () => {
    vi.useFakeTimers();
    prepareCable();
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    let resolveRead!: (snapshot: MicrophoneSelectionSnapshot) => void;
    mocks.getMicrophoneSelection.mockImplementationOnce(() => new Promise((resolve) => { resolveRead = resolve; }));
    await vi.advanceTimersByTimeAsync(1000);
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    resolveRead(selected(cableMicrophone));
    await flushPromises();
    expect(wrapper.get(".microphone-name").text()).toBe(physicalMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    wrapper.unmount();
  });

  it("keeps known names visible but disables stale choices until automatic inventory refresh succeeds", async () => {
    vi.useFakeTimers();
    prepareCable();
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    mocks.listMicrophones.mockRejectedValueOnce(new Error("capture enumeration failure"));
    await vi.advanceTimersByTimeAsync(10000);
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("等待刷新");
    expect(microphoneButton(wrapper, physicalMicrophone.name).attributes("disabled")).toBeDefined();
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toBe("麦克风列表暂时无法更新，正在自动重试。");
    await vi.advanceTimersByTimeAsync(10000);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(microphoneButton(wrapper, physicalMicrophone.name).attributes("disabled")).toBeUndefined();
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(3);
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(mocks.retryAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("does not treat a returned failed selection snapshot as success", async () => {
    prepareCable();
    mocks.selectMicrophone.mockResolvedValueOnce({ ...selected(physicalMicrophone), phase: "failed", lastError: "default_verify_failed" });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("暂不可用");
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toContain("切换失败");
    expect(wrapper.findAll(".audio-route-details .error-text").map((item) => item.text())).toEqual(["default_verify_failed"]);
    wrapper.unmount();
  });

  it("deduplicates shared diagnostic failures while retaining a distinct route failure", async () => {
    prepareCable();
    mocks.selectMicrophone.mockImplementationOnce(async () => {
      mocks.selection = { ...selected(physicalMicrophone), phase: "failed", lastError: "default_restore_failed" };
      mocks.audio = { ...mocks.audio, phase: "failed", lastError: "audio_route_failed" };
      throw new Error("default_restore_failed");
    });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    expect(wrapper.findAll(".audio-route-details .error-text").map((item) => item.text()))
      .toEqual(["default_restore_failed", "audio_route_failed"]);
    wrapper.unmount();
  });

  it("retains a specific save failure across getter polls and retries the requested capture target", async () => {
    vi.useFakeTimers();
    prepareCable();
    mocks.selectMicrophone.mockImplementationOnce(async () => {
      mocks.selection = { ...selected(cableMicrophone), currentMicrophoneId: physicalMicrophone.id, currentMicrophoneName: physicalMicrophone.name, phase: "external" };
      throw new Error("microphone_settings_save_failed");
    });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-current").text()).toContain(physicalMicrophone.name);
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toBe("麦克风已切换，但设置未保存。");
    mocks.selection = selected(physicalMicrophone);
    await vi.advanceTimersByTimeAsync(10000);
    expect(wrapper.get(".microphone-status").text()).toBe("设置未保存");
    expect(wrapper.get(".audio-route-details").text()).toContain("microphone_settings_save_failed");
    const retry = wrapper.get(".microphone-retry button");
    expect(retry.text()).toBe("重试保存设置");
    await retry.trigger("click");
    await flushPromises();
    expect(mocks.selectMicrophone.mock.calls).toEqual([[physicalMicrophone.id], [physicalMicrophone.id]]);
    expect(wrapper.get(".microphone-name").text()).toBe(physicalMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("keeps an uncertain selection failure during its readback, then clears it after a later confirmed poll", async () => {
    vi.useFakeTimers();
    prepareCable();
    mocks.selectMicrophone.mockImplementationOnce(async () => {
      mocks.selection = selected(physicalMicrophone);
      throw new Error("default_verify_failed");
    });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toBe("麦克风切换失败，可重试。");
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).not.toContain("未保存");
    await vi.advanceTimersByTimeAsync(1000);
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
    expect(wrapper.find(".microphone-retry").exists()).toBe(false);
    expect(wrapper.find(".audio-route-details .error-text").exists()).toBe(false);
    expect(mocks.selectMicrophone).toHaveBeenCalledOnce();
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({
      event: "microphone_selection", result: "passed", reason: "selection_confirmed_by_readback",
    }));
    await vi.advanceTimersByTimeAsync(1000);
    expect(mocks.reportFrontendEvent.mock.calls.filter(([event]) => event.reason === "selection_confirmed_by_readback")).toHaveLength(1);
    wrapper.unmount();
  });

  it.each([
    ["different target", selected(cableMicrophone)],
    ["different current device", { ...selected(physicalMicrophone), currentMicrophoneId: cableMicrophone.id }],
    ["unconfirmed phase", { ...selected(physicalMicrophone), phase: "external" }],
  ] as const)("retains the failed requested selection when polling reports a %s", async (_, snapshot) => {
    vi.useFakeTimers();
    prepareCable();
    mocks.selectMicrophone.mockRejectedValueOnce(new Error("default_verify_failed"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    mocks.selection = { ...snapshot };
    await vi.advanceTimersByTimeAsync(1000);
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("暂不可用");
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toBe("麦克风切换失败，可重试。");
    expect(wrapper.get(".microphone-retry button").text()).toBe("重试");
    expect(mocks.reportFrontendEvent).not.toHaveBeenCalledWith(expect.objectContaining({
      reason: "selection_confirmed_by_readback",
    }));
    wrapper.unmount();
  });

  it("preserves the saved name when its capture device disappears without presenting stale readiness", async () => {
    prepareCable();
    mocks.microphones = [physicalMicrophone];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("设备暂不可用");
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("requires a matching ready render route for a virtual microphone even when the Windows default is correct", async () => {
    prepareCable();
    mocks.audio.phase = "failed";
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("传声线路暂不可用");
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toContain("遥控器语音线路暂不可用");
    await wrapper.get(".microphone-retry button").trigger("click");
    await flushPromises();
    expect(mocks.selectMicrophone).toHaveBeenCalledExactlyOnceWith(cableMicrophone.id);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    wrapper.unmount();
  });

  it("shows a virtual route as unconfirmed after an audio read failure without blocking a physical target", async () => {
    prepareCable();
    mocks.getAudioSnapshot.mockRejectedValueOnce(new Error("audio read failed"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("传声线路待确认");
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    wrapper.unmount();
  });

  it("defers microphone selection and inventory refresh during an active voice session", async () => {
    vi.useFakeTimers();
    prepareCable();
    mocks.audio.phase = "streaming";
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(microphoneButton(wrapper, physicalMicrophone.name).attributes("disabled")).toBeDefined();
    await vi.advanceTimersByTimeAsync(10000);
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(1);
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    mocks.audio.phase = "ready";
    await vi.advanceTimersByTimeAsync(2000);
    expect(mocks.listMicrophones).toHaveBeenCalledTimes(2);
    expect(microphoneButton(wrapper, physicalMicrophone.name).attributes("disabled")).toBeUndefined();
    wrapper.unmount();
  });

  it("does not schedule inventory or write defaults after unmount while the initial selection read is pending", async () => {
    vi.useFakeTimers();
    let resolveRead!: (snapshot: MicrophoneSelectionSnapshot) => void;
    mocks.getMicrophoneSelection.mockImplementationOnce(() => new Promise((resolve) => { resolveRead = resolve; }));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    wrapper.unmount();
    resolveRead(selected(cableMicrophone));
    await flushPromises();
    await vi.advanceTimersByTimeAsync(20000);
    expect(mocks.getMicrophoneSelection).toHaveBeenCalledOnce();
    expect(mocks.listMicrophones).not.toHaveBeenCalled();
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
  });

  it.each([false, true])("accepts either paired render interface after host selection (alternate interface: %s)", async (useAlternate) => {
    prepareCable();
    const alternate = { ...cableEndpoint, id: "cable-speakers", name: "Speakers (VB-Audio Virtual Cable)" };
    mocks.microphones[0] = { ...cableMicrophone, renderEndpointIds: [cableEndpoint.id, alternate.id] };
    mocks.selection = selected(physicalMicrophone);
    mocks.selectMicrophone.mockImplementationOnce(async () => {
      const opened = useAlternate ? alternate : cableEndpoint;
      mocks.audio = { ...emptyAudio, phase: "ready", selectedEndpointId: opened.id, selectedEndpointName: opened.name };
      mocks.selection = selected(cableMicrophone);
      return { ...mocks.selection };
    });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, cableMicrophone.name).trigger("click");
    await flushPromises();
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(mocks.selectMicrophone).toHaveBeenCalledExactlyOnceWith(cableMicrophone.id);
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
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
    expect(guide.text()).toContain("目标麦克风");
    expect(guide.text()).not.toContain("CABLE Input");
    expect(guide.text()).toContain("系统默认麦克风");
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

describe("Microphone browser preview", () => {
  it("simulates physical and virtual capture selection without requiring native IPC", async () => {
    const bridge = await vi.importActual<typeof import("../lib/bridge")>("../lib/bridge");
    expect(bridge.isTauriRuntime()).toBe(false);
    const [captures, renders] = await Promise.all([bridge.listMicrophones(), bridge.listAudioEndpoints()]);
    const virtual = captures.find((microphone) => microphone.isVirtualCableCandidate)!;
    const physical = captures.find((microphone) => !microphone.isVirtualCableCandidate)!;
    expect(renders.map((endpoint) => endpoint.id)).toContain(virtual.renderEndpointIds[0]);
    expect((await bridge.selectMicrophone(virtual.id)).targetMicrophoneId).toBe(virtual.id);
    expect((await bridge.getAudioSnapshot()).phase).toBe("ready");
    const routeBeforePhysicalSelection = { ...await bridge.getAudioSnapshot() };
    expect((await bridge.selectMicrophone(physical.id)).currentMicrophoneId).toBe(physical.id);
    expect((await bridge.getMicrophoneSelection()).phase).toBe("ready");
    expect(await bridge.getAudioSnapshot()).toEqual(routeBeforePhysicalSelection);
    expect((await bridge.listMicrophones()).find((microphone) => microphone.id === physical.id)?.isDefault).toBe(true);
  });

  it("loads and refreshes the page using the actual preview inventory without losing capture choices", async () => {
    const bridge = await vi.importActual<typeof import("../lib/bridge")>("../lib/bridge");
    vi.resetAllMocks();
    mocks.getConnectionSnapshot.mockResolvedValue(emptyConnection);
    mocks.getVoiceHoldHotkey.mockResolvedValue(wetypeChord);
    mocks.getAudioSnapshot.mockImplementation(bridge.getAudioSnapshot);
    mocks.getMicrophoneSelection.mockImplementation(bridge.getMicrophoneSelection);
    mocks.listMicrophones.mockImplementation(bridge.listMicrophones);
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    try {
      await flushPromises();
      const captures = await bridge.listMicrophones();
      const current = await bridge.getMicrophoneSelection();
      expect(wrapper.get(".microphone-name").text()).toBe(current.targetMicrophoneName);
      expect(wrapper.findAll(".endpoint-list li")).toHaveLength(captures.length - 1);
      await wrapper.findAll("button").find((button) => button.text() === "刷新设备列表")!.trigger("click");
      await flushPromises();
      expect(mocks.listMicrophones).toHaveBeenCalledTimes(2);
      expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
      expect(wrapper.findAll(".endpoint-list li")).toHaveLength(captures.length - 1);
      expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
      expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    } finally {
      wrapper.unmount();
    }
  });
});
