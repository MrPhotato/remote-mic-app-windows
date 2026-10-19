<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import BatteryIndicator from "../components/BatteryIndicator.vue";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";
import type {
  AudioSnapshot,
  ConnectionSnapshot,
  KeyChord,
  MicrophoneEndpoint,
  MicrophoneSelectionSnapshot,
  PairedRemote,
  RuntimeSnapshot,
} from "../lib/bridge";
import {
  audioPhaseLabel,
  connectRemote,
  connectionPhaseLabel,
  disconnectRemote,
  getAudioSnapshot,
  getConnectionSnapshot,
  getMicrophoneSelection,
  getVoiceHoldHotkey,
  listMicrophones,
  openVbCableDownloadPage,
  remoteModelLabel,
  scanPairedRemotes,
  selectMicrophone,
  setVoiceHoldHotkey,
  voiceHoldHotkeyLabel,
} from "../lib/bridge";

const props = defineProps<{ runtime: RuntimeSnapshot | null }>();

const emptyConnection = (): ConnectionSnapshot => ({
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
});

const emptyAudio = (): AudioSnapshot => ({
  phase: "unsupported",
  selectedEndpointId: null,
  selectedEndpointName: null,
  queuedSamples: 0,
  submittedSamples: 0,
  generation: 0,
  lastError: null,
});

const connection = ref<ConnectionSnapshot>(emptyConnection());
const audio = ref<AudioSnapshot>(emptyAudio());
const scanning = ref(false);
const connectingDeviceId = ref("");
const disconnecting = ref(false);
const devices = ref<PairedRemote[]>([]);
const scanMessage = ref("尚未扫描");
const operationMessage = ref("");
const microphones = ref<MicrophoneEndpoint[]>([]);
const scanningAudio = ref(false);
const audioScanComplete = ref(false);
const audioInventoryStale = ref(false);
const selectingMicrophoneId = ref("");
const microphoneSelection = ref<MicrophoneSelectionSnapshot>({
  targetMicrophoneId: null, targetMicrophoneName: null,
  currentMicrophoneId: null, currentMicrophoneName: null,
  phase: "unconfigured", lastError: null, restorePending: false,
});
const microphoneSelectionLoaded = ref(false);
const microphoneSelectionReadError = ref("");
const audioReadError = ref("");
const pendingSelectionId = ref("");
let selectionReadInFlight = false;
let selectionRevision = 0;
const openingVbCablePage = ref(false);
const audioMessage = ref("尚未读取语音设备");
const audioOperationError = ref("");
const audioSettingsUnsaved = ref(false);
const voiceHotkey = ref<KeyChord | null>(null);
const loadingVoiceHotkey = ref(true);
const voiceHotkeyLoaded = ref(false);
const savingVoiceHotkey = ref(false);
const voiceHotkeyMessage = ref("正在读取快捷键设置…");
const voiceHotkeyError = ref(false);
let pollTimer: ReturnType<typeof setInterval> | undefined;
let pendingInventoryRefresh = false;
let audioInventoryAttempted = false;
let inventoryPollTicks = 0;
let disposed = false;

const voiceHotkeyPresets: Array<{ label: string; keys: string[] }> = [
  { label: "微信输入法（默认）", keys: ["left_control", "left_windows"] },
  { label: "Codex 听写 · Ctrl + Shift + D", keys: ["left_control", "left_shift", "d"] },
  { label: "关闭", keys: [] },
];

const activeVoiceHotkeyKeys = computed(() =>
  voiceHotkey.value ? [...voiceHotkey.value.keys].sort().join("+") : "",
);

function presetIsActive(keys: string[]): boolean {
  return voiceHotkeyLoaded.value && [...keys].sort().join("+") === activeVoiceHotkeyKeys.value;
}

const codexDictationActive = computed(() => presetIsActive(["left_control", "left_shift", "d"]));
const wetypeActive = computed(() => presetIsActive(["left_control", "left_windows"]));
const voiceHotkeyDisplay = computed(() => loadingVoiceHotkey.value ? "正在读取…"
  : !voiceHotkeyLoaded.value ? "读取失败"
    : codexDictationActive.value ? "Ctrl + Shift + D" : voiceHoldHotkeyLabel(voiceHotkey.value));

