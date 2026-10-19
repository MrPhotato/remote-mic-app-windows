<script setup lang="ts">
import { computed } from "vue";
import { useReleaseCheck } from "../lib/release-check-state";
const { checking, opening, result, error, checkedAt, fromCache, installing, installError, installProgress, completedUpdate, check, openRelease, install } = useReleaseCheck();
const displayedError = computed(() => installError.value || error.value);
const downloadPercent = computed(() => {
  const progress = installProgress.value;
  return progress?.contentLength ? Math.min(100, Math.floor(progress.downloaded / progress.contentLength * 100)) : null;
});
const installButtonText = computed(() => {
  if (!installing.value) return installError.value ? "重试更新" : "更新并重启";
  switch (installProgress.value?.phase) {
    case "downloading": return downloadPercent.value === null ? "正在下载…" : `正在下载 ${downloadPercent.value}%`;
    case "verifying": return "正在校验…";
    case "installing": return "正在安装…";
    default: return "正在准备…";
  }
});
const installStatusText = computed(() => {
  const progress = installProgress.value;
  if (progress?.phase === "installing") return "正在安装，即将重新打开无线麦…";
  if (progress?.phase === "verifying") return "下载完成，正在校验安装包…";
  if (progress?.phase === "downloading") {
    const downloaded = (progress.downloaded / 1048576).toFixed(1);
    const total = progress.contentLength ? ` / ${(progress.contentLength / 1048576).toFixed(1)} MB` : " MB";
    return `正在下载安装包（${downloaded}${total}）`;
  }
  return "正在准备更新…";
});
const lastCheckedText = computed(() => checkedAt.value === null ? "" : new Date(checkedAt.value).toLocaleString("zh-CN", {
  month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false,
}));
</script>

<template>
  <article class="card release-update-card" :aria-busy="checking || installing">
    <div class="card-title-row">
      <div>
        <h2>软件更新</h2>
        <p class="muted">启动时检查新版本，包括预览版。点击更新后自动安装并重启。</p>
      </div>
      <button class="secondary-button" type="button" :disabled="checking || opening || installing" @click="check">
        {{ checking ? "检查中…" : "检查更新" }}
      </button>
    </div>
    <p class="update-status" :class="{ 'error-text': displayedError }" :role="displayedError ? 'alert' : 'status'" aria-live="polite">
      <template v-if="installing">{{ installStatusText }}</template>
      <template v-else-if="checking">正在查询最新发布…</template>
      <template v-else-if="displayedError">{{ displayedError }}</template>
      <template v-else-if="result?.status === 'available'">
        发现新版本 {{ result.version }}<span v-if="result.prerelease">（预览版）</span>
        <span class="muted"> · 当前 {{ result.currentVersion }}</span>
      </template>
      <template v-else-if="result?.status === 'up_to_date'">{{ fromCache ? "上次检查时已是最新版本" : "当前已是最新版本" }}（{{ result.currentVersion }}）</template>
      <template v-else-if="result?.status === 'no_release'">暂时没有已发布的版本。</template>
      <template v-else>启动后会自动检查，也可点击检查更新。</template>
    </p>
    <p class="muted checked-time" :aria-hidden="!lastCheckedText">
      <template v-if="lastCheckedText">上次检查：{{ lastCheckedText }}</template>
      <template v-else>&nbsp;</template>
    </p>
    <div v-if="completedUpdate" class="completed-update">
      <div class="completed-update-title">
        <strong>已更新至 {{ completedUpdate.version }}</strong>
        <button class="release-page-button" type="button" @click="completedUpdate = null">知道了</button>
      </div>
      <details v-if="completedUpdate.notes" class="release-notes">
        <summary>本次更新内容</summary>
        <pre>{{ completedUpdate.notes }}</pre>
      </details>
    </div>
    <template v-if="result?.status === 'available'">
      <details v-if="result.notes" class="release-notes">
        <summary>更新内容</summary>
        <pre>{{ result.notes }}</pre>
      </details>
      <div class="update-actions">
        <button class="primary-button install-button" type="button" :disabled="opening || checking || installing" @click="install">
          <span v-if="installing && installProgress?.phase === 'downloading' && downloadPercent !== null" class="download-fill" :style="{ width: `${downloadPercent}%` }" aria-hidden="true" />
          <span class="install-label">{{ installButtonText }}</span>
        </button>
        <button v-if="result.releaseUrl" class="release-page-button" type="button" :disabled="opening || checking || installing" @click="openRelease">
          {{ opening ? "正在打开…" : "查看发布页" }}
        </button>
      </div>
    </template>
    <p class="muted update-source">更新来源：MrPhotato/remote-mic-app-windows</p>
  </article>
</template>

<style scoped>
.card-title-row { flex-wrap: wrap; gap: 12px; }
.card-title-row .muted { margin: 4px 0 0; }
.card-title-row .secondary-button { min-width: 88px; }
.update-status { margin: 14px 0 10px; min-height: 1.5em; line-height: 1.5; font-size: inherit; overflow-wrap: anywhere; }
.checked-time { margin: 0 0 10px; min-height: 1.5em; line-height: 1.5; font-size: 12px; }
.release-notes { margin: 12px 0; }
.release-notes summary { cursor: pointer; }
.release-notes pre { max-height: 240px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; font: inherit; color: var(--text-secondary); }
.update-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; }
.completed-update { margin: 12px 0; border-left: 3px solid var(--accent-primary, #a6a0ff); padding-left: 12px; }
.completed-update-title { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.install-button { position: relative; isolation: isolate; overflow: hidden; min-width: 140px; }
.install-button:disabled { opacity: 0.85; }
.download-fill { position: absolute; inset: 0 auto 0 0; background: rgb(255 255 255 / 22%); pointer-events: none; }
.install-label { position: relative; }
.release-page-button { border: 0; border-radius: 8px; padding: 8px 4px; background: transparent; color: var(--text-secondary); font: inherit; font-size: 13px; cursor: pointer; }
.release-page-button:hover:not(:disabled) { color: var(--text-primary); text-decoration: underline; }
.release-page-button:disabled { cursor: default; opacity: 0.5; }
.update-source { margin: 12px 0 0; font-size: 12px; overflow-wrap: anywhere; }
</style>
