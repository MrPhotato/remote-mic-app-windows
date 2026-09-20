<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import Rc003InputControl from "../components/Rc003InputControl.vue";
import {
  actionSummary, audioPhaseLabel, buttonLabel, connectionPhaseLabel,
  getButtonMappings, isTauriRuntime, remoteModelLabel, saveButtonMappings,
  type ButtonMappings, type ButtonTrigger, type RemoteButton, type RuntimeSnapshot,
} from "../lib/bridge";
import {
  applyCodingProfile, buildCodingProfile, CODING_PROFILE_BUTTONS,
  readCodingBackup, restoreCodingProfile, type CodingBackup, type CodingBackupSource,
} from "../lib/coding-profile";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";
import type { PageId } from "../navigation";

const props = defineProps<{ runtime: RuntimeSnapshot | null }>();
const emit = defineEmits<{ navigate: [page: PageId] }>();
const nativeRuntime = isTauriRuntime();
const current = ref<ButtonMappings | null>(null);
const backup = ref<CodingBackup | null>(null);
const originalBackup = ref<CodingBackup | null>(null);
const loading = ref(true);
const busy = ref(false);
const error = ref("");
const message = ref("");
const restoreReview = ref<CodingBackupSource | null>(null);
const backupError = ref(false);

const connection = computed(() => props.runtime?.platform.connection);
const audio = computed(() => props.runtime?.platform.audio);
const connected = computed(() => ["ready", "streaming", "draining"].includes(connection.value?.phase ?? ""));
const audioConfigured = computed(() => Boolean(audio.value?.selectedEndpointId)
  && ["ready", "streaming", "draining"].includes(audio.value?.phase ?? ""));
const next = computed(() => current.value ? buildCodingProfile(current.value) : null);
const backupDate = computed(() => backup.value ? new Date(backup.value.createdAt).toLocaleString() : "");
const canApply = computed(() => nativeRuntime && current.value !== null && !loading.value && !busy.value && !backupError.value);
const presetLabels: Partial<Record<RemoteButton, Partial<Record<ButtonTrigger, string>>>> = {
  home: { single: "打开 Codex", double: "新任务 · Ctrl + N", long: "设置 · Ctrl + ," },
  menu: { single: "命令菜单 · Ctrl + Shift + P", double: "选择模型 · Ctrl + Shift + M", long: "待处理项 · Ctrl + Alt + A" },
  tv: { single: "查看改动 · Ctrl + Alt + B", double: "开关侧边栏 · Ctrl + B", long: "撤销 · Ctrl + Z" },
  power: { single: "取消当前操作 · Esc" },
  ok: { single: "确认/发送 · Enter" },
  back: { single: "退格 · Backspace", double: "不额外绑定，连按继续退格", long: "按住连续退格" },
  up: { long: "按住连续向上" }, down: { long: "按住连续向下" },
  left: { long: "按住连续向左" }, right: { long: "按住连续向右" },
  volume_up: { single: "上一任务/标签页 · Ctrl + PageUp", double: "未配置动作", long: "按住连续切换上一项" },
  volume_down: { single: "下一任务/标签页 · Ctrl + PageDown", double: "未配置动作", long: "按住连续切换下一项" },
};

function presetLabel(button: RemoteButton, gesture: ButtonTrigger): string {
  return presetLabels[button]?.[gesture] ?? actionSummary(next.value?.actions[button]?.[gesture]);
}

function report(event: string, result: "passed" | "failed" | "unknown", reason: string, started = performance.now()): void {
  reportFrontendEvent({ event, phase: result === "unknown" ? "started" : "completed", result,
    reason, elapsedMs: Math.round(performance.now() - started) });
}

// No names, endpoint IDs, paths or error text enter diagnostics.
watch(() => [connected.value, audioConfigured.value, props.runtime?.platform.buttonMapping.enabled].join(":"), () => {
  if (!props.runtime) return;
  report("coding_readiness", "passed", `connection_${connected.value ? "ready" : "pending"}_audio_${audioConfigured.value ? "configured" : "pending"}`);
}, { immediate: true });

function refreshBackup(): void {
  try {
    backup.value = readCodingBackup(localStorage);
    originalBackup.value = readCodingBackup(localStorage, "original");
    backupError.value = false;
  } catch (cause) {
    backupError.value = true;
    error.value = cause instanceof Error ? cause.message : String(cause);
    report("coding_backup_read", "failed", "storage_or_validation_failed");
  }
}

async function load(): Promise<void> {
  loading.value = true;
  error.value = "";
  const started = performance.now();
  try {
    current.value = await getButtonMappings();
    refreshBackup();
    report("coding_profile_load", "passed", "settings_loaded", started);
  } catch (cause) {
    current.value = null;
    error.value = cause instanceof Error ? cause.message : String(cause);
    report("coding_profile_load", "failed", "settings_load_failed", started);
  } finally {
    loading.value = false;
  }
}