async function applyVoiceHotkey(keys: string[]) {
  if (savingVoiceHotkey.value || loadingVoiceHotkey.value || !voiceHotkeyLoaded.value) return;
  savingVoiceHotkey.value = true;
  voiceHotkeyError.value = false;
  voiceHotkeyMessage.value = "正在保存按住说话快捷键…";
  const started = performance.now();
  const preset = keys.includes("d") ? "codex" : keys.length ? "wetype" : "off";
  reportFrontendEvent({ event: "voice_hotkey_preset_save", phase: "started", result: "unknown", reason: `preset_${preset}` });
  try {
    voiceHotkey.value = await setVoiceHoldHotkey(
      keys.length ? { keys: [...keys] } : null,
    );
    voiceHotkeyMessage.value = voiceHotkey.value
      ? `按住说话快捷键已设为 ${voiceHoldHotkeyLabel(voiceHotkey.value)}`
      : "按住说话快捷键已关闭，语音键仅输出语音";
    reportFrontendEvent({ event: "voice_hotkey_preset_save", phase: "completed", result: "passed", reason: `preset_${preset}_saved`, elapsedMs: Math.round(performance.now() - started) });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    const refreshed = await refreshVoiceHotkey(false);
    voiceHotkeyError.value = true;
    voiceHotkeyMessage.value = `保存失败：${detail}。${refreshed ? "已重新读取当前设置。" : "当前设置读取失败，请重试。"}`;
    reportFrontendEvent({ event: "voice_hotkey_preset_save", phase: "completed", result: "failed", reason: "settings_save_failed", elapsedMs: Math.round(performance.now() - started) });
  } finally {
    savingVoiceHotkey.value = false;
  }
}

async function refreshVoiceHotkey(showMessage = true): Promise<boolean> {
  loadingVoiceHotkey.value = true;
  const started = performance.now();
  try {
    voiceHotkey.value = await getVoiceHoldHotkey();
    voiceHotkeyLoaded.value = true;
    if (showMessage) {
      voiceHotkeyError.value = false;
      voiceHotkeyMessage.value = voiceHotkey.value
        ? `当前按住说话快捷键：${voiceHoldHotkeyLabel(voiceHotkey.value)}`
        : "按住说话快捷键已关闭，语音键仅输出语音";
    }
    reportFrontendEvent({ event: "voice_hotkey_preset_load", phase: "completed", result: "passed", reason: "settings_loaded", elapsedMs: Math.round(performance.now() - started) });
    return true;
  } catch (error) {
    voiceHotkeyLoaded.value = false;
    if (showMessage) {
      voiceHotkeyError.value = true;
      voiceHotkeyMessage.value = error instanceof Error ? error.message : String(error);
    }
    reportFrontendEvent({ event: "voice_hotkey_preset_load", phase: "completed", result: "failed", reason: "settings_load_failed", elapsedMs: Math.round(performance.now() - started) });
    return false;
  } finally {
    loadingVoiceHotkey.value = false;
  }
}

watch(
  () => props.runtime?.platform.connection,
  (snapshot) => {
    if (snapshot) connection.value = snapshot;
  },
  { immediate: true },
);

watch(
  () => props.runtime?.platform.audio,
  (snapshot) => {
    if (snapshot) audio.value = snapshot;
  },
  { immediate: true },
);

const connectionActive = computed(() =>
  [
    "connecting",
    "discovering",
    "awaiting_capabilities",
    "ready",
    "streaming",
    "draining",
    "reconnecting",
    "suspended",
  ].includes(connection.value.phase),
);

const atvvReady = computed(() =>
  ["ready", "streaming", "draining"].includes(connection.value.phase),
);

const audioBusy = computed(() => ["streaming", "draining"].includes(audio.value.phase));

const wasapiReady = computed(() =>
  ["ready", "streaming", "draining"].includes(audio.value.phase),
);

const virtualCableEndpoints = computed(() =>
  microphones.value.filter((endpoint) => endpoint.isVirtualCableCandidate),
);

