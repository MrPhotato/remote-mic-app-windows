import { computed, inject, onScopeDispose, provide, ref, watch, type InjectionKey } from "vue";
import { reportFrontendEvent } from "./frontend-diagnostics";
import {
  getRc003InputStatus, rc003InputAvailable, rc003InputErrorMessage,
  startRc003Input, stopRc003Input, stoppedRc003InputStatus, type Rc003InputStatus,
} from "./rc003-input";

export interface Rc003InputContext {
  remoteModel: string;
  connected: boolean;
  connectionGeneration?: number;
  statusAvailable?: boolean;
}

function createState(context: () => Rc003InputContext, monitor: boolean) {
  const available = monitor && rc003InputAvailable();
  const status = ref(stoppedRc003InputStatus());
  const loaded = ref(false);
  const busy = ref(false);
  const commandError = ref("");
  const readError = ref(false);
  const promptDismissed = ref(false);
  const manualPromptRequested = ref(false);
  const promptedNeutralGeneration = ref<number | null>(null);
  const promptVisible = ref(false);
  let resumePromptGeneration: number | null = null;
  const remoteModel = computed(() => context().remoteModel);
  const connected = computed(() => context().connected);
  const contextAvailable = computed(() => context().statusAvailable !== false);
  const modelConfirmed = computed(() => remoteModel.value === "rc003");
  const awaitingNeutral = computed(() => modelConfirmed.value && available && connected.value
    && contextAvailable.value && loaded.value && !readError.value
    && status.value.phase === "waiting" && status.value.lastError === "awaiting_neutral");
  const active = computed(() => modelConfirmed.value && ["starting", "waiting", "ready"].includes(status.value.phase));
  const canStart = computed(() => modelConfirmed.value && available && connected.value && contextAvailable.value
    && loaded.value && !readError.value && !busy.value && !active.value);
  const phaseLabels: Record<Rc003InputStatus["phase"], string> = {
    stopped: "未启用", starting: "正在启用…", waiting: "正在准备…", ready: "增强已就绪", failed: "增强未就绪",
  };
  const statusLabel = computed(() => !modelConfirmed.value ? "型号尚未确认" : !available ? "预览模式"
    : readError.value || !contextAvailable.value ? "状态读取失败" : !loaded.value ? "正在读取状态…"
    : active.value && !connected.value ? "等待连接恢复" : phaseLabels[status.value.phase]);
  const errorMessage = computed(() => commandError.value
    || (status.value.phase === "failed" ? rc003InputErrorMessage(status.value.lastError) : ""));
  const promptRequested = computed(() => awaitingNeutral.value && !promptDismissed.value
    && (manualPromptRequested.value || promptedNeutralGeneration.value !== status.value.generation));
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
    if (resumePromptGeneration !== null) {
      if (next.generation !== resumePromptGeneration || next.phase !== "waiting" || next.lastError !== "awaiting_neutral") {
        resumePromptGeneration = null;
      } else if (awaitingNeutral.value && !promptDismissed.value) {
        // Connection and helper snapshots can arrive in either order. Reopen
        // an interrupted visible prompt, without repeating dismissed reminders.
        manualPromptRequested.value = true;
        resumePromptGeneration = null;
      }
    }
  }

  async function refresh(): Promise<void> {
    if (disposed || !modelConfirmed.value || !available || reading || busy.value) return;
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
    if (disposed || busy.value || !modelConfirmed.value || !available || (start && !canStart.value)) return;
    busy.value = true;
    commandError.value = "";
    const requestRevision = ++revision;
    try {
      const next = await (start ? startRc003Input() : stopRc003Input());
      if (!disposed && requestRevision === revision) acceptStatus(next);
    } catch (cause) {
      if (!disposed && requestRevision === revision) commandError.value = rc003InputErrorMessage(cause);
    } finally {
      if (!disposed) busy.value = false;
    }
  }

  function dismissNeutralPrompt(): void {
    promptDismissed.value = true;
    manualPromptRequested.value = false;
    resumePromptGeneration = null;
  }

  function reopenNeutralPrompt(): void {
    if (!awaitingNeutral.value) return;
    manualPromptRequested.value = true;
    promptDismissed.value = false;
  }

  function markPromptAttempted(): void {
    promptedNeutralGeneration.value = status.value.generation;
    manualPromptRequested.value = false;
  }

  function markPromptVisible(visible: boolean): void {
    promptVisible.value = visible;
  }

  if (monitor) {
    watch([remoteModel, () => context().connectionGeneration, connected, contextAvailable], (next, previous) => {
      revision += 1;
      if (!previous.length || next[0] !== previous[0] || next[1] !== previous[1]) {
        // Re-read after the context changes; helper and connection generations
        // are independent counters and may arrive in either order.
        if (previous[0] === "rc003" && next[0] === "rc003" && next[1] !== previous[1]
          && promptVisible.value && !promptDismissed.value) {
          resumePromptGeneration = status.value.generation;
        } else {
          resumePromptGeneration = null;
        }
        loaded.value = false;
        status.value = stoppedRc003InputStatus();
        commandError.value = "";
        readError.value = false;
      }
      manualPromptRequested.value = false;
      if (timer !== undefined) clearInterval(timer);
      timer = undefined;
      if (!modelConfirmed.value || !available) return;
      void refresh();
      timer = setInterval(() => { void refresh(); }, 1000);
    }, { immediate: true });
    onScopeDispose(() => {
      disposed = true;
      revision += 1;
      if (timer !== undefined) clearInterval(timer);
      // Disposing the app UI never starts/stops the user's helper.
    });
  }

  return {
    available, status, loaded, busy, readError, remoteModel, connected, contextAvailable,
    modelConfirmed, awaitingNeutral, active, canStart, statusLabel, errorMessage,
    promptDismissed, promptRequested, change, dismissNeutralPrompt, reopenNeutralPrompt, markPromptAttempted, markPromptVisible,
  };
}

type Rc003InputState = ReturnType<typeof createState>;
const stateKey: InjectionKey<Rc003InputState> = Symbol("rc003-input-state");

/** The app owns one monitor; page controls and the persistent prompt share it. */
export function provideRc003InputState(context: () => Rc003InputContext): Rc003InputState {
  const state = createState(context, true);
  provide(stateKey, state);
  return state;
}

export function useRc003InputState(previewContext: () => Rc003InputContext = () => ({ remoteModel: "unknown", connected: false })): Rc003InputState {
  // Isolated component previews remain inert; they must not create a second monitor.
  return inject(stateKey, null) ?? createState(previewContext, false);
}
