import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isTauriRuntime } from "./bridge";
import { reportFrontendEvent } from "./frontend-diagnostics";

export interface GithubReleaseCheck {
  currentVersion: string;
  status: "available" | "up_to_date" | "no_release";
  version: string | null;
  releaseUrl: string | null;
  notes: string | null;
  publishedAt: string | null;
  prerelease: boolean;
}

export async function checkGithubRelease(): Promise<GithubReleaseCheck> {
  if (!isTauriRuntime()) throw new Error("请在无线麦应用中检查更新。");
  return invoke<GithubReleaseCheck>("check_github_release");
}

export async function openGithubRelease(url: string): Promise<void> {
  // Restrict even mocked or stale IPC results before handing them to the opener.
  if (!/^https:\/\/github\.com\/MrPhotato\/remote-mic-app-windows\/releases\/tag\/v?\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(url)) {
    throw new Error("发布页地址无效，请重新检查更新。");
  }
  if (!isTauriRuntime()) throw new Error("请在无线麦应用中打开发布页。");
  const started = performance.now();
  reportFrontendEvent({ event: "release_page", phase: "started", result: "unknown", reason: "user_requested" });
  try {
    await openUrl(url);
    reportFrontendEvent({ event: "release_page", phase: "completed", result: "passed", reason: "open_url_accepted", elapsedMs: Math.round(performance.now() - started) });
  } catch (error) {
    reportFrontendEvent({ event: "release_page", phase: "completed", result: "failed", reason: "open_url_failed", elapsedMs: Math.round(performance.now() - started) });
    throw error;
  }
}