const selectedMicrophone = computed(() =>
  microphones.value.find((microphone) => microphone.id === microphoneSelection.value.targetMicrophoneId),
);
const selectedVirtualCable = computed(() => selectedMicrophone.value?.isVirtualCableCandidate === true);
const otherMicrophones = computed(() => microphones.value.filter((microphone) => microphone.id !== microphoneSelection.value.targetMicrophoneId));
const microphoneName = computed(() => microphoneSelection.value.targetMicrophoneName
  ?? selectedMicrophone.value?.name
  ?? (microphoneSelectionReadError.value ? "读取失败" : !microphoneSelectionLoaded.value ? "正在读取…" : "尚未选择"));
const defaultMatchesTarget = computed(() => Boolean(microphoneSelection.value.targetMicrophoneId)
  && microphoneSelection.value.currentMicrophoneId === microphoneSelection.value.targetMicrophoneId
  && microphoneSelection.value.phase === "ready");
const microphoneWaitingConnection = computed(() => microphoneSelection.value.phase === "waiting_connection");
const virtualRouteReady = computed(() => selectedVirtualCable.value && wasapiReady.value
  && selectedMicrophone.value!.renderEndpointIds.includes(audio.value.selectedEndpointId ?? "") && !audioReadError.value);
const selectionDisabled = computed(() => audioInventoryStale.value || !audioScanComplete.value
  || !microphoneSelectionLoaded.value || Boolean(microphoneSelectionReadError.value)
  || audioBusy.value || Boolean(selectingMicrophoneId.value));
const retryCandidate = computed(() => microphones.value.find((microphone) =>
  microphone.id === (pendingSelectionId.value || microphoneSelection.value.targetMicrophoneId)));

watch(
  [() => audio.value.selectedEndpointId, () => audio.value.phase === "failed"],
  () => {
    if (!audioInventoryAttempted || disposed) return;
    if (scanningAudio.value) pendingInventoryRefresh = true;
    else void detectAudioEndpoints();
  },
);

function microphoneDetail(microphone: MicrophoneEndpoint): string {
  if (audioInventoryStale.value) return "等待刷新";
  return microphone.isVirtualCableCandidate
    ? microphone.renderEndpointIds.length ? "遥控器语音" : "遥控器传声线路暂不可用"
    : "直接使用这个麦克风录音";
}

const phaseTone = computed(() => {
  if (connection.value.phase === "failed") return "error";
  if (connection.value.phase === "streaming") return "active";
  if (connection.value.phase === "ready") return "success";
  if (connectionActive.value) return "warning";
  return "pending";
});

const phaseDetail = computed(() => {
  if (connection.value.lastError) return connection.value.lastError;
  if (connection.value.capabilities) return "可以按住遥控器语音键说话";
  return "连接后即可使用遥控器语音键";
});

const audioStatus = computed(() => {
  if (!microphoneSelectionLoaded.value || microphoneSelectionReadError.value) return microphoneSelectionReadError.value ? "状态待确认" : "正在读取";
  if (audioInventoryStale.value) return "等待刷新";
  if (audioSettingsUnsaved.value) return "设置未保存";
  if (audioOperationError.value || microphoneSelection.value.phase === "failed") return "暂不可用";
  if (!microphoneSelection.value.targetMicrophoneId) return "待选择";
  if (microphoneWaitingConnection.value) return audioScanComplete.value && !selectedMicrophone.value ? "设备暂不可用" : "等待遥控器连接";
  if (!defaultMatchesTarget.value) return microphoneSelection.value.currentMicrophoneId ? "系统麦克风已更改" : "当前麦克风待确认";
  if (!audioScanComplete.value) return "正在读取";
  if (!selectedMicrophone.value) return "设备暂不可用";
  if (!selectedVirtualCable.value) return "已就绪";
  if (!virtualRouteReady.value) return audio.value.phase === "failed" ? "传声线路暂不可用" : "传声线路待确认";
  if (audio.value.phase === "streaming") return "传送中";
  if (audio.value.phase === "draining") return "收尾中";
  return "已就绪";
});

