import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import CodingPage from "./CodingPage.vue";
import { getRuntimeSnapshot, type ButtonMappings } from "../lib/bridge";
import { buildCodingProfile, CODING_BACKUP_KEY, CODING_LATEST_BACKUP_KEY } from "../lib/coding-profile";

const mocks = vi.hoisted(() => ({
  getButtonMappings: vi.fn(), saveButtonMappings: vi.fn(), isTauriRuntime: vi.fn(),
  setVoiceHoldHotkey: vi.fn(), selectAudioEndpoint: vi.fn(), testButtonMapping: vi.fn(),
  reportFrontendEvent: vi.fn(),
}));

vi.mock("../lib/bridge", async (importOriginal) => ({
  ...await importOriginal<typeof import("../lib/bridge")>(),
  getButtonMappings: mocks.getButtonMappings, saveButtonMappings: mocks.saveButtonMappings,
  isTauriRuntime: mocks.isTauriRuntime, setVoiceHoldHotkey: mocks.setVoiceHoldHotkey,
  selectAudioEndpoint: mocks.selectAudioEndpoint, testButtonMapping: mocks.testButtonMapping,
}));
vi.mock("../lib/frontend-diagnostics", () => ({ reportFrontendEvent: mocks.reportFrontendEvent }));

const original: ButtonMappings = { enabled: false, actions: {}, applications: [{ name: "Editor", path: "editor.exe" }] };

