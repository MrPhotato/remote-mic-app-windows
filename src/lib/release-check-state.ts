import { ref } from "vue";
import { checkGithubRelease, openGithubRelease, type GithubReleaseCheck } from "./github-release";
import { installAppUpdate, subscribeAppUpdateProgress, type AppUpdateProgress } from "./bridge";
import { reportFrontendEvent } from "./frontend-diagnostics";

// Shared by startup, the update notice and About; changing pages never starts a request.
const checking = ref(false);
const opening = ref(false);
const result = ref<GithubReleaseCheck | null>(null);
const error = ref("");
const checkedAt = ref<number | null>(null);
const fromCache = ref(false);
const installing = ref(false);
const installError = ref("");
const installProgress = ref<AppUpdateProgress | null>(null);
const completedUpdate = ref<{ version: string; notes: string | null } | null>(null);
let installAttempt = 0;
let progressUnsubscribe: (() => void) | null = null;
let startupChecked = false;
let installedVersion = "";

const CACHE_KEY = "sayall.github-release-check.v1";
const SUCCESS_CACHE_MS = 6 * 60 * 60 * 1000;
const RETRY_COOLDOWN_MS = 15 * 60 * 1000;
const RELEASE_PAGE = "https://github.com/MrPhotato/remote-mic-app-windows/releases/tag/";
const PENDING_UPDATE_KEY = "sayall.pending-update.v1";

function rememberPendingUpdate(selected: GithubReleaseCheck): void {
  try {
    localStorage.setItem(PENDING_UPDATE_KEY, JSON.stringify({
      fromVersion: selected.currentVersion, version: selected.version, notes: selected.notes?.slice(0, 16000) ?? null,
    }));
  } catch {
    reportFrontendEvent({ event: "release_install", phase: "notice", result: "failed", reason: "storage_unavailable" });
  }
}

function readCompletedUpdate(currentVersion: string): void {
  try {
    const raw = localStorage.getItem(PENDING_UPDATE_KEY);
    localStorage.removeItem(PENDING_UPDATE_KEY);
    if (!raw) return;
    const pending: unknown = JSON.parse(raw);
    if (!pending || typeof pending !== "object") return;
    const item = pending as Record<string, unknown>;
    if (item.version !== currentVersion || typeof item.fromVersion !== "string" || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(item.fromVersion) || item.fromVersion === currentVersion
      || !(item.notes === null || (typeof item.notes === "string" && item.notes.length <= 16000))) return;
    completedUpdate.value = { version: currentVersion, notes: item.notes as string | null };
    reportFrontendEvent({ event: "release_install", phase: "completed", result: "passed", reason: "target_version_running" });
  } catch {
    // A missing or damaged receipt must never prevent checking or using the app.
  }
}

function clearPendingUpdate(): void {
  try { localStorage.removeItem(PENDING_UPDATE_KEY); } catch { /* Storage is optional. */ }
}

interface CachedCheck {
  schema: 1;
  currentVersion: string;
  checkedAt: number;
  result: GithubReleaseCheck | null;
}

function isCachedResult(value: unknown, currentVersion: string): value is GithubReleaseCheck {
  if (!value || typeof value !== "object") return false;
  const item = value as Partial<GithubReleaseCheck>;
  if (item.currentVersion !== currentVersion || typeof item.prerelease !== "boolean"
    || !(item.notes === null || typeof item.notes === "string")
    || !(item.publishedAt === null || typeof item.publishedAt === "string")) return false;
  if (item.status === "no_release") return item.version === null && item.releaseUrl === null;
  if (item.status !== "available" && item.status !== "up_to_date") return false;
  if (typeof item.version !== "string" || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(item.version)) return false;
  return item.releaseUrl === `${RELEASE_PAGE}v${item.version}` || item.releaseUrl === `${RELEASE_PAGE}${item.version}`;
}

function readCache(currentVersion: string): CachedCheck | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const cached = JSON.parse(raw) as Partial<CachedCheck> | null;
    if (!cached || cached.schema !== 1 || cached.currentVersion !== currentVersion
      || typeof cached.checkedAt !== "number" || !Number.isFinite(cached.checkedAt)
      || !(cached.result === null || isCachedResult(cached.result, currentVersion))) return null;
    const age = Date.now() - cached.checkedAt;
    const ttl = cached.result ? SUCCESS_CACHE_MS : RETRY_COOLDOWN_MS;
    // Clock rollback or a different installed version must not suppress fresh checks.
    return age >= 0 && age < ttl ? cached as CachedCheck : null;
  } catch {
    return null;
  }
}

function saveCache(value: GithubReleaseCheck | null): void {
  if (!installedVersion) return;
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify({
      schema: 1, currentVersion: installedVersion, checkedAt: Date.now(), result: value,
    } satisfies CachedCheck));
  } catch {
    // Storage can be unavailable; the in-memory guard still limits this lifecycle.
    reportFrontendEvent({ event: "release_check", phase: "cache", result: "failed", reason: "storage_unavailable" });
  }
}