const audioTone = computed(() => {
  if (audioStatus.value === "已就绪") return "success";
  if (audioStatus.value === "传送中") return "active";
  if (audioOperationError.value || microphoneSelection.value.phase === "failed" || (!microphoneWaitingConnection.value && selectedVirtualCable.value && audio.value.phase === "failed")) return "error";
  return "warning";
});

const audioVisibleError = computed(() => {
  if (microphoneSelectionReadError.value) return "无法确认当前麦克风，正在重新读取。";
  if (audioInventoryStale.value) return "麦克风列表暂时无法更新，正在自动重试。";
  if (audioSettingsUnsaved.value) return "麦克风设置未保存，可重试。";
  if (audioOperationError.value) return "麦克风切换失败，可重试。";
  if (microphoneSelection.value.phase === "failed") return "麦克风暂不可用，可重试。";
  if (!microphoneWaitingConnection.value && selectedVirtualCable.value && audio.value.phase === "failed") return "遥控器语音线路暂不可用，可重试。";
  return "";
});

function reportAudioDetailsToggle(event: Event) {
  reportFrontendEvent({ event: "audio_advanced_settings", phase: "completed", result: "passed", reason: (event.target as HTMLDetailsElement).open ? "expanded" : "collapsed" });
}

async function refreshConnection() {
  try {
    connection.value = await getConnectionSnapshot();
  } catch (error) {
    operationMessage.value = error instanceof Error ? error.message : String(error);
  }
}

async function refreshAudio() {
  try {
    const snapshot = await getAudioSnapshot();
    if (disposed) return false;
    audio.value = snapshot;
    audioReadError.value = "";
    return true;
  } catch (error) {
    if (!disposed) audioReadError.value = error instanceof Error ? error.message : String(error);
    return false;
  }
}

async function refreshMicrophoneSelection(allowDuringSelection = false): Promise<boolean> {
  if (disposed || selectionReadInFlight || (selectingMicrophoneId.value && !allowDuringSelection)) return false;
  selectionReadInFlight = true;
  const revision = selectionRevision;
  try {
    const snapshot = await getMicrophoneSelection();
    if (disposed || revision !== selectionRevision) return false;
    const changed = !microphoneSelectionLoaded.value || Boolean(microphoneSelectionReadError.value)
      || snapshot.phase !== microphoneSelection.value.phase
      || snapshot.targetMicrophoneId !== microphoneSelection.value.targetMicrophoneId
      || snapshot.currentMicrophoneId !== microphoneSelection.value.currentMicrophoneId;
    microphoneSelection.value = snapshot;
    microphoneSelectionLoaded.value = true;
    microphoneSelectionReadError.value = "";
    if (changed) reportFrontendEvent({ event: "microphone_selection_load", phase: "completed", result: "passed", reason: `state_${snapshot.phase}` });
    return true;
  } catch (error) {
    if (disposed || revision !== selectionRevision) return false;
    if (!microphoneSelectionReadError.value) reportFrontendEvent({ event: "microphone_selection_load", phase: "completed", result: "failed", reason: "state_read_failed" });
    microphoneSelectionReadError.value = error instanceof Error ? error.message : String(error);
    return false;
  } finally {
    selectionReadInFlight = false;
  }
}

async function scan() {
  scanning.value = true;
  operationMessage.value = "";
  scanMessage.value = "正在寻找小米遥控器…";
  try {
    devices.value = await scanPairedRemotes();
    scanMessage.value = devices.value.length
      ? `找到 ${devices.value.length} 个已配对的小米遥控器`
      : "没有找到已配对的小米遥控器";
  } catch (error) {
    devices.value = [];
    scanMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    scanning.value = false;
  }
}

async function connect(device: PairedRemote) {
  connectingDeviceId.value = device.id;
  operationMessage.value = "";
  try {
    connection.value = await connectRemote(device.id);
    operationMessage.value = "已连接，正在确认语音功能";
  } catch (error) {
    operationMessage.value = error instanceof Error ? error.message : String(error);
    await refreshConnection();
  } finally {
    connectingDeviceId.value = "";
  }
}

async function disconnect() {
  disconnecting.value = true;
  operationMessage.value = "";
  try {
    connection.value = await disconnectRemote();
    operationMessage.value = "遥控器连接已释放，本次运行已停止自动重连";
  } catch (error) {
    operationMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    disconnecting.value = false;
  }
}

