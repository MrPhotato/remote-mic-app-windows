<script lang="ts">
// Page navigation and temporary read errors must not repeat the same reminder.
let promptedNeutralGeneration: number | null = null;
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, onUnmounted, ref, useId, watch } from "vue";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";
import {
  getRc003InputStatus, rc003InputAvailable, rc003InputErrorMessage,
  startRc003Input, stopRc003Input, stoppedRc003InputStatus, type Rc003InputStatus,
} from "../lib/rc003-input";

const props = defineProps<{ remoteModel: string; connected: boolean }>();
const permissionId = useId();
const neutralTitleId = useId();
const neutralDescriptionId = useId();
const neutralDialog = ref<HTMLDialogElement | null>(null);
const promptDismissed = ref(false);
const manualPromptRequested = ref(false);
const available = rc003InputAvailable();
const status = ref(stoppedRc003InputStatus());
const loaded = ref(false);
const busy = ref(false);
const commandError = ref("");
const readError = ref(false);
const modelConfirmed = computed(() => props.remoteModel === "rc003");
const awaitingNeutral = computed(() => modelConfirmed.value && available && props.connected && loaded.value && !readError.value
  && status.value.phase === "waiting" && status.value.lastError === "awaiting_neutral");
const active = computed(() => modelConfirmed.value && ["starting", "waiting", "ready"].includes(status.value.phase));
const canStart = computed(() => modelConfirmed.value && available && props.connected && loaded.value && !readError.value && !busy.value && !active.value);
const phaseLabels: Record<Rc003InputStatus["phase"], string> = {
  stopped: "未启用", starting: "正在启用…", waiting: "正在准备…", ready: "增强已就绪", failed: "增强未就绪",
};
const statusLabel = computed(() => !modelConfirmed.value ? "型号尚未确认" : !available ? "预览模式" : readError.value ? "状态读取失败" : !loaded.value ? "正在读取状态…" : active.value && !props.connected ? "等待连接恢复" : phaseLabels[status.value.phase]);
const errorMessage = computed(() => commandError.value || (status.value.phase === "failed" ? rc003InputErrorMessage(status.value.lastError) : ""));
let timer: ReturnType<typeof setInterval> | undefined;
let disposed = false;
let reading = false;
let revision = 0;

function acceptStatus(next: Rc003InputStatus): void {
  if (loaded.value && next.generation < status.value.generation) return;
  if (next.generation !== status.value.generation) promptDismissed.value = false;
  if (!loaded.value || next.phase !== status.value.phase) {
    reportFrontendEvent({ event: "rc003_input_state", phase: "observed", result: next.phase === "failed" ? "failed" : "passed", reason: `phase_${next.phase}` });
  }
  status.value = next;
  loaded.value = true;
  readError.value = false;
}

function closeNeutralPrompt(reason: string): void {
  if (!neutralDialog.value?.open) return;
  neutralDialog.value.close();
  reportFrontendEvent({ event: "rc003_neutral_prompt", phase: "hidden", result: "passed", reason });
}

function dismissNeutralPrompt(): void {
  promptDismissed.value = true;
}

function reopenNeutralPrompt(): void {
  manualPromptRequested.value = true;
  promptDismissed.value = false;
}

watch([awaitingNeutral, promptDismissed, manualPromptRequested, () => status.value.generation, neutralDialog], () => {
  if (!awaitingNeutral.value || promptDismissed.value) {
    manualPromptRequested.value = false;
    closeNeutralPrompt(promptDismissed.value ? "user_dismissed" : readError.value ? "status_unavailable"
      : !props.connected ? "disconnected" : `state_${status.value.phase}`);
    return;
  }
  if (disposed || !neutralDialog.value?.isConnected) return;
  if (neutralDialog.value.open) { promptedNeutralGeneration = status.value.generation; return; }
  if (promptedNeutralGeneration === status.value.generation && !manualPromptRequested.value) return;
  promptedNeutralGeneration = status.value.generation;
  manualPromptRequested.value = false;
  try {
    neutralDialog.value.showModal();
    reportFrontendEvent({ event: "rc003_neutral_prompt", phase: "shown", result: "passed", reason: "awaiting_neutral" });
  } catch {
    // The inline instruction remains available if the host cannot show a dialog.
    reportFrontendEvent({ event: "rc003_neutral_prompt", phase: "shown", result: "failed", reason: "dialog_unavailable" });
  }
}, { flush: "post" });

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
  const requestRevision = revision;
  try {
    const next = await (start ? startRc003Input() : stopRc003Input());
    if (!disposed && requestRevision === revision) acceptStatus(next);
  } catch (cause) {
    if (!disposed && requestRevision === revision) commandError.value = rc003InputErrorMessage(cause);
  } finally {
    if (!disposed) busy.value = false;
  }
}

watch(() => props.remoteModel, (model) => {
  closeNeutralPrompt("model_changed");
  revision += 1;
  loaded.value = false;
  status.value = stoppedRc003InputStatus();
  commandError.value = "";
  readError.value = false;
  if (timer !== undefined) clearInterval(timer);
  timer = undefined;
  if (model !== "rc003" || !available) return;
  void refresh();
  timer = setInterval(() => { void refresh(); }, 1000);
}, { immediate: true });