describe("Coding page", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.resetAllMocks();
    mocks.isTauriRuntime.mockReturnValue(true);
    mocks.getButtonMappings.mockResolvedValue(original);
    mocks.saveButtonMappings.mockImplementation(async (mappings: ButtonMappings) => mappings);
  });

  it("shows runtime connection/model and audio readiness without claiming speech recognition works", async () => {
    const runtime = await getRuntimeSnapshot();
    runtime.platform.connection.phase = "ready";
    runtime.platform.connection.remoteModel = "rc003";
    runtime.platform.audio.phase = "ready";
    runtime.platform.audio.selectedEndpointId = "test-cable";
    const wrapper = mount(CodingPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.text()).toContain("小米蓝牙遥控器 2 Pro");
    expect(wrapper.text()).toContain("RC003");
    expect(wrapper.find(".rc003-input-control").exists()).toBe(true);
    expect(wrapper.get(".mapping-table").text()).not.toContain("原始行为");
    expect(wrapper.text()).toContain("已连接");
    expect(wrapper.text()).toContain("音频已配置");
    expect(wrapper.text()).toContain("识别文字是否进入 Codex 需要实际试用确认");
    expect(wrapper.get(".voice-guide").text()).toContain("Codex 听写 · Ctrl + Shift + D");
    expect(wrapper.get(".voice-guide").text()).toContain("Codex 听写模式不需要微信输入法");
    expect(wrapper.get(".voice-guide").text()).toContain("VB-CABLE");
    const voice = wrapper.findAll("button").find((button) => button.text() === "配置语音")!;
    await voice.trigger("click");
    expect(wrapper.emitted("navigate")).toEqual([["connection"]]);
    wrapper.unmount();
  });

  it("shows success only after persistence and never changes voice settings or emits test keys", async () => {
    let resolveSave!: (value: ButtonMappings) => void;
    mocks.saveButtonMappings.mockImplementation(() => new Promise<ButtonMappings>((resolve) => { resolveSave = resolve; }));
    const wrapper = mount(CodingPage, { props: { runtime: null } });
    await flushPromises();
    expect(wrapper.text()).toContain("替换下列 12 个键");
    expect(wrapper.get(".mapping-table").text()).toContain("下一任务/标签页");
    expect(wrapper.get(".mapping-table").text()).toContain("上一任务/标签页");
    expect(wrapper.get(".mapping-table").text()).toContain("按住连续切换上一项");
    expect(wrapper.get(".mapping-table").text()).toContain("按住连续切换下一项");
    expect(wrapper.get(".mapping-table").text()).toContain("命令菜单 · Ctrl + Shift + P");
    expect(wrapper.get(".mapping-table").text()).toContain("选择模型 · Ctrl + Shift + M");
    expect(wrapper.get(".mapping-table").text()).toContain("待处理项 · Ctrl + Alt + A");
    expect(wrapper.get(".mapping-table").text()).toContain("开关侧边栏 · Ctrl + B");
    expect(wrapper.get(".mapping-table").text()).toContain("查看改动 · Ctrl + Alt + B");
    expect(wrapper.get(".mapping-table").text()).toContain("打开/收起终端 · Ctrl + `");
    expect(wrapper.get(".mapping-table").text()).toContain("撤销 · Ctrl + Z");
    expect(wrapper.get(".mapping-table").text()).toContain("未配置动作");
    expect(wrapper.get(".mapping-table").text()).not.toContain("系统音量＋（原始行为）");
    expect(wrapper.text()).toContain("单按需等待约 0.3 秒");
    expect(wrapper.text()).toContain("返回首按立即普通退格");
    expect(wrapper.text()).toContain("双击时先删后发送 Ctrl + Z 撤销");
    expect(wrapper.text()).toContain("具体撤销内容由当前编辑器决定");
    expect(wrapper.text()).not.toContain("删除当前短句");
    expect(wrapper.text()).not.toContain("本预设未配置音量动作");
    await wrapper.get('[data-testid="apply-profile"]').trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="status"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="apply-profile"]').attributes("disabled")).toBeDefined();
    resolveSave(mocks.saveButtonMappings.mock.calls[0][0]);
    await flushPromises();
    expect(wrapper.get('[role="status"]').text()).toContain("Codex 预设已保存");
    expect(mocks.setVoiceHoldHotkey).not.toHaveBeenCalled();
    expect(mocks.selectAudioEndpoint).not.toHaveBeenCalled();
    expect(mocks.testButtonMapping).not.toHaveBeenCalled();
    expect(localStorage.getItem(CODING_BACKUP_KEY)).not.toBeNull();
    expect(localStorage.getItem(CODING_LATEST_BACKUP_KEY)).not.toBeNull();
    wrapper.unmount();
  });

  it("displays save errors, retains the backup and never reports success", async () => {
    mocks.saveButtonMappings.mockRejectedValue(new Error("无法保存按键配置"));
    const wrapper = mount(CodingPage, { props: { runtime: null } });
    await flushPromises();
    await wrapper.get('[data-testid="apply-profile"]').trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("无法保存按键配置");
    expect(wrapper.find('[role="status"]').exists()).toBe(false);
    expect(localStorage.getItem(CODING_BACKUP_KEY)).not.toBeNull();
    expect(mocks.reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ event: "coding_profile_apply", result: "failed", reason: "backup_or_settings_failed" }));
    wrapper.unmount();
  });

  it("makes the restoration scope visible and restores through the same backend", async () => {
    const wrapper = mount(CodingPage, { props: { runtime: null } });
    await flushPromises();
    await wrapper.get('[data-testid="apply-profile"]').trigger("click");
    await flushPromises();
    const restore = wrapper.findAll("button").find((button) => button.text() === "恢复最近应用前配置")!;
    await restore.trigger("click");
    expect(wrapper.text()).toContain("包括应用预设后做的修改");
    await wrapper.get('[data-testid="restore-profile"]').trigger("click");
    await flushPromises();
    expect(mocks.saveButtonMappings).toHaveBeenLastCalledWith(original);
    expect(wrapper.get('[role="status"]').text()).toContain("已恢复最近一次应用预设前");
    wrapper.unmount();
  });

  it("keeps an explicit first-backup restore option when a more recent snapshot exists", async () => {
    const latest = buildCodingProfile(original);
    localStorage.setItem(CODING_BACKUP_KEY, JSON.stringify({ version: 1, createdAt: "2026-09-18T08:00:00Z", mappings: original }));
    localStorage.setItem(CODING_LATEST_BACKUP_KEY, JSON.stringify({ version: 1, createdAt: "2026-09-18T09:00:00Z", mappings: latest }));
    const wrapper = mount(CodingPage, { props: { runtime: null } });
    await flushPromises();
    const restore = wrapper.findAll("button").find((button) => button.text() === "恢复最初备份")!;
    await restore.trigger("click");
    expect(wrapper.text()).toContain("将恢复首次应用预设前的配置");
    await wrapper.get('[data-testid="restore-profile"]').trigger("click");
    await flushPromises();
    expect(mocks.saveButtonMappings).toHaveBeenLastCalledWith(original);
    expect(wrapper.get('[role="status"]').text()).toContain("已恢复首次应用预设前");
    expect(localStorage.getItem(CODING_LATEST_BACKUP_KEY)).not.toBeNull();
    wrapper.unmount();
  });

  it("disables persistence in a browser preview", async () => {
    mocks.isTauriRuntime.mockReturnValue(false);
    const wrapper = mount(CodingPage, { props: { runtime: null } });
    await flushPromises();
    expect(wrapper.text()).toContain("浏览器预览：仅展示界面");
    expect(wrapper.get('[data-testid="apply-profile"]').attributes("disabled")).toBeDefined();
    expect(mocks.saveButtonMappings).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("shows an unconnected remote and unconfigured audio as pending", async () => {
    const runtime = await getRuntimeSnapshot();
    runtime.platform.connection.phase = "disconnected";
    runtime.platform.connection.remoteModel = "unknown";
    runtime.platform.audio.phase = "unconfigured";
    runtime.platform.audio.selectedEndpointId = null;
    const wrapper = mount(CodingPage, { props: { runtime } });
    await flushPromises();
    expect(wrapper.text()).toContain("遥控器已断开");
    expect(wrapper.text()).toContain("连接后显示");
    expect(wrapper.text()).toContain("待配置");
    expect(wrapper.get(".readiness-grid").text()).not.toContain("音频已配置");
    wrapper.unmount();
  });

  it("blocks applying a preset when its saved backup is invalid", async () => {
    localStorage.setItem(CODING_BACKUP_KEY, "corrupt backup");
    const wrapper = mount(CodingPage, { props: { runtime: null } });
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("备份格式不正确");
    expect(wrapper.get('[data-testid="apply-profile"]').attributes("disabled")).toBeDefined();
    expect(mocks.saveButtonMappings).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