async function detectAudioEndpoints() {
  if (scanningAudio.value || disposed) return;
  audioInventoryAttempted = true;
  scanningAudio.value = true;
  audioMessage.value = "正在读取麦克风…";
  try {
    const captures = await listMicrophones();
    if (disposed) return;
    microphones.value = captures;
    audioInventoryStale.value = false;
    audioScanComplete.value = true;
    reportFrontendEvent({ event: "audio_route_detection", phase: "completed", result: "passed", reason: "capture_inventory_read" });
    audioMessage.value = `找到 ${captures.length} 个麦克风`;
  } catch (error) {
    if (disposed) return;
    audioInventoryStale.value = true;
    audioScanComplete.value = false;
    audioMessage.value = error instanceof Error ? error.message : String(error);
    reportFrontendEvent({ event: "audio_route_detection", phase: "completed", result: "failed", reason: "endpoint_enumeration_failed" });
  } finally {
    scanningAudio.value = false;
    if (pendingInventoryRefresh && !disposed) {
      pendingInventoryRefresh = false;
      void detectAudioEndpoints();
    }
  }
}

async function scanAudio() {
  await Promise.all([refreshMicrophoneSelection(), refreshAudio(), detectAudioEndpoints()]);
}

async function chooseMicrophone(microphone: MicrophoneEndpoint) {
  if (selectionDisabled.value || !microphones.value.some((item) => item.id === microphone.id)) return;
  selectingMicrophoneId.value = microphone.id;
  selectionRevision += 1;
  pendingSelectionId.value = microphone.id;
  audioOperationError.value = "";
  audioSettingsUnsaved.value = false;
  const started = performance.now();
  const reason = microphone.isVirtualCableCandidate ? "virtual_microphone" : "physical_microphone";
  try {
    const snapshot = await selectMicrophone(microphone.id);
    if (disposed) return;
    microphoneSelection.value = snapshot;
    microphoneSelectionLoaded.value = true;
    microphoneSelectionReadError.value = "";
    const savedWaiting = snapshot.phase === "waiting_connection" && snapshot.targetMicrophoneId === microphone.id;
    const confirmed = savedWaiting || (snapshot.phase === "ready" && snapshot.targetMicrophoneId === microphone.id && snapshot.currentMicrophoneId === microphone.id);
    if (confirmed) pendingSelectionId.value = "";
    else audioOperationError.value = snapshot.lastError ?? "microphone_selection_unconfirmed";
    await Promise.all([refreshAudio(), detectAudioEndpoints()]);
    reportFrontendEvent({ event: "microphone_selection", phase: "completed", result: confirmed ? "passed" : "failed", reason: savedWaiting ? "target_saved_waiting_connection" : confirmed ? reason : "selection_unconfirmed", elapsedMs: Math.round(performance.now() - started) });
  } catch (error) {
    if (disposed) return;
    const detail = error instanceof Error ? error.message : String(error);
    audioOperationError.value = detail;
    audioSettingsUnsaved.value = detail === "microphone_settings_save_failed";
    await Promise.all([refreshMicrophoneSelection(true), refreshAudio()]);
    reportFrontendEvent({ event: "microphone_selection", phase: "completed", result: "failed", reason: audioSettingsUnsaved.value ? "settings_save_failed" : "selection_failed", elapsedMs: Math.round(performance.now() - started) });
  } finally {
    selectingMicrophoneId.value = "";
  }
}

async function openVbCablePage() {
  openingVbCablePage.value = true;
  try {
    await openVbCableDownloadPage();
    audioMessage.value = "已打开 VB-CABLE 官方下载页面；安装时需要管理员权限，完成后请重启电脑";
  } catch (error) {
    audioMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    openingVbCablePage.value = false;
  }
}

async function initializeAudio() {
  await Promise.all([refreshAudio(), refreshMicrophoneSelection()]);
  await detectAudioEndpoints();
}