onBeforeUnmount(() => closeNeutralPrompt("page_left"));

onUnmounted(() => {
  disposed = true;
  revision += 1;
  if (timer !== undefined) clearInterval(timer);
  // Leaving a page must not change the user's process-wide enhancement choice.
});
</script>

<template>
  <section v-if="remoteModel !== 'rc001'" class="card rc003-input-control" aria-label="RC003 按键增强">
    <div class="enhancement-heading">
      <div class="enhancement-title">
        <h2>RC003 按键增强</h2>
        <span class="badge" :class="status.phase === 'ready' && connected && !readError ? 'success' : 'pending'" role="status">{{ statusLabel }}</span>
      </div>
      <button
        class="enhancement-switch"
        type="button"
        role="switch"
        aria-label="RC003 按键增强"
        :aria-checked="active"
        :aria-describedby="permissionId"
        :aria-busy="busy"
        :disabled="active ? busy || !available : !canStart"
        @click="change(!active)"
      >
        <span class="switch-track" aria-hidden="true"><span class="switch-thumb"></span></span>
        <span aria-hidden="true">{{ busy ? active ? '正在关闭…' : '等待 Windows 授权…' : active ? '已开启' : '已关闭' }}</span>
      </button>
    </div>
    <p :id="permissionId" class="permission-note"><strong>RC003 · 需要管理员权限</strong>。开启时请在 Windows 授权提示中确认。</p>
    <p class="muted">启用返回、音量＋/－，让确认键长按只执行设定动作，并防止 TV 键多输入字符。每次启动无线麦后需手动开启。</p>
    <p v-if="!modelConfirmed" class="muted">尚未确认遥控器型号，按键增强暂不可用；型号确认后会自动更新。</p>
    <p v-else-if="!available" class="muted">请在 Windows 应用中启用按键增强。</p>
    <p v-else-if="active && !connected || status.phase === 'waiting' && status.lastError === 'waiting_for_connection'" class="muted">正在等待遥控器连接恢复，请稍候。</p>
    <p v-else-if="!connected" class="muted">请先在「连接与语音」中连接 RC003。</p>
    <div v-else-if="awaitingNeutral" class="neutral-inline">
      <strong>请按一下方向键并松开。</strong>
      <button type="button" class="secondary-button" @click="reopenNeutralPrompt">查看提示</button>
    </div>
    <p v-else-if="status.phase === 'waiting'" class="muted">正在准备或恢复按键增强，请稍候。</p>
    <p v-else-if="status.phase === 'ready'" class="muted">按键动作可在下方设置。关闭增强前请先松开遥控器按键。</p>
    <p v-if="errorMessage" class="error-text" role="alert">{{ errorMessage }}</p>
    <p v-else-if="readError" class="error-text" role="alert">暂时无法读取按键增强状态，正在重试。</p>
    <dialog ref="neutralDialog" class="neutral-dialog" :aria-labelledby="neutralTitleId" :aria-describedby="neutralDescriptionId" @cancel.prevent="dismissNeutralPrompt">
      <div class="neutral-direction" aria-hidden="true"><span>↑</span><span>←</span><span>●</span><span>→</span><span>↓</span></div>
      <h2 :id="neutralTitleId">按一下方向键</h2>
      <p :id="neutralDescriptionId" class="neutral-instruction">按遥控器任意方向键，然后松开。</p>
      <p class="muted">完成后，按键增强会自动就绪，提示也会消失。</p>
      <button type="button" class="secondary-button" autofocus @click="dismissNeutralPrompt">稍后操作</button>
    </dialog>
  </section>
</template>

<style scoped>
.rc003-input-control {
  margin: 14px 0;
  padding: 15px 18px;
  border-color: var(--accent-border);
  border-top-color: var(--glass-edge);
  background:
    linear-gradient(100deg, var(--accent-surface) 0%, transparent 46%),
    var(--card);
}
.enhancement-heading, .enhancement-title { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
.enhancement-heading { justify-content: space-between; }
.enhancement-title h2 { margin: 0; font-size: 16px; }
.rc003-input-control p { margin: 9px 0 0; font-size: 12.5px; line-height: 1.6; }
.permission-note strong { color: var(--text-primary); }
.enhancement-switch {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
  min-height: 44px;
  padding: 8px 12px 8px 9px;
  border: 1px solid var(--glass-rim);
  border-top-color: var(--glass-edge);
  border-radius: var(--radius-pill);
  background: var(--glass-control);
  box-shadow: inset 0 1px 0 var(--glass-edge-soft);
  color: var(--text-primary);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: border-color 0.18s, background-color 0.18s;
}
.enhancement-switch:hover:not(:disabled) { border-color: var(--accent-border); background: var(--surface-hover); }
.enhancement-switch:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
.enhancement-switch:disabled { opacity: .6; cursor: not-allowed; }
.switch-track {
  display: inline-flex;
  align-items: center;
  width: 44px;
  height: 26px;
  padding: 3px;
  box-sizing: border-box;
  border: 1px solid transparent;
  border-radius: 999px;
  background: var(--toggle-off);
  box-shadow: inset 0 1px 3px rgba(20, 25, 40, 0.25);
  transition: background-color var(--motion-state) var(--ease-out-quart), box-shadow var(--motion-state) var(--ease-out-quart);
}
/* 滑块用 transform 滑到右端（轨道内宽 36 − 滑块 18 = 18px），与通用开关同一曲线。 */
.switch-thumb {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--toggle-thumb);
  box-shadow: 0 1px 3px var(--toggle-shadow);
  transition: transform var(--motion-state) var(--ease-out-quart);
}
.neutral-inline {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 12px;
  padding: 12px 14px;
  border: 1px solid color-mix(in srgb, var(--warning) 50%, transparent);
  border-radius: var(--radius-control);
  background: var(--warning-surface);
  color: var(--warning-text);
  font-size: 14px;
}
/* 浮层是最上一层玻璃：更厚的模糊、更大的圆角与投影。
   显式打开（[open]）时整体稍微上浮并淡入，背景也淡入；
   原生 <dialog> 关闭由浏览器一次性移除，无需 leave 过渡。 */
