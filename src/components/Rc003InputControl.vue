<script setup lang="ts">
import { useId } from "vue";
import { useRc003InputState } from "../lib/rc003-input-state";

// Keep the page's existing component contract; the app owns the runtime context.
const props = defineProps<{ remoteModel: string; connected: boolean }>();
const permissionId = useId();
const {
  available, status, busy, readError, remoteModel, connected, contextAvailable, modelConfirmed,
  awaitingNeutral, active, canStart, statusLabel, errorMessage, change, reopenNeutralPrompt,
} = useRc003InputState(() => props);
</script>
<template>
  <section v-if="remoteModel !== 'rc001'" class="card rc003-input-control" aria-label="RC003 按键增强">
    <div class="enhancement-heading">
      <div class="enhancement-title">
        <h2>RC003 按键增强</h2>
        <span class="badge" :class="status.phase === 'ready' && connected && contextAvailable && !readError ? 'success' : 'pending'" role="status">{{ statusLabel }}</span>
      </div>
      <button
        class="enhancement-switch"
        type="button"
        role="switch"
        aria-label="RC003 按键增强"
        :aria-checked="active"
        :aria-describedby="permissionId"
        :aria-busy="busy"
        :disabled="active ? busy || !available : !canStart"
        @click="change(!active)"
      >
        <span class="switch-track" aria-hidden="true"><span class="switch-thumb"></span></span>
        <span aria-hidden="true">{{ busy ? active ? '正在关闭…' : '等待 Windows 授权…' : active ? '已开启' : '已关闭' }}</span>
      </button>
    </div>
    <p :id="permissionId" class="permission-note"><strong>RC003 · 需要管理员权限</strong>。开启时请在 Windows 授权提示中确认。</p>
    <p class="muted">启用返回、音量＋/－，让确认键长按只执行设定动作，并防止 TV 键多输入字符。每次启动无线麦后需手动开启。</p>
    <p v-if="!modelConfirmed" class="muted">尚未确认遥控器型号，按键增强暂不可用；型号确认后会自动更新。</p>
    <p v-else-if="!available" class="muted">请在 Windows 应用中启用按键增强。</p>
    <p v-else-if="active && !connected || status.phase === 'waiting' && status.lastError === 'waiting_for_connection'" class="muted">正在等待遥控器连接恢复，请稍候。</p>
    <p v-else-if="!connected" class="muted">请先在「连接与语音」中连接 RC003。</p>
    <div v-else-if="awaitingNeutral" class="neutral-inline">
      <strong>请按一下方向键并松开。</strong>
      <button type="button" class="secondary-button" @click="reopenNeutralPrompt">查看提示</button>
    </div>
    <p v-else-if="status.phase === 'waiting'" class="muted">正在准备或恢复按键增强，请稍候。</p>
    <p v-else-if="status.phase === 'ready'" class="muted">按键动作可在下方设置。关闭增强前请先松开遥控器按键。</p>
    <p v-if="errorMessage" class="error-text" role="alert">{{ errorMessage }}</p>
    <p v-else-if="readError" class="error-text" role="alert">暂时无法读取按键增强状态，正在重试。</p>

  </section>
</template>

<style scoped>
.rc003-input-control {
  margin: 14px 0;
  padding: 15px 18px;
  border-color: var(--accent-border);
  border-top-color: var(--glass-edge);
  background:
    linear-gradient(100deg, var(--accent-surface) 0%, transparent 46%),
    var(--card);
}
.enhancement-heading, .enhancement-title { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
.enhancement-heading { justify-content: space-between; }
.enhancement-title h2 { margin: 0; font-size: 16px; }
.rc003-input-control p { margin: 9px 0 0; font-size: 12.5px; line-height: 1.6; }
.permission-note strong { color: var(--text-primary); }
.enhancement-switch {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
  min-height: 44px;
  padding: 8px 12px 8px 9px;
  border: 1px solid var(--glass-rim);
  border-top-color: var(--glass-edge);
  border-radius: var(--radius-pill);
  background: var(--glass-control);
  box-shadow: inset 0 1px 0 var(--glass-edge-soft);
  color: var(--text-primary);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: border-color 0.18s, background-color 0.18s;
}
.enhancement-switch:hover:not(:disabled) { border-color: var(--accent-border); background: var(--surface-hover); }
.enhancement-switch:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
.enhancement-switch:disabled { opacity: .6; cursor: not-allowed; }
.switch-track {
  display: inline-flex;
  align-items: center;
  width: 44px;
  height: 26px;
  padding: 3px;
  box-sizing: border-box;
  border: 1px solid transparent;
  border-radius: 999px;
  background: var(--toggle-off);
  box-shadow: inset 0 1px 3px rgba(20, 25, 40, 0.25);
  transition: background-color var(--motion-state) var(--ease-out-quart), box-shadow var(--motion-state) var(--ease-out-quart);
}
/* 滑块用 transform 滑到右端（轨道内宽 36 − 滑块 18 = 18px），与通用开关同一曲线。 */
.switch-thumb {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--toggle-thumb);
  box-shadow: 0 1px 3px var(--toggle-shadow);
  transition: transform var(--motion-state) var(--ease-out-quart);
}
.neutral-inline {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 12px;
  padding: 12px 14px;
  border: 1px solid color-mix(in srgb, var(--warning) 50%, transparent);
  border-radius: var(--radius-control);
  background: var(--warning-surface);
  color: var(--warning-text);
  font-size: 14px;
}
.enhancement-switch[aria-checked="true"] .switch-thumb { transform: translate3d(18px, 0, 0); }
.enhancement-switch[aria-checked="true"] .switch-track {
  background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 84%, white) 0%, var(--accent) 100%);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.34);
}
@media (prefers-reduced-motion: reduce) {
  .switch-track, .switch-thumb { transition: none; }
}
@media (forced-colors: active) {
  .switch-track { border-color: ButtonText; background: Canvas; }
  .switch-thumb { background: ButtonText; box-shadow: none; }
  .enhancement-switch[aria-checked="true"] .switch-track { background: Highlight; }
  .enhancement-switch[aria-checked="true"] .switch-thumb { background: HighlightText; }

}
</style>
