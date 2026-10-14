<script setup lang="ts">
import { computed } from "vue";
import type { RuntimeSnapshot } from "../lib/bridge";

const props = defineProps<{ runtime: RuntimeSnapshot | null }>();

const bluetoothStatus = computed(() => {
  if (props.runtime?.platform.bleVoiceReady) return { tone: "success", label: "语音链路已就绪" };
  if (props.runtime?.platform.bleScanAvailable) return { tone: "warning", label: "待连接" };
  return { tone: "pending", label: "当前电脑不支持" };
});

const inputStatus = computed(() => {
  if (props.runtime?.platform.rawInputReady) return { tone: "success", label: "已运行" };
  if (props.runtime?.platform.windowsApiAvailable) return { tone: "warning", label: "未就绪" };
  return { tone: "pending", label: "当前电脑不支持" };
});

const audioStatus = computed(() => {
  if (props.runtime?.platform.wasapiReady) return { tone: "success", label: "已就绪" };
  if (props.runtime?.platform.windowsApiAvailable) return { tone: "warning", label: "待选择设备" };
  return { tone: "pending", label: "当前电脑不支持" };
});

/** 描边图标（24×24 视窗），与侧栏图标同一笔画粗细。 */
const ICONS = {
  bluetooth: ["M7 7l10 10-5 4V3l5 4L7 17"],
  input: [
    "M3.5 7.5h17a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5h-17A1.5 1.5 0 0 1 2 15V9a1.5 1.5 0 0 1 1.5-1.5z",
    "M6.5 10.8h.01M10 10.8h.01M13.5 10.8h.01M17 10.8h.01",
    "M8 13.6h8",
  ],
  audio: ["M12 3.5a3 3 0 0 1 3 3V12a3 3 0 0 1-6 0V6.5a3 3 0 0 1 3-3z", "M5.5 11a6.5 6.5 0 0 0 13 0", "M12 17.5V21"],
} as const;
</script>

<template>
  <section>
    <header class="page-header">
      <div>
        <h1>权限</h1>
        <p class="muted">无线麦只用到下面三项系统能力，主程序以普通用户身份运行。</p>
      </div>
    </header>

    <article class="card permission-list">
      <div class="permission-row">
        <div class="permission-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none"><path v-for="(d, i) in ICONS.bluetooth" :key="i" :d="d" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </div>
        <div><strong>蓝牙</strong><p>读取已配对的遥控器并建立连接。</p></div>
        <span class="badge" :class="bluetoothStatus.tone">
          {{ bluetoothStatus.label }}
        </span>
      </div>
      <div class="permission-row">
        <div class="permission-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none"><path v-for="(d, i) in ICONS.input" :key="i" :d="d" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </div>
        <div><strong>按键监听与模拟</strong><p>只监听小米遥控器的按键；按键模拟仅在自定义映射启用时使用。</p></div>
        <span class="badge" :class="inputStatus.tone">{{ inputStatus.label }}</span>
      </div>
      <div class="permission-row">
        <div class="permission-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none"><path v-for="(d, i) in ICONS.audio" :key="i" :d="d" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </div>
        <div><strong>音频设备</strong><p>将遥控器声音传给目标应用，不改动系统默认设备。</p></div>
        <span class="badge" :class="audioStatus.tone">{{ audioStatus.label }}</span>
      </div>
    </article>
    <p class="muted permission-footnote">RC003 按键增强需要管理员授权，只在你手动开启时请求，不随应用自动启动。</p>
  </section>
</template>

<style scoped>
.permission-icon svg { width: 20px; height: 20px; }
.permission-footnote { margin: 12px 4px 0; font-size: 12.5px; line-height: 1.6; }
</style>
