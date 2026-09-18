<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import BatteryIndicator from "../components/BatteryIndicator.vue";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";
import type {
  AudioEndpoint,
  AudioSnapshot,
  ConnectionSnapshot,
  KeyChord,
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
  getVoiceHoldHotkey,
  listAudioEndpoints,
  openVbCableDownloadPage,
  remoteModelLabel,
  scanPairedRemotes,
  selectAudioEndpoint,
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
const audioEndpoints = ref<AudioEndpoint[]>([]);
const showEndpointList = ref(false);
const scanningAudio = ref(false);
const audioScanComplete = ref(false);
const selectingEndpointId = ref("");
const openingVbCablePage = ref(false);
const audioMessage = ref("尚未读取语音设备");
const voiceHotkey = ref<KeyChord | null>(null);
const loadingVoiceHotkey = ref(true);
const voiceHotkeyLoaded = ref(false);
const savingVoiceHotkey = ref(false);
const voiceHotkeyMessage = ref("正在读取快捷键设置…");
const voiceHotkeyError = ref(false);
let pollTimer: ReturnType<typeof setInterval> | undefined;

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
  audioEndpoints.value.filter((endpoint) => endpoint.isVirtualCableCandidate),
);

const virtualCableInstalled = computed(() => virtualCableEndpoints.value.length > 0);

const phaseTone = computed(() => {
  if (connection.value.phase === "failed") return "error";
  if (connection.value.phase === "streaming") return "active";
  if (connection.value.phase === "ready") return "success";
  if (connectionActive.value) return "warning";
  return "pending";
});

const phaseDetail = computed(() => {
  if (connection.value.lastError) return connection.value.lastError;
  if (connection.value.capabilities) return "语音功能已确认，可以按住遥控器语音键说话";
  return "连接后即可使用遥控器语音键";
});

const audioTone = computed(() => {
  if (audio.value.phase === "failed") return "error";
  if (audio.value.phase === "streaming") return "active";
  if (audio.value.phase === "ready") return "success";
  if (audio.value.phase === "draining") return "warning";
  return "pending";
});

const audioDetail = computed(() => {
  if (audio.value.lastError) return audio.value.lastError;
  if (audio.value.selectedEndpointName) return "语音会写入选中的设备";
  return "不会自动改动系统默认设备，需要在这里明确选择";
});

async function refreshConnection() {
  try {
    connection.value = await getConnectionSnapshot();
  } catch (error) {
    operationMessage.value = error instanceof Error ? error.message : String(error);
  }
}

