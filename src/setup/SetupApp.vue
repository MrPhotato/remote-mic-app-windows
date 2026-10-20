<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import WindowChrome from "../components/WindowChrome.vue";
import { preview, setupApi, type SetupInfo, type InstalledInfo } from "./api";

const nativeWindow = preview ? null : getCurrentWindow();
const info = ref<SetupInfo | null>(null);
const installed = ref<InstalledInfo | null>(null);
const directory = ref("");
const desktopShortcut = ref(true);
const launch = ref(true);
const loading = ref(false);
const browsing = ref(false);
const installing = ref(false);
const finishing = ref(false);
const windowBusy = ref(false);
const error = ref("");
const windowError = ref("");
const step = computed(() => installed.value ? 2 : installing.value ? 1 : 0);
const unavailable = computed(() => loading.value || browsing.value || installing.value || finishing.value);
const validDirectory = computed(() => !!directory.value.trim() && !/[\x00-\x1f]/.test(directory.value));
const errorText = (value: unknown, fallback: string): string => typeof value === "string" && value.trim() ? value : fallback;

async function load(): Promise<void> {
  if (unavailable.value) return;
  loading.value = true;
  error.value = "";
  try {
    info.value = await setupApi.info();
    directory.value = info.value.directory;
  } catch (cause) {
    error.value = errorText(cause, "无法读取安装信息，请重试。");
  } finally { loading.value = false; }
}

async function browse(): Promise<void> {
  if (preview || unavailable.value || info.value?.existing) return;
  browsing.value = true;
  error.value = "";
  try {
    const selected = await setupApi.browse(directory.value);
    if (selected !== null) directory.value = selected;
  } catch (cause) {
    error.value = errorText(cause, "无法打开文件夹选择窗口。也可以直接填写安装位置。");
  } finally { browsing.value = false; }
}

async function install(): Promise<void> {
  if (preview || unavailable.value || !info.value || installed.value || !validDirectory.value) return;
  installing.value = true;
  error.value = "";
  try {
    installed.value = await setupApi.install(directory.value.trim(), desktopShortcut.value);
  } catch (cause) {
    error.value = errorText(cause, "安装未完成，请重试。");
  } finally { installing.value = false; }
}

async function finish(): Promise<void> {
  if (preview || unavailable.value || !installed.value) return;
  finishing.value = true;
  error.value = "";
  try {
    await setupApi.finish(launch.value);
  } catch (cause) {
    error.value = errorText(cause, "无法完成此操作，请重试。");
  } finally { finishing.value = false; }
}

async function windowAction(action: "minimize" | "maximize" | "close"): Promise<void> {
  if (!nativeWindow || windowBusy.value || action === "maximize") return;
  if (action === "close" && (installing.value || finishing.value || browsing.value)) return;
  windowBusy.value = true;
  windowError.value = "";
  try {
    if (action === "minimize") await nativeWindow.minimize();
    else await nativeWindow.close();
  } catch { windowError.value = "窗口操作未完成，请重试。"; }
  finally { windowBusy.value = false; }
}

onMounted(load);
</script>

