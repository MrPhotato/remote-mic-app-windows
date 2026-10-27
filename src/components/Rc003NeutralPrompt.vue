<script setup lang="ts">
import { onBeforeUnmount, ref, useId, watch } from "vue";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";
import { useRc003InputState } from "../lib/rc003-input-state";

const {
  status, awaitingNeutral, promptDismissed, promptRequested, readError, connected,
  contextAvailable, modelConfirmed, dismissNeutralPrompt, markPromptAttempted, markPromptVisible,
} = useRc003InputState();
const neutralTitleId = useId();
const neutralDescriptionId = useId();
const neutralDialog = ref<HTMLDialogElement | null>(null);

function closeNeutralPrompt(reason: string): void {
  if (!neutralDialog.value?.open) return;
  neutralDialog.value.close();
  markPromptVisible(false);
  reportFrontendEvent({ event: "rc003_neutral_prompt", phase: "hidden", result: "passed", reason });
}

watch([awaitingNeutral, promptDismissed, promptRequested, () => status.value.generation, neutralDialog], () => {
  if (!awaitingNeutral.value || promptDismissed.value) {
    closeNeutralPrompt(promptDismissed.value ? "user_dismissed" : readError.value || !contextAvailable.value ? "status_unavailable"
      : !modelConfirmed.value ? "model_changed" : !connected.value ? "disconnected" : `state_${status.value.phase}`);
    return;
  }
  if (!neutralDialog.value?.isConnected) return;
  if (neutralDialog.value.open) { markPromptAttempted(); return; }
  if (!promptRequested.value) return;
  markPromptAttempted();
  try {
    neutralDialog.value.showModal();
    markPromptVisible(true);
    reportFrontendEvent({ event: "rc003_neutral_prompt", phase: "shown", result: "passed", reason: "awaiting_neutral" });
  } catch {
    reportFrontendEvent({ event: "rc003_neutral_prompt", phase: "shown", result: "failed", reason: "dialog_unavailable" });
  }
}, { flush: "post" });

onBeforeUnmount(() => closeNeutralPrompt("app_closed"));
</script>

<template>
    <dialog ref="neutralDialog" class="neutral-dialog" :aria-labelledby="neutralTitleId" :aria-describedby="neutralDescriptionId" @cancel.prevent="dismissNeutralPrompt">
      <div class="neutral-direction" aria-hidden="true"><span>↑</span><span>←</span><span>●</span><span>→</span><span>↓</span></div>
      <h2 :id="neutralTitleId">按一下方向键</h2>
      <p :id="neutralDescriptionId" class="neutral-instruction">按遥控器任意方向键，然后松开。</p>
      <p class="muted">完成后，按键增强会自动就绪，提示也会消失。</p>
      <button type="button" class="secondary-button" autofocus @click="dismissNeutralPrompt">稍后操作</button>
    </dialog>
</template>

<style scoped>
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
.neutral-dialog p { margin: 10px 0; font-size: 13px; }
.neutral-dialog .neutral-instruction { font-size: 17px; font-weight: 600; }
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

@media (forced-colors: active) {
  .neutral-dialog { background: Canvas; backdrop-filter: none; }
}
</style>
