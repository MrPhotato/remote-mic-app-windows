<script setup lang="ts">
withDefaults(defineProps<{
  focused?: boolean;
  available?: boolean;
  busy?: boolean;
  closeDisabled?: boolean;
  maximizable?: boolean;
  maximized?: boolean;
  closeTitle?: string;
  error?: string;
}>(), {
  focused: true, available: true, busy: false, closeDisabled: false,
  maximizable: true, maximized: false, closeTitle: "关闭窗口", error: "",
});
defineEmits<{ action: [action: "minimize" | "maximize" | "close"] }>();
</script>

<template>
  <header class="window-titlebar" :class="{ inactive: !focused }" aria-label="窗口控制">
    <div class="window-drag-area" data-tauri-drag-region>
      <span class="window-caption" data-tauri-drag-region><slot>无线麦 <span data-tauri-drag-region>SayAll</span></slot></span>
    </div>
    <span v-if="error" class="window-action-error" role="alert">{{ error }}</span>
    <div class="window-controls">
      <button type="button" class="window-control" aria-label="最小化" title="最小化" :disabled="!available || busy" @click="$emit('action', 'minimize')">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10" /></svg>
      </button>
      <button v-if="maximizable" type="button" class="window-control" :aria-label="maximized ? '还原窗口' : '最大化'" :title="maximized ? '还原窗口' : '最大化'" :disabled="!available || busy" @click="$emit('action', 'maximize')">
        <svg v-if="maximized" viewBox="0 0 16 16" aria-hidden="true"><path d="M5.5 4V2.5h8v8H12M2.5 5.5h8v8h-8z" /></svg>
        <svg v-else viewBox="0 0 16 16" aria-hidden="true"><rect x="3" y="3" width="10" height="10" rx=".5" /></svg>
      </button>
      <button type="button" class="window-control window-close" aria-label="关闭窗口" :title="closeTitle" :disabled="!available || busy || closeDisabled" @click="$emit('action', 'close')">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m3.5 3.5 9 9m0-9-9 9" /></svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.window-titlebar { height: var(--titlebar-height); display: flex; align-items: center; position: relative; z-index: 30; padding: 0 8px 0 18px; user-select: none; background: var(--glass-rail); backdrop-filter: blur(var(--glass-blur-strong)) saturate(var(--glass-saturate)); border-bottom: 1px solid var(--glass-rim); box-shadow: inset 0 1px 0 var(--glass-edge-soft); }
.window-drag-area { display: flex; align-items: center; align-self: stretch; flex: 1; min-width: 0; }
.window-caption { font-size: 12px; font-weight: 600; color: var(--text-secondary); letter-spacing: .01em; }
.window-caption > span { margin-left: 5px; font-weight: 400; }
.inactive .window-caption { color: var(--text-nav); }
.window-controls { display: flex; gap: 3px; }
.window-control { width: 40px; height: 28px; display: grid; place-items: center; padding: 0; border: 1px solid var(--glass-rim); border-radius: 9px; color: var(--text-control); background: var(--glass-control); box-shadow: inset 0 1px 0 var(--glass-edge-soft); transition: background-color var(--motion-press), border-color var(--motion-press), box-shadow var(--motion-press); }
.window-control svg { width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 1.2; pointer-events: none; }
.window-control:not(:disabled):hover { background: var(--surface-hover); border-color: var(--glass-edge-soft); box-shadow: inset 0 1px 0 var(--glass-edge-soft), 0 2px 6px var(--shadow-control); }
.window-control:not(:disabled):active { background: var(--accent-surface-strong); }
.window-close:not(:disabled):hover { background: #c42b36; color: #fff; border-color: rgba(255,255,255,.25); }
.window-action-error { color: var(--error-text); font-size: 12px; margin-right: 12px; }
@supports not (backdrop-filter: blur(1px)) { .window-titlebar { background: var(--surface-canvas); } }
@media (prefers-reduced-motion: reduce) { .window-control { transition: none; } }
@media (forced-colors: active) { .window-titlebar { background: Canvas; border-color: CanvasText; box-shadow: none; backdrop-filter: none; } .window-control { color: ButtonText; border-color: ButtonText; } .window-control:not(:disabled):hover { background: Highlight; color: HighlightText; } }
</style>