async function runCheck(source: "startup" | "manual"): Promise<void> {
  if (checking.value || opening.value || installing.value) return;
  checking.value = true;
  // Retain the previous details and timestamp while rechecking so the card does
  // not collapse. The UI shows progress and disables download until completion.
  error.value = "";
  installError.value = "";
  installProgress.value = null;
  fromCache.value = false;
  const started = performance.now();
  reportFrontendEvent({ event: "release_check", phase: "started", result: "unknown", reason: `${source}_requested` });
  // Persist the attempt before the request: WebView reloads cannot create a request loop.
  saveCache(null);
  try {
    result.value = await checkGithubRelease();
    installedVersion = result.value.currentVersion;
    checkedAt.value = Date.now();
    saveCache(result.value);
    reportFrontendEvent({ event: "release_check", phase: "completed", result: "passed", reason: `${source}_${result.value.status}`, elapsedMs: Math.round(performance.now() - started) });
  } catch (failure) {
    result.value = null;
    error.value = failure instanceof Error ? failure.message : String(failure);
    saveCache(null);
    reportFrontendEvent({ event: "release_check", phase: "completed", result: "failed", reason: `${source}_check_failed`, elapsedMs: Math.round(performance.now() - started) });
  } finally {
    checking.value = false;
  }
}

// Manual checks intentionally bypass the startup cache and cooldown.
async function check(): Promise<void> {
  await runCheck("manual");
}

async function checkAtStartup(currentVersion: string): Promise<void> {
  if (startupChecked) return;
  startupChecked = true;
  installedVersion = currentVersion;
  readCompletedUpdate(currentVersion);
  if (checking.value || opening.value || installing.value) return;
  const cached = readCache(currentVersion);
  if (cached) {
    result.value = cached.result;
    checkedAt.value = cached.result ? cached.checkedAt : null;
    fromCache.value = Boolean(cached.result);
    error.value = cached.result ? "" : "上次检查未完成，可点击检查更新重试。";
    reportFrontendEvent({ event: "release_check", phase: "skipped", result: "unknown", reason: cached.result ? "startup_cache_used" : "startup_retry_cooldown" });
    return;
  }
  await runCheck("startup");
}

async function openRelease(): Promise<void> {
  if (!result.value?.releaseUrl || opening.value || checking.value || installing.value) return;
  opening.value = true;
  error.value = "";
  try {
    await openGithubRelease(result.value.releaseUrl);
  } catch {
    error.value = "无法打开发布页，请稍后重试。";
  } finally {
    opening.value = false;
  }
}

async function install(): Promise<void> {
  const selected = result.value;
  if (installing.value || checking.value || opening.value || selected?.status !== "available" || !selected.version) return;
  const attempt = ++installAttempt;
  installing.value = true;
  installError.value = "";
  error.value = "";
  installProgress.value = { phase: "preparing", downloaded: 0, contentLength: null, finished: false };
  const started = performance.now();
  reportFrontendEvent({ event: "release_install", phase: "started", result: "unknown", reason: "user_requested" });
  let active = true;
  let unsubscribe: (() => void) | null = null;
  try {
    // Subscribe before invoking, and keep this subscription independent of page
    // mounts. A user can leave About without losing or duplicating an update.
    unsubscribe = await subscribeAppUpdateProgress((event) => {
      if (!active || attempt !== installAttempt || !installing.value) return;
      const order = { preparing: 0, downloading: 1, verifying: 2, installing: 3 };
      const previous = installProgress.value;
      if (!(event.phase in order) || (previous && order[event.phase] < order[previous.phase])) return;
      installProgress.value = {
        ...event,
        downloaded: Number.isFinite(event.downloaded) ? Math.max(previous?.downloaded ?? 0, event.downloaded, 0) : previous?.downloaded ?? 0,
        contentLength: event.contentLength !== null && Number.isFinite(event.contentLength) && event.contentLength > 0 ? event.contentLength : null,
      };
    });
    if (attempt !== installAttempt) return;
    progressUnsubscribe = unsubscribe;
    rememberPendingUpdate(selected);
    await installAppUpdate(selected.version);
    if (attempt !== installAttempt) return;
    // Windows normally exits inside the command. A resolved call means the
    // installer was requested, not that installation or restart has completed.
    installProgress.value = { ...installProgress.value!, phase: "installing" };
    reportFrontendEvent({ event: "release_install", phase: "handoff", result: "unknown", reason: "installer_requested", elapsedMs: Math.round(performance.now() - started) });
  } catch (failure) {
    if (attempt !== installAttempt) return;
    installing.value = false;
    clearPendingUpdate();
    installError.value = failure instanceof Error ? failure.message : String(failure);
    reportFrontendEvent({ event: "release_install", phase: "completed", result: "failed", reason: "update_failed", elapsedMs: Math.round(performance.now() - started) });
  } finally {
    active = false;
    unsubscribe?.();
    if (progressUnsubscribe === unsubscribe) progressUnsubscribe = null;
  }
}

export function useReleaseCheck() {
  return { checking, opening, result, error, checkedAt, fromCache, installing, installError, installProgress, completedUpdate, check, checkAtStartup, openRelease, install };
}

/** Test isolation without deleting the persistent cache used by restart tests. */
export function resetReleaseCheckForTests(): void {
  installAttempt += 1;
  progressUnsubscribe?.();
  progressUnsubscribe = null;
  installing.value = false;
  installError.value = "";
  installProgress.value = null;
  completedUpdate.value = null;
  checking.value = false;
  opening.value = false;
  result.value = null;
  error.value = "";
  checkedAt.value = null;
  fromCache.value = false;
  startupChecked = false;
  installedVersion = "";
}
