<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/bridge";
import { reportFrontendEvent } from "../lib/frontend-diagnostics";
import WindowChrome from "./WindowChrome.vue";

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
  <WindowChrome
    :focused="focused"
    :available="!!nativeWindow"
    :busy="busy"
    :maximized="maximized"
    :error="error"
    close-title="关闭到托盘"
    @action="act"
  />
</template>
