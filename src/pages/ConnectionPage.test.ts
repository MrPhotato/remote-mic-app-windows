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

function waitingForConnection(target: MicrophoneEndpoint, current: MicrophoneEndpoint): MicrophoneSelectionSnapshot {
  return { targetMicrophoneId: target.id, targetMicrophoneName: target.name,
    currentMicrophoneId: current.id, currentMicrophoneName: current.name,
    phase: "waiting_connection", lastError: null, restorePending: false };
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
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
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

  function observeSelectionAnimation(wrapper: ReturnType<typeof mount>) {
    const row = wrapper.get(".selected-microphone-row").element as HTMLElement;
    const animations: { cancel: ReturnType<typeof vi.fn>; finish: () => void }[] = [];
    const animate = vi.fn(() => {
      let finish!: () => void;
      let reject!: (reason: Error) => void;
      const finished = new Promise<void>((resolve, rejectPromise) => { finish = resolve; reject = rejectPromise; });
      const cancel = vi.fn(() => reject(new Error("cancelled")));
      animations.push({ cancel, finish });
      return { finished, cancel };
    });
    Object.defineProperty(row, "animate", { configurable: true, value: animate });
    return { animate, animations };
  }

  it("only acknowledges a confirmed choice, cancels superseded feedback and cleans up on unmount", async () => {
    vi.useFakeTimers();
    prepareCable();
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const feedback = observeSelectionAnimation(wrapper);
    await vi.advanceTimersByTimeAsync(1000);
    expect(feedback.animate).not.toHaveBeenCalled();
    let resolveSelection!: (snapshot: MicrophoneSelectionSnapshot) => void;
    mocks.selectMicrophone.mockImplementationOnce(() => new Promise(resolve => { resolveSelection = resolve; }));
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    expect(feedback.animate).not.toHaveBeenCalled();
    mocks.selection = selected(physicalMicrophone);
    resolveSelection(mocks.selection);
    await flushPromises();
    expect(feedback.animate).toHaveBeenCalledTimes(1);
    expect(wrapper.get(".selected-microphone-row button").text()).toBe("使用中");
    await microphoneButton(wrapper, cableMicrophone.name).trigger("click");
    await flushPromises();
    expect(feedback.animations[0].cancel).toHaveBeenCalledOnce();
    expect(feedback.animate).toHaveBeenCalledTimes(2);
    wrapper.unmount();
    expect(feedback.animations[1].cancel).toHaveBeenCalledOnce();
    await flushPromises();
  });

  it("acknowledges an offline save without claiming it is the active microphone", async () => {
    prepareCable();
    mocks.selection = waitingForConnection(physicalMicrophone, physicalMicrophone);
    mocks.selectMicrophone.mockImplementationOnce(async () => {
      mocks.selection = waitingForConnection(cableMicrophone, physicalMicrophone);
      return { ...mocks.selection };
    });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const feedback = observeSelectionAnimation(wrapper);
    await microphoneButton(wrapper, cableMicrophone.name).trigger("click");
    await flushPromises();
    expect(feedback.animate).toHaveBeenCalledOnce();
    expect(wrapper.get(".selected-microphone-row button").text()).toBe("已选择");
    expect(wrapper.get(".endpoint-list .microphone-active button").text()).toBe("使用中");
    feedback.animations[0].finish();
    await flushPromises();
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "microphone_selection_feedback", result: "passed", reason: "settled" }));
    wrapper.unmount();
    expect(feedback.animations[0].cancel).not.toHaveBeenCalled();
  });

  it("does not animate rejected, unconfirmed or late-after-unmount choices", async () => {
    prepareCable();
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const feedback = observeSelectionAnimation(wrapper);
    mocks.selectMicrophone.mockRejectedValueOnce(new Error("microphone_settings_save_failed"));
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    mocks.selectMicrophone.mockResolvedValueOnce({ ...selected(cableMicrophone), phase: "failed" });
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    await flushPromises();
    let resolveSelection!: (snapshot: MicrophoneSelectionSnapshot) => void;
    mocks.selectMicrophone.mockImplementationOnce(() => new Promise(resolve => { resolveSelection = resolve; }));
    await microphoneButton(wrapper, physicalMicrophone.name).trigger("click");
    wrapper.unmount();
    resolveSelection(selected(physicalMicrophone));
    await flushPromises();
    expect(feedback.animate).not.toHaveBeenCalled();
  });

  it("keeps the active other microphone selectable as a target with reduced motion", async () => {
    prepareCable();
    mocks.selection = waitingForConnection(cableMicrophone, physicalMicrophone);
    vi.stubGlobal("matchMedia", () => ({ matches: true }));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    const feedback = observeSelectionAnimation(wrapper);
    await wrapper.get(".endpoint-list .microphone-active button").trigger("click");
    await flushPromises();
    expect(mocks.selectMicrophone).toHaveBeenCalledExactlyOnceWith(physicalMicrophone.id);
    expect(wrapper.get(".selected-microphone-row button").text()).toBe("使用中");
    expect(feedback.animate).not.toHaveBeenCalled();
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "microphone_selection_feedback", result: "passed", reason: "reduced_motion" }));
    wrapper.unmount();
  });

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
    expect(card.findAll(".microphone-active")).toHaveLength(1);
    expect(card.get(".selected-microphone-row button").text()).toBe("使用中");
    expect(card.get(".selected-microphone-row button").attributes("disabled")).toBeDefined();
    expect(card.text()).not.toContain("当前生效");
    expect(card.text()).toContain("目标麦克风");
    expect(card.text()).toContain("连接遥控器后自动切换到所选麦克风，断开后恢复原设置");
    expect(card.text()).toContain("退出程序时也会尝试恢复");
    expect(card.text()).toContain("期间手动更改过的系统麦克风不会被覆盖");
    expect(card.text()).not.toContain("启动时切换");
    expect(card.text()).not.toContain("启动前的默认麦克风");
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

  it("shows a saved offline target without reporting an external change or stale route failure", async () => {
    prepareCable();
    mocks.selection = waitingForConnection(cableMicrophone, physicalMicrophone);
    mocks.audio.phase = "failed";
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("等待遥控器连接");
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-current").text()).toContain(physicalMicrophone.name);
    expect(wrapper.get(".microphone-waiting").text()).toContain("目标已保存");
    expect(wrapper.get(".selected-microphone-row").classes()).not.toContain("microphone-active");
    expect(wrapper.get(".endpoint-list .microphone-active strong").text()).toBe(physicalMicrophone.name);
    expect(wrapper.get(".microphone-waiting").text()).toContain("当前系统麦克风保持不变");
    expect(wrapper.get(".selected-microphone-row button").text()).toBe("已选择");
    expect(wrapper.get(".endpoint-list .microphone-active button").text()).toBe("使用中");
    expect(wrapper.get(".endpoint-list .microphone-active button").attributes("disabled")).toBeUndefined();
    expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
    expect(microphoneButton(wrapper, physicalMicrophone.name).attributes("disabled")).toBeUndefined();
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("accepts an offline target save without claiming the system microphone switched", async () => {
    prepareCable();
    mocks.selection = waitingForConnection(physicalMicrophone, physicalMicrophone);
    mocks.audio = { ...emptyAudio };
    mocks.selectMicrophone.mockImplementationOnce(async () => {
      mocks.selection = waitingForConnection(cableMicrophone, physicalMicrophone);
      return { ...mocks.selection };
    });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, cableMicrophone.name).trigger("click");
    await flushPromises();
    expect(mocks.selectMicrophone).toHaveBeenCalledExactlyOnceWith(cableMicrophone.id);
    expect(wrapper.get(".microphone-status").text()).toBe("等待遥控器连接");
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(wrapper.get(".microphone-current").text()).toContain(physicalMicrophone.name);
    expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
    expect(wrapper.get(".audio-settings-card").text()).not.toContain("已就绪");
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "microphone_selection", result: "passed", reason: "target_saved_waiting_connection" }));
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("refreshes waiting, connected and restored states without issuing another selection", async () => {
    vi.useFakeTimers();
    prepareCable();
    mocks.selection = waitingForConnection(cableMicrophone, physicalMicrophone);
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("等待遥控器连接");
    expect(wrapper.get(".endpoint-list .microphone-active strong").text()).toBe(physicalMicrophone.name);
    mocks.getConnectionSnapshot.mockResolvedValue({ ...emptyConnection, phase: "ready" });
    mocks.selection = selected(cableMicrophone);
    await vi.advanceTimersByTimeAsync(1000);
    expect(wrapper.get(".microphone-status").text()).toBe("已就绪");
    expect(wrapper.find(".microphone-waiting").exists()).toBe(false);
    expect(wrapper.get(".selected-microphone-row button").text()).toBe("使用中");
    expect(wrapper.find(".endpoint-list .microphone-active").exists()).toBe(false);
    expect(wrapper.find(".microphone-current").exists()).toBe(false);
    expect(wrapper.get(".audio-route-details").text()).toContain("已记住本次连接前的默认麦克风，断开连接或退出时恢复");
    mocks.getConnectionSnapshot.mockResolvedValue({ ...emptyConnection, phase: "disconnected" });
    mocks.selection = waitingForConnection(cableMicrophone, physicalMicrophone);
    await vi.advanceTimersByTimeAsync(1000);
    expect(wrapper.get(".microphone-status").text()).toBe("等待遥控器连接");
    expect(wrapper.get(".microphone-current").text()).toContain(physicalMicrophone.name);
    expect(wrapper.get(".audio-route-details").text()).not.toContain("已记住本次连接前");
    expect(wrapper.get(".endpoint-list .microphone-active strong").text()).toBe(physicalMicrophone.name);
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("keeps an offline save failure visible without claiming the microphone changed", async () => {
    prepareCable();
    mocks.selection = waitingForConnection(physicalMicrophone, physicalMicrophone);
    mocks.selectMicrophone.mockRejectedValueOnce(new Error("microphone_settings_save_failed"));
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, cableMicrophone.name).trigger("click");
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("设置未保存");
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toBe("麦克风设置未保存，可重试。");
    expect(wrapper.get(".microphone-name").text()).toBe(physicalMicrophone.name);
    expect(wrapper.find(".microphone-waiting").exists()).toBe(false);
    expect(wrapper.get(".microphone-retry button").text()).toBe("重试保存设置");
    expect(wrapper.get(".audio-settings-card").text()).not.toContain("麦克风已切换");
    wrapper.unmount();
  });

  it("does not confirm an offline save when the returned target differs from the requested one", async () => {
    prepareCable();
    mocks.selection = waitingForConnection(physicalMicrophone, physicalMicrophone);
    mocks.selectMicrophone.mockResolvedValueOnce({ ...mocks.selection });
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    await microphoneButton(wrapper, cableMicrophone.name).trigger("click");
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("暂不可用");
    expect(wrapper.find(".microphone-waiting").exists()).toBe(false);
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "microphone_selection", result: "failed", reason: "selection_unconfirmed" }));
    wrapper.unmount();
  });

  it("does not report a missing saved capture as ready while waiting for connection", async () => {
    prepareCable();
    mocks.selection = waitingForConnection(cableMicrophone, physicalMicrophone);
    mocks.microphones = [physicalMicrophone];
    const wrapper = mount(ConnectionPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.get(".microphone-status").text()).toBe("设备暂不可用");
    expect(wrapper.get(".microphone-name").text()).toBe(cableMicrophone.name);
    expect(mocks.selectMicrophone).not.toHaveBeenCalled();
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
    expect(wrapper.get(".endpoint-list .microphone-active strong").text()).toBe(physicalMicrophone.name);
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
    expect(wrapper.get(".selected-microphone-row button").text()).toBe("重新应用");
    expect(wrapper.get(".selected-microphone-row button").attributes("disabled")).toBeUndefined();
    expect(wrapper.get(".selected-microphone-row").classes()).toContain("microphone-active");
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
    expect(wrapper.find(".microphone-active").exists()).toBe(false);
    expect(wrapper.find(".microphone-current").exists()).toBe(false);
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
    expect(wrapper.find(".microphone-active").exists()).toBe(false);
    await vi.advanceTimersByTimeAsync(1000);
    expect(wrapper.get(".selected-microphone-row button").text()).toBe("使用中");
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
    expect(wrapper.get('.audio-settings-card [role="alert"]').text()).toBe("麦克风设置未保存，可重试。");
    await vi.advanceTimersByTimeAsync(10000);
    expect(wrapper.get(".microphone-status").text()).toBe("设置未保存");
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

  it("does not infer a save failure merely because a failed selection currently matches the requested default", async () => {
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
  it("previews offline capture preferences without pretending to connect or switch the system microphone", async () => {
    const bridge = await vi.importActual<typeof import("../lib/bridge")>("../lib/bridge");
    expect(bridge.isTauriRuntime()).toBe(false);
    const [captures, renders] = await Promise.all([bridge.listMicrophones(), bridge.listAudioEndpoints()]);
    const virtual = captures.find((microphone) => microphone.isVirtualCableCandidate)!;
    const physical = captures.find((microphone) => !microphone.isVirtualCableCandidate)!;
    expect(renders.map((endpoint) => endpoint.id)).toContain(virtual.renderEndpointIds[0]);
    const initialRoute = { ...await bridge.getAudioSnapshot() };
    const selectedVirtual = await bridge.selectMicrophone(virtual.id);
    expect(selectedVirtual.targetMicrophoneId).toBe(virtual.id);
    expect(selectedVirtual.currentMicrophoneId).toBe(physical.id);
    expect(selectedVirtual.phase).toBe("waiting_connection");
    expect(selectedVirtual.restorePending).toBe(false);
    expect(await bridge.getAudioSnapshot()).toEqual(initialRoute);
    expect((await bridge.selectMicrophone(physical.id)).currentMicrophoneId).toBe(physical.id);
    expect((await bridge.getMicrophoneSelection()).phase).toBe("waiting_connection");
    expect(await bridge.getAudioSnapshot()).toEqual(initialRoute);
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
      expect(wrapper.get(".microphone-status").text()).toBe("等待遥控器连接");
      expect(wrapper.findAll(".endpoint-list li")).toHaveLength(captures.length - 1);
      expect(wrapper.find('.audio-settings-card [role="alert"]').exists()).toBe(false);
      expect(mocks.selectMicrophone).not.toHaveBeenCalled();
    } finally {
      wrapper.unmount();
    }
  });
});
