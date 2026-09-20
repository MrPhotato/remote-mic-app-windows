<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";
import {
  getRc003InputStatus, rc003InputAvailable, rc003InputErrorMessage,
  startRc003Input, stopRc003Input, stoppedRc003InputStatus, type Rc003InputStatus,
} from "../lib/rc003-input";

const props = defineProps<{ remoteModel: string; connected: boolean }>();
const available = rc003InputAvailable();
const status = ref(stoppedRc003InputStatus());
const loaded = ref(false);
const busy = ref(false);
const commandError = ref("");
const readError = ref(false);
const active = computed(() => ["starting", "waiting", "ready"].includes(status.value.phase));
const canStart = computed(() => available && props.connected && loaded.value && !readError.value && !busy.value && !active.value);
const phaseLabels: Record<Rc003InputStatus["phase"], string> = {
  stopped: "未启用", starting: "正在启用…", waiting: "正在准备…", ready: "增强已就绪", failed: "增强未就绪",
};
const statusLabel = computed(() => !available ? "预览模式" : readError.value ? "状态读取失败" : !loaded.value ? "正在读取状态…" : phaseLabels[status.value.phase]);
const errorMessage = computed(() => commandError.value || (status.value.phase === "failed" ? rc003InputErrorMessage(status.value.lastError) : ""));
let timer: ReturnType<typeof setInterval> | undefined;
let disposed = false;
let reading = false;
let revision = 0;

function acceptStatus(next: Rc003InputStatus): void {
  if (loaded.value && next.generation < status.value.generation) return;
  if (!loaded.value || next.phase !== status.value.phase) {
    reportFrontendEvent({ event: "rc003_input_state", phase: "observed", result: next.phase === "failed" ? "failed" : "passed", reason: `phase_${next.phase}` });
  }
  status.value = next;
  loaded.value = true;
  readError.value = false;
}

async function refresh(): Promise<void> {
  if (disposed || props.remoteModel !== "rc003" || !available || reading || busy.value) return;
  reading = true;
  const requestRevision = revision;
  try {
    const next = await getRc003InputStatus();
    if (!disposed && requestRevision === revision) acceptStatus(next);
  } catch {
    if (!disposed && requestRevision === revision) {
      if (!readError.value) reportFrontendEvent({ event: "rc003_input_state", phase: "observed", result: "failed", reason: "status_read_failed" });
      readError.value = true;
    }
  } finally {
    reading = false;
  }
}

async function change(start: boolean): Promise<void> {
  if (busy.value || props.remoteModel !== "rc003" || !available || (start && !canStart.value)) return;
  busy.value = true;
  commandError.value = "";
  revision += 1;
  try {
    const next = await (start ? startRc003Input() : stopRc003Input());
    if (!disposed) acceptStatus(next);
  } catch (cause) {
    if (!disposed) commandError.value = rc003InputErrorMessage(cause);
  } finally {
    if (!disposed) busy.value = false;
  }
}

watch(() => props.remoteModel, (model) => {
  revision += 1;
  loaded.value = false;
  if (timer !== undefined) clearInterval(timer);
  timer = undefined;
  if (model !== "rc003" || !available) return;
  void refresh();
  timer = setInterval(() => { void refresh(); }, 1000);
}, { immediate: true });

onUnmounted(() => {
  disposed = true;
  revision += 1;
  if (timer !== undefined) clearInterval(timer);
  // Leaving a page must not change the user's process-wide enhancement choice.
});
</script>

<template>
  <section v-if="remoteModel === 'rc003'" class="card rc003-input-control" aria-label="RC003 三键增强">
    <div class="enhancement-heading">
      <div class="enhancement-title"><h2>RC003 三键增强</h2><span class="badge" :class="status.phase === 'ready' && !readError ? 'success' : 'pending'" role="status">{{ statusLabel }}</span></div>
      <button v-if="active" class="secondary-button" type="button" :disabled="busy || !available" @click="change(false)">{{ busy ? '正在处理…' : '停止增强' }}</button>
      <button v-else class="secondary-button" type="button" :disabled="!canStart" @click="change(true)">{{ busy ? '等待 Windows 授权…' : '启用三键增强' }}</button>
    </div>
    <p>识别返回、音量＋和音量－需要一次 Windows 管理员授权。仅辅助程序申请权限，主程序保持普通权限，无需修改驱动。</p>
    <p class="muted">每次启动无线麦后需手动启用。未配置动作的增强按键只显示高亮；要调节音量，请在「按键映射」中设置音量快捷键。</p>
    <p v-if="!available" class="muted">浏览器预览和仿真模式无法启用三键增强。</p>
    <p v-else-if="active && !connected || status.phase === 'waiting' && status.lastError === 'waiting_for_connection'" class="muted">正在等待遥控器连接恢复，请稍候。</p>
    <p v-else-if="!connected" class="muted">请先在「连接与语音」中连接 RC003。</p>
    <p v-else-if="status.phase === 'waiting' && status.lastError === 'awaiting_neutral'" class="muted">请按一下方向键并松开，完成首次初始化。</p>
    <p v-else-if="status.phase === 'waiting'" class="muted">正在准备或恢复三键增强，请稍候。</p>
    <p v-else-if="status.phase === 'ready'" class="muted">按下返回或音量键，在「按键映射」中检查高亮与已配置动作。</p>
    <p v-if="errorMessage" class="error-text" role="alert">{{ errorMessage }}</p>
    <p v-else-if="readError" class="error-text" role="alert">暂时无法读取三键增强状态，正在重试。</p>
  </section>
</template>

<style scoped>
.rc003-input-control { margin: 14px 0; padding: 15px 18px; }
.enhancement-heading, .enhancement-title { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
.enhancement-heading { justify-content: space-between; }
.enhancement-title h2 { margin: 0; font-size: 14px; }
.rc003-input-control p { margin: 9px 0 0; font-size: 12px; line-height: 1.6; }
.enhancement-heading button { flex-shrink: 0; }
</style>
