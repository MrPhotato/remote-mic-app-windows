<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/bridge";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";

const nativeWindow = isTauriRuntime() ? getCurrentWindow() : null;
const maximized = ref(false);
const focused = ref(true);
const busy = ref(false);
const error = ref("");
let disposed = false;
let stateRequest = 0;
let focusEvents = 0;
const listeners: UnlistenFn[] = [];

async function refreshMaximized(): Promise<void> {
  if (!nativeWindow || disposed) return;
  const request = ++stateRequest;
  try {
    const value = await nativeWindow.isMaximized();
    if (!disposed && request === stateRequest) maximized.value = value;
  } catch {
    reportFrontendEvent({ event: "window_chrome", phase: "state", result: "failed", reason: "maximized_read_failed" });
  }
}

async function retainListener(pending: Promise<UnlistenFn>): Promise<void> {
  try {
    const unlisten = await pending;
    if (disposed) unlisten();
    else listeners.push(unlisten);
  } catch {
    reportFrontendEvent({ event: "window_chrome", phase: "listen", result: "failed", reason: "listener_failed" });
  }
}

async function act(action: "minimize" | "maximize" | "close"): Promise<void> {
  if (!nativeWindow || busy.value) return;
  busy.value = true;
  error.value = "";
  const started = performance.now();
  try {
    if (action === "minimize") await nativeWindow.minimize();
    else if (action === "maximize") {
      await nativeWindow.toggleMaximize();
      await refreshMaximized();
    } else await nativeWindow.close(); // Existing CloseRequested handler owns tray behavior.
    reportFrontendEvent({ event: "window_chrome", phase: action, result: "passed", reason: "native_request_accepted", elapsedMs: Math.round(performance.now() - started) });
  } catch {
    error.value = "窗口操作未完成，请重试。";
    reportFrontendEvent({ event: "window_chrome", phase: action, result: "failed", reason: "native_request_failed", elapsedMs: Math.round(performance.now() - started) });
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  if (!nativeWindow) return;
  void refreshMaximized();
  void nativeWindow.isFocused().then(value => { if (!disposed && focusEvents === 0) focused.value = value; }).catch(() => {
    reportFrontendEvent({ event: "window_chrome", phase: "state", result: "failed", reason: "focus_read_failed" });
  });
  void retainListener(nativeWindow.onResized(() => { void refreshMaximized(); }));
  void retainListener(nativeWindow.onFocusChanged(({ payload }) => { focusEvents++; if (!disposed) focused.value = payload; }));
});
onUnmounted(() => {
  disposed = true;
  stateRequest++;
  for (const unlisten of listeners) unlisten();
});
</script>

<template>
  <header class="window-titlebar" :class="{ inactive: !focused }" aria-label="窗口控制">
    <div class="window-drag-area" data-tauri-drag-region>
      <span class="window-caption" data-tauri-drag-region>无线麦 <span data-tauri-drag-region>SayAll</span></span>
    </div>
    <span v-if="error" class="window-action-error" role="alert">{{ error }}</span>
    <div class="window-controls">
      <button type="button" class="window-control" aria-label="最小化" title="最小化" :disabled="!nativeWindow || busy" @click="act('minimize')">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10" /></svg>
      </button>
      <button type="button" class="window-control" :aria-label="maximized ? '还原窗口' : '最大化'" :title="maximized ? '还原窗口' : '最大化'" :disabled="!nativeWindow || busy" @click="act('maximize')">
        <svg v-if="maximized" viewBox="0 0 16 16" aria-hidden="true"><path d="M5.5 4V2.5h8v8H12M2.5 5.5h8v8h-8z" /></svg>
        <svg v-else viewBox="0 0 16 16" aria-hidden="true"><rect x="3" y="3" width="10" height="10" rx=".5" /></svg>
      </button>
      <button type="button" class="window-control window-close" aria-label="关闭窗口" title="关闭到托盘" :disabled="!nativeWindow || busy" @click="act('close')">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m3.5 3.5 9 9m0-9-9 9" /></svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.window-titlebar { height: var(--titlebar-height); display: flex; align-items: center; position: relative; z-index: 30; padding: 0 8px 0 18px; user-select: none; background: var(--glass-rail); backdrop-filter: blur(var(--glass-blur-strong)) saturate(var(--glass-saturate)); border-bottom: 1px solid var(--glass-rim); box-shadow: inset 0 1px 0 var(--glass-edge-soft); }
.window-drag-area { display: flex; align-items: center; align-self: stretch; flex: 1; min-width: 0; }
.window-caption { font-size: 12px; font-weight: 600; color: var(--text-secondary); letter-spacing: .01em; }
.window-caption > span { margin-left: 5px; font-weight: 400; }
.inactive .window-caption { color: var(--text-nav); }
.window-controls { display: flex; gap: 3px; }
.window-control { width: 40px; height: 28px; display: grid; place-items: center; padding: 0; border: 1px solid var(--glass-rim); border-radius: 9px; color: var(--text-control); background: var(--glass-control); box-shadow: inset 0 1px 0 var(--glass-edge-soft); transition: background-color var(--motion-press), border-color var(--motion-press), box-shadow var(--motion-press); }
.window-control svg { width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 1.2; pointer-events: none; }
.window-control:not(:disabled):hover { background: var(--surface-hover); border-color: var(--glass-edge-soft); box-shadow: inset 0 1px 0 var(--glass-edge-soft), 0 2px 6px var(--shadow-control); }
.window-control:not(:disabled):active { background: var(--accent-surface-strong); }
.window-close:not(:disabled):hover { background: #c42b36; color: #fff; border-color: rgba(255,255,255,.25); }
.window-action-error { color: var(--error-text); font-size: 12px; margin-right: 12px; }
@supports not (backdrop-filter: blur(1px)) { .window-titlebar { background: var(--surface-canvas); } }
@media (prefers-reduced-motion: reduce) { .window-control { transition: none; } }
@media (forced-colors: active) { .window-titlebar { background: Canvas; border-color: CanvasText; box-shadow: none; backdrop-filter: none; } .window-control { color: ButtonText; border-color: ButtonText; } .window-control:not(:disabled):hover { background: Highlight; color: HighlightText; } }
</style>
