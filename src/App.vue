<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import Sidebar from "./components/Sidebar.vue";
import { getButtonMappings, getRuntimeSnapshot, isTauriRuntime, type ButtonMappings, type RuntimeSnapshot } from "./lib/bridge";
import { useReleaseCheck } from "./lib/release-check-state";
import { reportFrontendEvent } from "./lib/frontend-diagnostics";
import {
  detectReloadRecovery,
  isBrowserReloadAccelerator,
  loadPersistedPage,
  persistActivePage,
  touchLiveness,
  type PageId,
} from "./navigation";
import AboutPage from "./pages/AboutPage.vue";
import WindowTitlebar from "./components/WindowTitlebar.vue";
import ButtonsPage from "./pages/ButtonsPage.vue";
import CodingPage from "./pages/CodingPage.vue";
import ConnectionPage from "./pages/ConnectionPage.vue";
import PermissionsPage from "./pages/PermissionsPage.vue";

const initialPage = loadPersistedPage() ?? "coding";
const { result: releaseResult, checkAtStartup } = useReleaseCheck();
let releaseStartupScheduled = false;
const activePage = ref<PageId>(initialPage);
const pageReady = ref(initialPage !== "buttons");
const preparedMappings = ref<ButtonMappings | null>(null);
const navigationPending = ref(false);
const navigationError = ref("");
let navigationRequest = 0;
let disposed = false;
watch(activePage, (page) => persistActivePage(page));
const runtime = ref<RuntimeSnapshot | null>(null);
const loadError = ref("");
let runtimePollTimer: ReturnType<typeof setInterval> | undefined;
let initialRuntimeReported = false;

async function navigate(page: PageId): Promise<void> {
  if (page === "buttons" && navigationPending.value) return;
  const request = ++navigationRequest;
  navigationError.value = "";
  if (page !== "buttons") {
    navigationPending.value = false;
    activePage.value = page;
    pageReady.value = true;
    return;
  }
  if (activePage.value === page && pageReady.value) return;
  navigationPending.value = true;
  const started = performance.now();
  try {
    const mappings = await getButtonMappings();
    if (disposed || request !== navigationRequest) return;
    preparedMappings.value = mappings;
    activePage.value = page;
    pageReady.value = true;
    reportFrontendEvent({ event: "buttons_page_prepare", phase: "completed", result: "passed", reason: "configuration_ready_before_navigation", elapsedMs: Math.round(performance.now() - started) });
  } catch {
    if (disposed || request !== navigationRequest) return;
    navigationError.value = "按键配置读取失败，请重试。";
    reportFrontendEvent({ event: "buttons_page_prepare", phase: "completed", result: "failed", reason: "configuration_read_failed", elapsedMs: Math.round(performance.now() - started) });
  } finally {
    if (!disposed && request === navigationRequest) navigationPending.value = false;
  }
}

function handleWindowKeydown(event: KeyboardEvent): void {
  if (isBrowserReloadAccelerator(event)) {
    event.preventDefault();
    event.stopPropagation();
  }
}

const activeComponent = computed(() => ({
  coding: CodingPage,
  buttons: ButtonsPage,
  connection: ConnectionPage,
  permissions: PermissionsPage,
  about: AboutPage,
})[activePage.value]);

const voiceLive = computed(() => runtime.value?.platform.connection.voiceState === "streaming");

onMounted(async () => {
  if (initialPage === "buttons") void navigate("buttons");
  const refreshRuntime = async () => {
    try {
      runtime.value = await getRuntimeSnapshot();
      if (!releaseStartupScheduled && isTauriRuntime() && runtime.value?.appVersion) {
        releaseStartupScheduled = true;
        void checkAtStartup(runtime.value.appVersion);
      }
      loadError.value = "";
      touchLiveness();
      if (!initialRuntimeReported) {
        reportFrontendEvent({
          event: "runtime_snapshot",
          phase: "completed",
          result: "passed",
          reason: "initial_ipc_ready",
        });
        initialRuntimeReported = true;
      }
    } catch (error) {
      loadError.value = error instanceof Error ? error.message : String(error);
      if (!initialRuntimeReported) {
        reportFrontendEvent({
          event: "runtime_snapshot",
          phase: "completed",
          result: "failed",
          reason: "initial_ipc_failed",
        });
        initialRuntimeReported = true;
      }
    }
  };
  // 心跳在前次会话仍新鲜 = 本次挂载是渲染进程崩溃后的重载恢复
  // （Bugs/2026-09-12）；上报后日志可区分冷启动与重载。
  if (detectReloadRecovery()) {
    reportFrontendEvent({
      event: "webview_reload_recovery",
      phase: "completed",
      result: "passed",
      reason: "fresh_liveness_heartbeat",
    });
  }
  window.addEventListener("keydown", handleWindowKeydown, true);
  await refreshRuntime();
  if (disposed) return;
  runtimePollTimer = setInterval(() => {
    void refreshRuntime();
  }, 1_000);
});

onUnmounted(() => {
  disposed = true;
  navigationRequest++;
  if (runtimePollTimer) clearInterval(runtimePollTimer);
  window.removeEventListener("keydown", handleWindowKeydown, true);
});
</script>

<template>
  <div class="ambient" :class="{ live: voiceLive }" aria-hidden="true">
    <span class="ambient-light indigo"></span>
    <span class="ambient-light mint"></span>
    <span class="ambient-light violet"></span>
  </div>
  <WindowTitlebar />
  <div class="app-shell">
    <Sidebar :active-page="activePage" :update-available="releaseResult?.status === 'available'" @select="navigate" />
    <main class="content" :aria-busy="navigationPending">
      <div v-if="loadError" class="error-banner" role="alert">暂时读不到运行状态，正在重试：{{ loadError }}</div>
      <div v-if="navigationError" class="error-banner" role="alert">
        {{ navigationError }} <button class="secondary-button" type="button" @click="navigate('buttons')">重新读取</button>
      </div>
      <p v-if="!pageReady && navigationPending" class="muted" role="status">正在读取按键配置…</p>
      <!-- 页面切换：out-in 顺序让旧页面先淡出，新页面再上浮进场。
           玻璃面板的 backdrop-filter 对后一帧全重新模糊，用 mode=out-in
           避免两屏同时在渲染时的峰值绘制开销。 -->
      <Transition name="page" mode="out-in">
        <component v-if="pageReady" :is="activeComponent" :key="activePage" :runtime="runtime" v-bind="activePage === 'buttons' ? { initialMappings: preparedMappings } : {}" @navigate="navigate" />
      </Transition>
    </main>
  </div>
</template>