<template>
  <div class="setup-shell">
    <div class="ambient" aria-hidden="true">
      <span class="ambient-light indigo" /><span class="ambient-light mint" /><span class="ambient-light violet" />
    </div>
    <WindowChrome :available="!!nativeWindow" :busy="windowBusy" :maximizable="false"
      :close-disabled="installing || finishing || browsing" :error="windowError" @action="windowAction">
      <span data-tauri-drag-region>无线麦 SayAll · 安装</span>
    </WindowChrome>

    <main class="setup-main">
      <div class="setup-brand">
        <img src="/app-logo.png" alt="" width="60" height="60" />
        <div><h1>无线麦 <span>SayAll</span></h1><p class="muted">让遥控器成为你的语音与按键助手</p></div>
        <span v-if="info" class="setup-version">{{ preview ? '界面预览' : `Windows · ${info.version}` }}</span>
      </div>

      <ol class="setup-steps" aria-label="安装进度">
        <li v-for="(label, index) in ['选择位置', '安装', '完成']" :key="label" :class="{ current: step === index, complete: step > index }" :aria-current="step === index ? 'step' : undefined">
          <span class="step-number" aria-hidden="true"><svg v-if="step > index" viewBox="0 0 16 16"><path d="m3 8 3 3 7-7" /></svg><template v-else>{{ index + 1 }}</template></span>
          {{ label }}
        </li>
      </ol>

      <section class="card setup-card" :aria-busy="loading || installing || finishing" aria-labelledby="setup-heading">
        <template v-if="installed">
          <div class="setup-complete-heading">
            <span class="setup-check" aria-hidden="true"><svg viewBox="0 0 24 24"><path d="m5 12 4 4L19 6" /></svg></span>
            <div><h2 id="setup-heading">安装完成</h2><p class="muted">无线麦 {{ installed.version }} 已准备就绪</p></div>
          </div>
          <p class="setup-installed-directory" :title="installed.directory">{{ installed.directory }}</p>
          <label class="setup-choice"><input v-model="launch" type="checkbox" :disabled="finishing" data-testid="launch-option" /><span>打开无线麦</span></label>
        </template>
        <template v-else-if="installing">
          <h2 id="setup-heading">正在安装无线麦</h2>
          <p class="muted">请稍候，安装完成后即可使用。</p>
          <div class="setup-progress" role="progressbar" aria-label="正在安装" aria-valuetext="正在安装，请稍候"><span /></div>
          <p class="setup-installed-directory" :title="directory">{{ directory }}</p>
        </template>
        <template v-else>
          <h2 id="setup-heading">{{ !info || info.existing ? '安装无线麦' : '安装到哪里？' }}</h2>
          <p class="setup-description muted">{{ info?.existing ? '已找到无线麦，将保留你的设置。' : '选择安装文件夹。' }}</p>
          <label for="setup-directory" class="setup-field-label">安装位置</label>
          <div class="setup-directory-row">
            <input id="setup-directory" v-model="directory" type="text" class="setup-path" autocomplete="off" spellcheck="false" :readonly="info?.existing" :disabled="loading || !info || browsing" :aria-describedby="info?.existing ? 'setup-location-note' : undefined" />
            <button v-if="!info?.existing" type="button" class="secondary-button" :disabled="preview || unavailable || !info" @click="browse">{{ browsing ? '选择中…' : '浏览…' }}</button>
          </div>
          <p v-if="info?.existing" id="setup-location-note" class="setup-location-note muted">升级保留原安装位置；更换位置需卸载后重装。</p>
          <label class="setup-choice"><input v-model="desktopShortcut" type="checkbox" :disabled="unavailable || !info" data-testid="desktop-option" /><span>创建桌面快捷方式</span></label>
        </template>

        <div class="setup-feedback" aria-live="polite">
          <p v-if="error" class="error-text" role="alert">{{ error }}</p>
          <p v-else-if="loading" class="muted" role="status">正在读取安装信息…</p>
          <p v-else-if="preview" class="muted">这是界面预览。安装请运行安装包。</p>
        </div>
        <footer class="setup-actions">
          <span class="setup-footnote muted">{{ installing ? '安装期间请保持此窗口打开' : '仅为当前 Windows 用户安装' }}</span>
          <button v-if="!info && !loading" type="button" class="primary-button" @click="load">重试</button>
          <button v-else-if="installed" type="button" class="primary-button" :disabled="preview || finishing" @click="finish">{{ finishing ? '正在完成…' : '完成' }}</button>
          <button v-else type="button" class="primary-button" :disabled="preview || unavailable || !info || !validDirectory" @click="install">{{ installing ? '正在安装…' : '开始安装' }}</button>
        </footer>
      </section>
    </main>
  </div>
</template>