async function refreshAudio() {
  try {
    audio.value = await getAudioSnapshot();
    return true;
  } catch (error) {
    audioMessage.value = error instanceof Error ? error.message : String(error);
    return false;
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

async function detectAudioEndpoints(autoSelectVirtualCable: boolean) {
  scanningAudio.value = true;
  audioMessage.value = "正在读取语音设备…";
  try {
    audioEndpoints.value = await listAudioEndpoints();
    audioScanComplete.value = true;
    const virtualCables = audioEndpoints.value.filter(
      (endpoint) => endpoint.isVirtualCableCandidate,
    );
    if (virtualCables.length === 1 && autoSelectVirtualCable && !audio.value.selectedEndpointId) {
      await chooseAudioEndpoint(virtualCables[0], true);
      return;
    }
    audioMessage.value = virtualCables.length
      ? `已检测到 ${virtualCables.length} 个 VB-CABLE 语音设备`
      : "未检测到 VB-CABLE；安装完成后需要重启电脑，再重新检测";
  } catch (error) {
    audioEndpoints.value = [];
    audioScanComplete.value = true;
    audioMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    scanningAudio.value = false;
  }
}

async function scanAudio() {
  await detectAudioEndpoints(false);
  // 用户主动读取端点 = 想看列表；选好即收起（每次只用一个端点）。
  showEndpointList.value = audioEndpoints.value.length > 0;
}

async function chooseAudioEndpoint(endpoint: AudioEndpoint, automatic = false) {
  selectingEndpointId.value = endpoint.id;
  audioMessage.value = "正在打开语音设备…";
  try {
    audio.value = await selectAudioEndpoint(endpoint.id);
    audioMessage.value = automatic
      ? `已自动选择 ${endpoint.name}`
      : `已选择 ${endpoint.name}`;
    showEndpointList.value = false;
  } catch (error) {
    audioMessage.value = error instanceof Error ? error.message : String(error);
    await refreshAudio();
  } finally {
    selectingEndpointId.value = "";
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
  const restoredAudio = await refreshAudio();
  await detectAudioEndpoints(restoredAudio);
}

onMounted(() => {
  void refreshConnection();
  void initializeAudio();
  void refreshVoiceHotkey();
  pollTimer = setInterval(() => {
    void refreshConnection();
    void refreshAudio();
  }, 1_000);
});

onUnmounted(() => {
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
            <h2>遥控器连接</h2>
            <p class="muted">连接已配对的小米遥控器。</p>
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
        <p class="muted voice-hotkey-row">按住遥控器语音键时按下所选快捷键，松开时释放。遥控器麦克风的声音送入右侧设备，由 Codex 听写或语音输入法转成文字。</p>
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
            <li>在右侧选择 CABLE Input，把遥控器麦克风的声音送入 VB-CABLE；</li>
            <li>让 Codex 听写使用 CABLE Output 麦克风；若 Codex 没有麦克风选择入口，在 Windows 中将它使用的录音设备设为 CABLE Output；</li>
            <li>将 Codex 切到前台，点击输入框。先用键盘按住 Ctrl + Shift + D，确认 Codex 的听写可用；</li>
            <li>按住遥控器语音键说话，松开结束。检查识别文字后再发送；本程序不会自动发送。</li>
          </ol>
          <p>此模式不需要微信输入法。使用遥控器麦克风仍需要 VB-CABLE。</p>
        </details>
        <details v-else-if="wetypeActive" class="usage-hint-details">
          <summary>微信输入法使用步骤（点开查看）</summary>
          <ol>
            <li>语音设备选择 CABLE Input；</li>
            <li>在微信输入法的语音设置里，把麦克风设为 CABLE Output；若没有这个选项，把系统默认录音设备设为 CABLE Output；</li>
            <li>在目标应用的文本框内切换到微信输入法（看任务栏输入指示器确认）；</li>
            <li>按住遥控器语音键约半秒以上再说话，松开后等待文字出现（需要联网）。快速点按不出文字是微信输入法自己的最短按住要求，不是故障。遥控器语音键自带的 F5 按键会被应用自动屏蔽，物理键盘的 F5 不受影响。</li>
          </ol>
        </details>
      </article>

      <article class="card">
        <div class="card-title-row">
          <div>
            <h2>语音设备</h2>
            <p class="muted">遥控器麦克风写入 CABLE Input；Codex 或语音输入法从 CABLE Output 录音。</p>
          </div>
          <button
            class="secondary-button"
            type="button"
            :disabled="scanningAudio || audioBusy || !runtime?.platform.windowsApiAvailable"
            @click="scanAudio()"
          >
            {{ scanningAudio ? "读取中…" : "刷新设备列表" }}
          </button>
        </div>

        <div class="status-panel" aria-live="polite">
          <div class="status-copy">
            <div class="status-heading">
              <span class="status-dot" :class="audioTone"></span>
              <strong>{{ audio.selectedEndpointName ?? audioPhaseLabel(audio.phase) }}</strong>
            </div>
            <small>{{ audioDetail }}</small>
          </div>
        </div>

        <p class="muted scan-summary">{{ audioMessage }}</p>
        <div v-if="audioEndpoints.length" class="endpoint-select-row">
          <button
            class="secondary-button"
            type="button"
            @click="showEndpointList = !showEndpointList"
          >
            {{ showEndpointList ? "收起列表" : audio.selectedEndpointId ? "更换设备" : "选择设备" }}
          </button>
          <span v-if="!showEndpointList" class="muted endpoint-count">
            共 {{ audioEndpoints.length }} 个设备可选
          </span>
        </div>
        <ul v-if="showEndpointList && audioEndpoints.length" class="device-list endpoint-list">
          <li v-for="endpoint in audioEndpoints" :key="endpoint.id">
            <div>
              <strong>{{ endpoint.name }}</strong>
              <small>{{ endpoint.isVirtualCableCandidate ? "推荐（Codex 听写、语音输入法使用）" : "其他音频设备" }}</small>
            </div>
            <button
              type="button"
              :disabled="audioBusy || Boolean(selectingEndpointId) || audio.selectedEndpointId === endpoint.id"
              @click="chooseAudioEndpoint(endpoint)"
            >
              {{
                selectingEndpointId === endpoint.id
                  ? "正在启用…"
                  : audio.selectedEndpointId === endpoint.id
                    ? "当前设备"
                    : "选择"
              }}
            </button>
          </li>
        </ul>

        <div class="setting-list compact two-col">
          <div class="setting-row">
            <strong>语音设备</strong>
            <span>{{ wasapiReady ? audioPhaseLabel(audio.phase) : "待选择" }}</span>
          </div>
        </div>

        <div v-if="audioScanComplete && !virtualCableInstalled" class="info-callout warning vb-cable-callout">
          <div>
            <strong>需要安装 VB-CABLE</strong>
            <p>由 VB-Audio 提供的免费虚拟声卡。安装需要管理员权限，完成后需重启电脑。</p>
          </div>
          <div class="button-row">
            <button class="primary-button" type="button" :disabled="openingVbCablePage" @click="openVbCablePage">
              {{ openingVbCablePage ? "正在打开…" : "打开官方下载页" }}
            </button>
            <button class="secondary-button" type="button" :disabled="scanningAudio" @click="scanAudio()">
              重新检测
            </button>
          </div>
        </div>
        <div v-else class="info-callout" :class="{ warning: !wasapiReady }">
          {{
            wasapiReady
              ? "语音设备已就绪。"
              : virtualCableInstalled
                ? "已检测到 VB-CABLE。这里选择 CABLE Input；让 Codex 或语音输入法使用 CABLE Output 麦克风。"
                : "正在检测 VB-CABLE…"
          }}
        </div>
      </article>
    </div>
  </section>
</template>