.neutral-dialog {
  width: min(400px, calc(100vw - 40px));
  max-height: calc(100vh - 40px);
  box-sizing: border-box;
  padding: 30px 28px 26px;
  border: 1px solid var(--glass-rim);
  border-top-color: var(--glass-edge);
  border-radius: var(--radius-overlay);
  background: var(--glass-overlay);
  backdrop-filter: blur(var(--glass-blur-strong)) saturate(var(--glass-saturate));
  -webkit-backdrop-filter: blur(var(--glass-blur-strong)) saturate(var(--glass-saturate));
  color: var(--text-primary);
  box-shadow: inset 0 1px 0 var(--glass-edge), 0 30px 80px -24px var(--shadow-float);
  text-align: center;
  overflow: auto;
  opacity: 0;
  transform: translate3d(0, 10px, 0) scale(var(--motion-overlay-scale));
}
.neutral-dialog[open] {
  animation: dialog-rise var(--motion-expand) var(--ease-out-quart) forwards;
}
.neutral-dialog::backdrop {
  background: rgba(8, 10, 18, 0.42);
  backdrop-filter: blur(4px);
  opacity: 0;
}
.neutral-dialog[open]::backdrop {
  animation: dialog-backdrop var(--motion-expand) var(--ease-out-quart) forwards;
}
@keyframes dialog-rise {
  to { opacity: 1; transform: translate3d(0, 0, 0) scale(1); }
}
@keyframes dialog-backdrop {
  to { opacity: 1; }
}
@media (prefers-reduced-motion: reduce) {
  .neutral-dialog[open] { animation: none; opacity: 1; transform: none; }
  .neutral-dialog[open]::backdrop { animation: none; opacity: 1; }
}
.neutral-dialog h2 { margin: 18px 0 12px; font-size: 24px; }
.rc003-input-control .neutral-dialog p { margin: 10px 0; font-size: 13px; }
.rc003-input-control .neutral-dialog .neutral-instruction { font-size: 17px; font-weight: 600; }
.neutral-dialog button { margin-top: 18px; min-height: 38px; }
.neutral-direction {
  display: grid;
  grid-template-columns: repeat(3, 24px);
  grid-template-rows: repeat(3, 24px);
  place-content: center;
  width: 96px;
  height: 96px;
  margin: 0 auto;
  border: 1px solid var(--accent-border);
  border-radius: 50%;
  background: radial-gradient(circle at 35% 30%, var(--glass-edge-soft), transparent 60%), var(--accent-surface);
  box-shadow: inset 0 1px 0 var(--glass-edge), 0 10px 26px -14px color-mix(in srgb, var(--accent) 60%, transparent);
  color: var(--accent-text);
  font-size: 23px;
  line-height: 24px;
}
.neutral-direction span:first-child { grid-column: 2; }
.neutral-direction span:nth-child(2) { grid-column: 1; }
.neutral-direction span:nth-child(3) { font-size: 12px; }
.neutral-direction span:last-child { grid-column: 2; }
.enhancement-switch[aria-checked="true"] .switch-thumb { transform: translate3d(18px, 0, 0); }
.enhancement-switch[aria-checked="true"] .switch-track {
  background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 84%, white) 0%, var(--accent) 100%);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.34);
}
@media (prefers-reduced-motion: reduce) {
  .switch-track, .switch-thumb { transition: none; }
}
@media (forced-colors: active) {
  .switch-track { border-color: ButtonText; background: Canvas; }
  .switch-thumb { background: ButtonText; box-shadow: none; }
  .enhancement-switch[aria-checked="true"] .switch-track { background: Highlight; }
  .enhancement-switch[aria-checked="true"] .switch-thumb { background: HighlightText; }
  .neutral-dialog { background: Canvas; backdrop-filter: none; }
}
</style>
