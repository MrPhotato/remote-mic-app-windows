<script setup lang="ts">
import { useReleaseCheck } from "../lib/release-check-state";
const { checking, opening, result, error, check, openRelease } = useReleaseCheck();
</script>

<template>
  <article class="card release-update-card" :aria-busy="checking">
    <div class="card-title-row">
      <div>
        <h2>软件更新</h2>
        <p class="muted">从 GitHub 检查新版本，包括预览版。</p>
      </div>
      <button class="secondary-button" type="button" :disabled="checking || opening" @click="check">
        {{ checking ? "检查中…" : "检查更新" }}
      </button>
    </div>
    <p class="update-status" role="status" aria-live="polite">
      <template v-if="checking">正在查询最新发布…</template>
      <template v-else-if="result?.status === 'available'">
        发现新版本 {{ result.version }}<span v-if="result.prerelease">（预览版）</span>
        <span class="muted"> · 当前 {{ result.currentVersion }}</span>
      </template>
      <template v-else-if="result?.status === 'up_to_date'">当前已是最新版本（{{ result.currentVersion }}）</template>
      <template v-else-if="result?.status === 'no_release'">暂时没有已发布的版本。</template>
      <template v-else-if="!error">点击检查更新，查看是否有新版本。</template>
    </p>
    <p v-if="error" class="error-text" role="alert">{{ error }}</p>
    <template v-if="result?.status === 'available'">
      <details v-if="result.notes" class="release-notes">
        <summary>更新内容</summary>
        <pre>{{ result.notes }}</pre>
      </details>
      <button v-if="result.releaseUrl" class="primary-button" type="button" :disabled="opening || checking" @click="openRelease">
        {{ opening ? "正在打开…" : "前往下载" }}
      </button>
    </template>
    <p class="muted update-source">更新来源：MrPhotato/remote-mic-app-windows</p>
  </article>
</template>

<style scoped>
.card-title-row { flex-wrap: wrap; gap: 12px; }
.card-title-row .muted { margin: 4px 0 0; }
.update-status { margin: 14px 0 10px; }
.update-status:empty { display: none; }
.release-notes { margin: 12px 0; }
.release-notes summary { cursor: pointer; }
.release-notes pre { max-height: 240px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; font: inherit; color: var(--text-secondary); }
.update-source { margin: 12px 0 0; font-size: 12px; overflow-wrap: anywhere; }
</style>
