import { ref } from "vue";
import { checkGithubRelease, openGithubRelease, type GithubReleaseCheck } from "./github-release";

// Keep a manual check and its result alive when the user switches pages.
const checking = ref(false);
const opening = ref(false);
const result = ref<GithubReleaseCheck | null>(null);
const error = ref("");

async function check(): Promise<void> {
  if (checking.value || opening.value) return;
  checking.value = true;
  result.value = null;
  error.value = "";
  try {
    result.value = await checkGithubRelease();
  } catch (failure) {
    error.value = failure instanceof Error ? failure.message : String(failure);
  } finally {
    checking.value = false;
  }
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
  return { checking, opening, result, error, check, openRelease };
}
