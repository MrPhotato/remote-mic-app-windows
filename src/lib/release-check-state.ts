import { ref } from "vue";
import { checkGithubRelease, openGithubRelease, type GithubReleaseCheck } from "./github-release";
import { reportFrontendEvent } from "./frontend-diagnostics";

// Shared by startup, the update notice and About; changing pages never starts a request.
const checking = ref(false);
const opening = ref(false);
const result = ref<GithubReleaseCheck | null>(null);
const error = ref("");
const checkedAt = ref<number | null>(null);
const fromCache = ref(false);
let startupChecked = false;
let installedVersion = "";

const CACHE_KEY = "sayall.github-release-check.v1";
const SUCCESS_CACHE_MS = 6 * 60 * 60 * 1000;
const RETRY_COOLDOWN_MS = 15 * 60 * 1000;
const RELEASE_PAGE = "https://github.com/MrPhotato/remote-mic-app-windows/releases/tag/";

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
  if (checking.value || opening.value) return;
  checking.value = true;
  // Retain the previous details and timestamp while rechecking so the card does
  // not collapse. The UI shows progress and disables download until completion.
  error.value = "";
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
  if (checking.value || opening.value) return;
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
  if (!result.value?.releaseUrl || opening.value || checking.value) return;
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

export function useReleaseCheck() {
  return { checking, opening, result, error, checkedAt, fromCache, check, checkAtStartup, openRelease };
}

/** Test isolation without deleting the persistent cache used by restart tests. */
export function resetReleaseCheckForTests(): void {
  checking.value = false;
  opening.value = false;
  result.value = null;
  error.value = "";
  checkedAt.value = null;
  fromCache.value = false;
  startupChecked = false;
  installedVersion = "";
}