async function changeProfile(restore: boolean): Promise<void> {
  if (busy.value || !nativeRuntime) return;
  busy.value = true;
  error.value = "";
  message.value = "";
  const started = performance.now();
  const event = restore ? "coding_profile_restore" : "coding_profile_apply";
  report(event, "unknown", "user_requested", started);
  try {
    const port = { loadMappings: getButtonMappings, saveMappings: saveButtonMappings, storage: localStorage };
    if (restore) {
      const source = restoreReview.value ?? "latest";
      current.value = await restoreCodingProfile(port, source);
      message.value = source === "original" ? "已恢复首次应用预设前的按键配置。" : "已恢复最近一次应用预设前的按键配置。";
    } else {
      const result = await applyCodingProfile(port);
      current.value = result.mappings;
      backup.value = result.backup;
      message.value = "Codex 预设已保存。请连接遥控器，并在 Codex 前台确认按键效果。";
    }
    restoreReview.value = null;
    report(event, "passed", "settings_saved", started);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    report(event, "failed", "backup_or_settings_failed", started);
  } finally {
    refreshBackup();
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <section class="coding-page">
    <header class="page-header">
      <div>
        <p class="eyebrow">遥控 Coding · 本地版</p>
        <h1>Codex 遥控</h1>
        <p class="intro muted">按住说话，改字发送，切换任务，再查看代码改动。</p>
      </div>
      <span class="badge pending">Windows</span>
    </header>

    <p v-if="!nativeRuntime" class="preview-note">浏览器预览：仅展示界面，无法连接遥控器或保存按键。</p>

    <div class="readiness-grid">
      <article class="card readiness-card">
        <div class="card-title-row"><h2>遥控器</h2><span class="badge" :class="connected ? 'success' : 'pending'">{{ connection ? connectionPhaseLabel(connection.phase) : '正在读取' }}</span></div>
        <p>{{ connection ? remoteModelLabel(connection.remoteModel) : '连接后显示型号' }}<span v-if="connection?.remoteModel !== 'unknown' && connection" class="model-code"> · {{ connection.remoteModel.toUpperCase() }}</span></p>
        <button class="secondary-button" type="button" @click="emit('navigate', 'connection')">连接设置</button>
      </article>
      <article class="card readiness-card">
        <div class="card-title-row"><h2>语音输出</h2><span class="badge" :class="audioConfigured ? 'success' : 'pending'">{{ audioConfigured ? '音频已配置' : '待配置' }}</span></div>
        <p>{{ audio ? audioPhaseLabel(audio.phase) : '正在读取' }}<span class="muted"> · 配合 Codex 听写或语音输入法</span></p>
        <button class="secondary-button" type="button" @click="emit('navigate', 'connection')">配置语音</button>
      </article>
    </div>

    <Rc003InputControl :remote-model="connection?.remoteModel ?? 'unknown'" :connected="connected" />

    <article class="card preset-card">
      <div class="card-title-row">
        <div><h2>日常 Coding 预设</h2><p class="muted">应用后启用映射，并替换下列 12 个键的配置。常用操作单按完成，主页、菜单和 TV 提供双按与长按。</p></div>
        <button class="secondary-button" type="button" @click="emit('navigate', 'buttons')">自定义按键</button>
      </div>
      <table class="mapping-table">
        <thead><tr><th scope="col">遥控器按键</th><th scope="col">单按</th><th scope="col">双按</th><th scope="col">长按</th></tr></thead>
        <tbody>
          <tr v-for="button in CODING_PROFILE_BUTTONS" :key="button">
            <th scope="row">{{ buttonLabel(button) }}</th>
            <td><span class="preset-action">{{ presetLabel(button, 'single') }}</span><span class="current-action muted">当前：{{ current ? actionSummary(current.actions[button]?.single) : '正在读取…' }}</span></td>
            <td>{{ presetLabel(button, 'double') }}</td>
            <td>{{ presetLabel(button, 'long') }}</td>
          </tr>
        </tbody>
      </table>
      <p class="preserved-note muted">主页、菜单和 TV 启用了双按，单按需等待约 0.3 秒。返回每次按下立即普通退格，快速连按继续删除，按住连续退格、松开停止。TV 长按撤销（Ctrl + Z）；可在「按键映射」中改到其他可配置的单击、双击或长按，也可禁用。方向键按住连续操作。音量＋/－用于切换上一/下一任务或标签页；RC003 请先启用三键增强。搜索文件可从命令菜单进入。</p>
      <p class="input-note">快捷键作用于当前前台窗口，请先按主页打开 Codex。确定键发送 Enter：在输入框中可能发送内容，在审批提示中可能批准操作。电源键在本方案中配置为 Esc，用于关闭弹层或取消当前操作。</p>
      <div class="preset-actions">
        <span class="muted">{{ backup ? `可恢复的最近备份：${backupDate}。首次备份也会保留。` : '每次应用前先备份当前配置，同时保留首次备份。' }}</span>
        <button class="primary-button" data-testid="apply-profile" type="button" :disabled="!canApply" @click="changeProfile(false)">{{ busy ? '正在保存…' : '应用日常默认方案' }}</button>
      </div>
      <div v-if="backup" class="restore-area">
        <div v-if="!restoreReview" class="button-row">
          <button class="secondary-button" type="button" :disabled="busy || !nativeRuntime" @click="restoreReview = 'latest'">恢复最近应用前配置</button>
          <button v-if="originalBackup" class="secondary-button" type="button" :disabled="busy || !nativeRuntime" @click="restoreReview = 'original'">恢复最初备份</button>
        </div>
        <template v-else>
          <p>将恢复{{ restoreReview === 'original' ? '首次' : '最近一次' }}应用预设前的配置，覆盖当前全部按键映射、映射开关及自定义应用列表，包括应用预设后做的修改；语音设置不变。</p>
          <div class="button-row"><button class="secondary-button" type="button" :disabled="busy" @click="restoreReview = null">取消</button><button class="secondary-button" data-testid="restore-profile" type="button" :disabled="busy || backupError" @click="changeProfile(true)">确认恢复</button></div>
        </template>
      </div>
      <p v-if="message" class="save-success" role="status">{{ message }}</p>
      <div v-if="error" class="error-text" role="alert">{{ error }} <button v-if="!current" type="button" class="secondary-button" :disabled="loading" @click="load">重新读取</button></div>
    </article>

    <article class="card voice-guide">
      <h2>说话录入，确认后发送</h2>
      <p>在「连接与语音」中选择「Codex 听写 · Ctrl + Shift + D」，也可以保留微信输入法。使用遥控器麦克风时，语音设备选择 VB-CABLE 的 CABLE Input，并让 Codex 或输入法使用 CABLE Output 麦克风。将 Codex 切到前台并点击输入框，按住遥控器语音键说话，松开结束。Codex 听写模式不需要微信输入法。</p>
      <p class="muted">沿用当前 Codex 按住听写或已选的语音热键，不改语音设置与自定义应用列表。松开语音键后检查文字，再按确定发送。音频已配置只表示输出通道就绪，识别文字是否进入 Codex 需要实际试用确认。</p>
    </article>
  </section>
</template>

<style scoped>
.coding-page { max-width: 1040px; margin: 0 auto; }
.eyebrow { margin: 0 0 9px; color: var(--accent-text); font-size: 12px; font-weight: 600; letter-spacing: .5px; }
.intro { font-size: 14px; margin: 9px 0 4px; }
.readiness-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; margin: 16px 0; }
.readiness-card { padding: 18px; }
.readiness-card p { font-size: 14px; margin: 13px 0; }
.model-code { color: var(--text-secondary); font-size: 12px; }
.preset-card { padding: 20px; }
.preset-card .card-title-row p { margin: 7px 0 16px; font-size: 13px; }
.mapping-table { width: 100%; border-collapse: collapse; font-size: 13px; table-layout: fixed; }
.mapping-table th, .mapping-table td { padding: 10px 12px; border-bottom: 1px solid var(--border); text-align: left; overflow-wrap: anywhere; }
.mapping-table thead { color: var(--text-secondary); background: var(--surface-subtle); }
.mapping-table th:first-child { width: 13%; }
.mapping-table tbody th { font-weight: 600; }
.mapping-table td { color: var(--accent-text); }
.preset-action { font-weight: 600; }
.current-action { display: block; margin-top: 5px; font-size: 11px; }
.preserved-note { font-size: 12px; margin: 13px 0; }
.input-note, .preview-note { padding: 11px 13px; background: var(--warning-surface-soft); color: var(--warning-strong); border-radius: 8px; font-size: 13px; line-height: 1.55; }
.preset-actions { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; margin-top: 18px; }
.preset-actions > span { font-size: 12px; }
.preset-actions .primary-button { min-height: 36px; }
.restore-area { margin-top: 12px; font-size: 13px; }
.restore-area .button-row { justify-content: flex-start; }
.save-success { color: var(--success-text); font-size: 13px; margin-bottom: 0; }
.voice-guide { margin-top: 16px; padding: 18px 20px; }
.voice-guide p { font-size: 13px; margin: 10px 0 0; }
</style>