onMounted(() => {
  void refreshConnection();
  void initializeAudio();
  void refreshVoiceHotkey();
  pollTimer = setInterval(() => {
    void refreshConnection();
    void refreshAudio();
    void refreshMicrophoneSelection();
    inventoryPollTicks += 1;
    if (inventoryPollTicks >= 10 && !audioBusy.value && !scanningAudio.value && !selectingMicrophoneId.value) {
      inventoryPollTicks = 0;
      void detectAudioEndpoints();
    }
  }, 1_000);
});

onUnmounted(() => {
  disposed = true;
  pendingInventoryRefresh = false;
  if (pollTimer) clearInterval(pollTimer);
});
</script>

<template>
  <section>
    <header class="page-header">
      <div>
        <h1>连接与语音</h1>
      </div>
      <span class="badge" :class="phaseTone">{{ connectionPhaseLabel(connection.phase) }}</span>
    </header>

    <div class="two-column">
      <article class="card">
        <div class="card-title-row">
          <div>
            <h2>遥控器</h2>
            <p class="muted">从 Windows 已配对的设备中选择小米遥控器。</p>
          </div>
          <button
            class="primary-button"
            type="button"
            :disabled="scanning || connectionActive || !runtime?.platform.bleScanAvailable"
            @click="scan"
          >
            {{ scanning ? "扫描中…" : "扫描已配对设备" }}
          </button>
        </div>

        <div class="status-panel" aria-live="polite">
          <div class="status-copy">
            <div class="status-heading">
              <span class="status-dot" :class="phaseTone"></span>
              <strong>{{ connection.remoteName ?? connectionPhaseLabel(connection.phase) }}</strong>
            </div>
            <small>{{ phaseDetail }}</small>
          </div>
          <button
            v-if="connectionActive"
            class="secondary-button status-action"
            type="button"
            :disabled="disconnecting"
            @click="disconnect"
          >
            {{ disconnecting ? "断开中…" : "断开" }}
          </button>
        </div>

        <p class="muted scan-summary">{{ scanMessage }}</p>
        <p v-if="operationMessage" class="operation-message">{{ operationMessage }}</p>

        <ul v-if="devices.length" class="device-list">
          <li v-for="device in devices" :key="device.id">
            <div><strong>{{ device.name }}</strong><small>{{ remoteModelLabel(device.model) }}</small></div>
            <button
              type="button"
              :disabled="connectionActive || Boolean(connectingDeviceId)"
              @click="connect(device)"
            >
              {{ connectingDeviceId === device.id ? "连接中…" : "连接" }}
            </button>
          </li>
        </ul>

        <div class="setting-list compact two-col">
          <div class="setting-row">
            <strong>设备型号</strong>
            <span>{{ remoteModelLabel(connection.remoteModel) }}</span>
          </div>
          <div class="setting-row">
            <strong>电池电量</strong>
            <BatteryIndicator :connection="connection" />
          </div>
          <div class="setting-row">
            <strong>语音按键</strong>
            <span>{{ atvvReady ? "已就绪" : "正在确认" }}</span>
          </div>
          <div class="setting-row">
            <strong>睡眠唤醒自动重连</strong>
            <span>{{ connection.powerNotificationsAvailable ? "已启用" : "暂不可用" }}</span>
          </div>
          <div class="setting-row">
            <strong>按住说话快捷键</strong>
            <span>{{ voiceHotkeyDisplay }}</span>
          </div>
        </div>
        <p class="muted voice-hotkey-row">按住语音键时，无线麦会同时按住下面选择的快捷键，松开时一起松开——由 Codex 听写或语音输入法把遥控器收到的声音转成文字。</p>
        <div class="button-row voice-hotkey-presets">
          <button
            v-for="preset in voiceHotkeyPresets"
            :key="preset.label"
            :class="presetIsActive(preset.keys) ? 'primary-button' : 'secondary-button'"
            type="button"
            :aria-pressed="presetIsActive(preset.keys)"
            :disabled="savingVoiceHotkey || loadingVoiceHotkey || !voiceHotkeyLoaded || !runtime?.platform.windowsApiAvailable || presetIsActive(preset.keys)"
            @click="applyVoiceHotkey(preset.keys)"
          >
            {{ preset.label }}
          </button>
        </div>
        <p class="scan-summary" :class="voiceHotkeyError ? 'error-text' : 'muted'" :role="voiceHotkeyError ? 'alert' : 'status'">{{ voiceHotkeyMessage }}</p>
        <button v-if="!voiceHotkeyLoaded && !loadingVoiceHotkey" class="secondary-button" type="button" :disabled="savingVoiceHotkey" @click="refreshVoiceHotkey()">重新读取快捷键</button>
        <details v-if="codexDictationActive" class="usage-hint-details" open>
          <summary>Codex 听写使用步骤</summary>
          <ol>
            <li>确认右侧目标麦克风已就绪；使用遥控器录音时选择 VB-CABLE；</li>
            <li>让 Codex 听写使用系统默认麦克风；</li>
            <li>将 Codex 切到前台，点击输入框；</li>
            <li>按住遥控器语音键说话，松开结束。检查识别文字后再发送；本程序不会自动发送。</li>
          </ol>
          <p>此模式不需要微信输入法。使用遥控器麦克风仍需要 VB-CABLE。</p>
        </details>
        <details v-else-if="wetypeActive" class="usage-hint-details">
          <summary>微信输入法使用步骤（点开查看）</summary>
          <ol>
            <li>确认右侧目标麦克风已就绪；使用遥控器录音时选择 VB-CABLE；</li>
            <li>在微信输入法的语音设置里，把麦克风设为系统默认麦克风；</li>
            <li>在目标应用的文本框内切换到微信输入法（看任务栏输入指示器确认）；</li>
            <li>按住遥控器语音键约半秒后再说话，松开后等待文字出现（需要联网）。</li>
          </ol>
        </details>
      </article>

      <article class="card audio-settings-card">
        <div class="card-title-row">
          <h2>麦克风</h2>
          <button
            class="secondary-button"
            type="button"
            :disabled="scanningAudio || audioBusy || Boolean(selectingMicrophoneId)"
            @click="scanAudio()"
          >
            {{ scanningAudio ? "读取中…" : "刷新设备列表" }}
          </button>
        </div>

        <div class="microphone-target" aria-live="polite">
          <div class="status-heading">
            <span class="status-dot" :class="audioTone"></span>
            <strong>目标麦克风</strong>
            <span class="microphone-status muted">{{ audioStatus }}</span>
          </div>
          <div class="selected-microphone-row">
            <strong class="microphone-name">{{ microphoneName }}</strong>
            <button
              v-if="selectedMicrophone && !audioSettingsUnsaved && !audioVisibleError && !defaultMatchesTarget && !microphoneWaitingConnection"
              class="secondary-button"
              type="button"
              :disabled="selectionDisabled"
              @click="chooseMicrophone(selectedMicrophone)"
            >
              {{ selectingMicrophoneId ? "正在保存…" : "使用这个麦克风" }}
            </button>
          </div>
          <p class="muted microphone-target-hint">连接遥控器后自动切换到所选麦克风，断开后恢复原设置。目标应用选择“系统默认麦克风”即可跟随切换。</p>
          <p v-if="microphoneWaitingConnection && !audioVisibleError" class="muted microphone-waiting" role="status">目标已保存，等待遥控器连接；当前系统麦克风保持不变。</p>
          <p v-if="microphoneSelectionLoaded && !microphoneSelectionReadError && microphoneSelection.currentMicrophoneId && microphoneSelection.currentMicrophoneId !== microphoneSelection.targetMicrophoneId" class="muted microphone-current">当前系统麦克风：{{ microphoneSelection.currentMicrophoneName ?? '名称暂不可用' }}</p>
        </div>

        <p v-if="audioVisibleError" class="error-text microphone-error" role="alert">{{ audioVisibleError }}</p>


        <section class="microphone-options" aria-labelledby="other-microphones-heading">
          <h3 id="other-microphones-heading">其余麦克风选择</h3>
          <ul v-if="otherMicrophones.length" id="audio-endpoint-list" class="device-list endpoint-list">
            <li v-for="microphone in otherMicrophones" :key="microphone.id">
              <div>
                <strong>{{ microphone.name }}</strong>
                <small>{{ microphoneDetail(microphone) }}</small>
              </div>
              <button
                type="button"
                :disabled="selectionDisabled"
                @click="chooseMicrophone(microphone)"
              >
                {{ selectingMicrophoneId === microphone.id ? "正在保存…" : "选择" }}
              </button>
            </li>
          </ul>
          <p v-else class="muted microphone-empty">{{ scanningAudio || !audioScanComplete ? "正在读取…" : "没有其他麦克风" }}</p>
        </section>

        <div v-if="retryCandidate && (audioVisibleError || audioSettingsUnsaved)" class="endpoint-select-row microphone-retry">
          <button
            class="primary-button"
            type="button"
            :disabled="selectionDisabled"
            @click="chooseMicrophone(retryCandidate)"
          >
            {{ selectingMicrophoneId ? "处理中…" : audioSettingsUnsaved ? "重试保存设置" : "重试" }}
          </button>
        </div>

        <details class="usage-hint-details audio-route-details" @toggle="reportAudioDetailsToggle">
          <summary>诊断详情</summary>
          <p>传送状态：{{ audioPhaseLabel(audio.phase) }}</p>
          <p v-if="audio.selectedEndpointName">内部传声线路：{{ audio.selectedEndpointName }}</p>
          <p class="muted" role="status">{{ audioMessage }}</p>
          <p v-if="audioOperationError" class="error-text">{{ audioOperationError }}</p>
          <p v-if="microphoneSelectionReadError" class="error-text">{{ microphoneSelectionReadError }}</p>
          <p v-if="microphoneSelection.lastError" class="error-text">{{ microphoneSelection.lastError }}</p>
          <p v-if="audioReadError" class="error-text">{{ audioReadError }}</p>
          <p v-if="audio.lastError" class="error-text">{{ audio.lastError }}</p>
          <ul v-if="microphones.some(microphone => microphone.isDefault || microphone.isDefaultCommunications)">
            <li v-for="microphone in microphones.filter(item => item.isDefault || item.isDefaultCommunications)" :key="microphone.id">
              {{ microphone.name }}：{{ [microphone.isDefault ? 'Windows 默认录音设备' : '', microphone.isDefaultCommunications ? '默认通信设备' : ''].filter(Boolean).join('、') }}
            </li>
          </ul>
          <p class="muted">普通麦克风直接录音；VB-CABLE 接收遥控器语音。关闭窗口仅缩到托盘；遥控器断开时恢复连接前的默认麦克风，退出程序时也会尝试恢复。期间手动更改过的系统麦克风不会被覆盖。固定选择了某个设备的应用不会跟随系统默认设置。</p>
          <p v-if="microphoneSelection.restorePending" class="muted">已记住本次连接前的默认麦克风，断开连接或退出时恢复。</p>
          <button v-if="audioScanComplete && !virtualCableEndpoints.length" class="secondary-button" type="button" :disabled="openingVbCablePage" @click="openVbCablePage">
            {{ openingVbCablePage ? "正在打开…" : "VB-CABLE 官方下载页" }}
          </button>
        </details>
      </article>
    </div>
  </section>
</template>

<style scoped>
.audio-settings-card {
  min-width: 0;
  align-self: start;
}

.microphone-target {
  display: grid;
  gap: 10px;
  margin-top: 16px;
}

.microphone-status {
  margin-left: auto;
  font-size: 0.85rem;
}

.selected-microphone-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 16px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: var(--surface-control);
}

.microphone-name,
.endpoint-list strong {
  overflow-wrap: anywhere;
}

.selected-microphone-row button,
.endpoint-list button {
  flex-shrink: 0;
}

.microphone-target-hint,
.microphone-current,
.microphone-error,
.microphone-empty {
  margin: 0;
}

.microphone-error,
.microphone-options,
.audio-route-details {
  margin-top: 16px;
}

.microphone-options h3 {
  margin: 0 0 10px;
  font-size: 1rem;
}

.endpoint-list {
  margin: 0;
}

.endpoint-list li > div {
  min-width: 0;
}

.audio-route-details ul {
  padding-left: 20px;
  overflow-wrap: anywhere;
}
</style>
