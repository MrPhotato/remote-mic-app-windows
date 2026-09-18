// @vitest-environment jsdom

import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Ref } from "vue";
import { ref } from "vue";
import type { RuntimeSnapshot } from "../lib/bridge";
import { useAppUpdate, type AppUpdatePhase } from "../lib/app-update";
import AboutPage from "./AboutPage.vue";

const phase: Ref<AppUpdatePhase> = ref("idle");
const info = ref<Awaited<ReturnType<typeof useAppUpdate>>["info"]["value"]>(null);
const errorMessage = ref("");
const progress = ref({ downloaded: 0, contentLength: null as number | null, finished: false });
const includePrereleases = ref(false);
const preferenceBusy = ref(false);
const preferenceError = ref("");
const check = vi.fn<() => Promise<void>>();
const install = vi.fn<() => Promise<void>>();
const loadUpdatePreferences = vi.fn<() => Promise<void>>();
const setIncludePrereleases = vi.fn<(enabled: boolean) => Promise<void>>();
const themePreference = ref<"system" | "light" | "dark">("system");
const themeBusy = ref(false);
const themeError = ref("");
const setThemePreference = vi.fn<(value: "system" | "light" | "dark") => Promise<void>>();

vi.mock("../lib/app-update", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/app-update")>();
  return {
    ...actual,
    useAppUpdate: () => ({
      phase,
      info,
      errorMessage,
      progress,
      includePrereleases,
      preferenceBusy,
      preferenceError,
      check,
      install,
      loadUpdatePreferences,
      setIncludePrereleases,
    }),
  };
});

vi.mock("../lib/theme", () => ({
  useTheme: () => ({
    preference: themePreference,
    busy: themeBusy,
    errorMessage: themeError,
    setThemePreference,
  }),
}));

const runtime: RuntimeSnapshot = {
  appVersion: "0.1.0",
  platform: {
    platform: "browser-preview",
    windowsApiAvailable: false,
    bleScanAvailable: false,
    bleVoiceReady: false,
    wasapiReady: false,
    rawInputReady: false,
    sendInputReady: false,
    verificationStatus: "浏览器预览不代表真机通过",
    connection: {
      phase: "idle",
      remoteName: null,
      remoteModel: "unknown",
      capabilities: null,
      voiceState: "idle",
      decodedSamples: 0,
      generation: 0,
      reconnectAttempt: 0,
      powerNotificationsAvailable: false,
      lastError: null,
    },
    audio: {
      phase: "unsupported",
      selectedEndpointId: null,
      selectedEndpointName: null,
      queuedSamples: 0,
      submittedSamples: 0,
      generation: 0,
      lastError: null,
    },
    rawInput: {
      phase: "unsupported",
      matchedDeviceCount: 0,
      rawEventCount: 0,
      semanticEdgeCount: 0,
      lastButton: null,
      lastIsPressed: false,
      activeButtons: [],
      lastError: null,
    },
    buttonMapping: {
      enabled: true,
      gateActive: false,
      listenerActive: false,
      swallowedEdges: 0,
      leakedDowns: 0,
      firedGestures: 0,
      lastFired: null,
      lastError: null,
    },
  },
};

describe("about page update panel", () => {
  beforeEach(() => {
    phase.value = "idle";
    info.value = null;
    errorMessage.value = "";
    progress.value = { downloaded: 0, contentLength: null, finished: false };
    includePrereleases.value = false;
    preferenceBusy.value = false;
    preferenceError.value = "";
    check.mockReset();
    install.mockReset();
    loadUpdatePreferences.mockReset();
    setIncludePrereleases.mockReset();
    themePreference.value = "system";
    themeBusy.value = false;
    themeError.value = "";
    setThemePreference.mockReset();
  });

  it("外观选择器提供系统、浅色、深色三档并立即保存", async () => {
    const wrapper = mount(AboutPage, { props: { runtime } });
    const radios = wrapper.findAll<HTMLInputElement>('input[name="theme-preference"]');

    expect(radios.map((radio) => radio.attributes("value"))).toEqual([
      "system",
      "light",
      "dark",
    ]);
    expect(radios[0].element.checked).toBe(true);
    expect(wrapper.text()).toContain("跟随 Windows 的应用颜色模式");

    await radios[2].setValue(true);
    expect(setThemePreference).toHaveBeenCalledWith("dark");
  });

  it("外观设置失败时显示就地错误", () => {
    themeError.value = "外观设置保存失败，请稍后重试。";
    const wrapper = mount(AboutPage, { props: { runtime } });
    expect(wrapper.get('[role="alert"]').text()).toContain("外观设置保存失败");
  });

  it("本地版显示来源且不调用或提供上游更新", async () => {
    const wrapper = mount(AboutPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.text()).toContain("本地定制版");
    expect(wrapper.text()).toContain("GPL-3.0");
    expect(wrapper.text()).not.toContain("检查更新");
    expect(wrapper.text()).not.toContain("下载并安装");
    expect(check).not.toHaveBeenCalled();
    expect(loadUpdatePreferences).not.toHaveBeenCalled();
    expect(install).not.toHaveBeenCalled();
  });

  it("诊断摘要可在关于页生成并复制完整可见内容", async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>();
    writeText.mockResolvedValue();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    const wrapper = mount(AboutPage, { props: { runtime } });
    const buttons = wrapper.findAll<HTMLButtonElement>(".diagnostics-card button");
    expect(buttons.map((button) => button.text())).toEqual([
      "生成摘要",
      "复制摘要",
      "打开日志目录",
    ]);

    await buttons[0].trigger("click");
    await flushPromises();

    const report = wrapper.get(".diagnostic-output").text();
    expect(report).toContain('"schemaVersion": 1');
    expect(report).not.toContain("remoteName");
    expect(report).not.toContain("selectedEndpointName");
    expect(report).not.toContain("lastError");
    expect(wrapper.text()).toContain("诊断摘要已生成");

    await buttons[1].trigger("click");
    await flushPromises();

    expect(writeText).toHaveBeenCalledOnce();
    expect(writeText).toHaveBeenCalledWith(report);
    expect(wrapper.text()).toContain("诊断摘要已复制到剪贴板");
  });

  it("浏览器预览下打开日志目录给出明确不可用提示而不是静默失败", async () => {
    const wrapper = mount(AboutPage, { props: { runtime } });
    const button = wrapper
      .findAll("button")
      .find((candidate) => candidate.text().includes("打开日志目录"));
    expect(button).toBeDefined();

    await button!.trigger("click");
    await flushPromises();

    expect(wrapper.text()).toContain("当前是浏览器预览，无法打开日志目录");
  });
});
