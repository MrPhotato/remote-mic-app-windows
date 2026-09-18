<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import Sidebar from "./components/Sidebar.vue";
import { getRuntimeSnapshot, type RuntimeSnapshot } from "./lib/bridge";
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
import ButtonsPage from "./pages/ButtonsPage.vue";
import CodingPage from "./pages/CodingPage.vue";
import ConnectionPage from "./pages/ConnectionPage.vue";
import PermissionsPage from "./pages/PermissionsPage.vue";

const activePage = ref<PageId>(loadPersistedPage() ?? "coding");
watch(activePage, (page) => persistActivePage(page));
const runtime = ref<RuntimeSnapshot | null>(null);
const loadError = ref("");
let runtimePollTimer: ReturnType<typeof setInterval> | undefined;
let initialRuntimeReported = false;

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

onMounted(async () => {
  const refreshRuntime = async () => {
    try {
      runtime.value = await getRuntimeSnapshot();
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
  runtimePollTimer = setInterval(() => {
    void refreshRuntime();
  }, 1_000);
});

onUnmounted(() => {
  if (runtimePollTimer) clearInterval(runtimePollTimer);
  window.removeEventListener("keydown", handleWindowKeydown, true);
});
</script>

<template>
  <div class="app-shell">
    <Sidebar :active-page="activePage" @select="activePage = $event" />
    <main class="content">
      <div v-if="loadError" class="error-banner">无法读取运行状态：{{ loadError }}</div>
      <component :is="activeComponent" :runtime="runtime" @navigate="activePage = $event" />
    </main>
  </div>
</template>
