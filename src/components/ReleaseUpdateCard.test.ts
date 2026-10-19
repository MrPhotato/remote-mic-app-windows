// @vitest-environment jsdom
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ReleaseUpdateCard from "./ReleaseUpdateCard.vue";
import { checkGithubRelease, openGithubRelease, type GithubReleaseCheck } from "../lib/github-release";
import { resetReleaseCheckForTests, useReleaseCheck } from "../lib/release-check-state";
import { installAppUpdate, subscribeAppUpdateProgress, type AppUpdateProgress } from "../lib/bridge";

vi.mock("../lib/github-release", () => ({ checkGithubRelease: vi.fn(), openGithubRelease: vi.fn() }));
vi.mock("../lib/frontend-diagnostics", () => ({ reportFrontendEvent: vi.fn() }));
vi.mock("../lib/bridge", () => ({ installAppUpdate: vi.fn(), subscribeAppUpdateProgress: vi.fn() }));

const available: GithubReleaseCheck = {
  currentVersion: "0.3.5", status: "available", version: "0.3.6", prerelease: true,
  releaseUrl: "https://github.com/MrPhotato/remote-mic-app-windows/releases/tag/v0.3.6",
  notes: "修复说明\n<script>alert('untrusted')</script>", publishedAt: "2026-10-19T00:00:00Z",
};

describe("Release 更新卡片", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    localStorage.clear();
    resetReleaseCheckForTests();
    vi.mocked(subscribeAppUpdateProgress).mockResolvedValue(() => {});
  });

  it("检查期间切页回来复用请求，完成后再次进来也保留结果", async () => {
    let finish!: (value: GithubReleaseCheck) => void;
    vi.mocked(checkGithubRelease).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const first = mount(ReleaseUpdateCard);
    await first.get("button").trigger("click");
    first.unmount();
    const second = mount(ReleaseUpdateCard);
    expect(second.text()).toContain("检查中");
    await second.get("button").trigger("click");
    expect(checkGithubRelease).toHaveBeenCalledOnce();
    finish(available);
    await flushPromises();
    expect(second.text()).toContain("发现新版本");
    second.unmount();
    const third = mount(ReleaseUpdateCard);
    expect(third.text()).toContain("发现新版本 0.3.6");
    expect(checkGithubRelease).toHaveBeenCalledOnce();
  });

  it("打开页面不联网；重复点击不重复查询，结束后才能重新检查", async () => {
    let finish!: (value: GithubReleaseCheck) => void;
    vi.mocked(checkGithubRelease).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const wrapper = mount(ReleaseUpdateCard);
    await flushPromises();
    expect(checkGithubRelease).not.toHaveBeenCalled();
    const button = wrapper.get("button");
    await button.trigger("click");
    await button.trigger("click");
    expect(checkGithubRelease).toHaveBeenCalledOnce();
    expect(button.attributes("disabled")).toBeDefined();
    expect(wrapper.get('[role="status"]').text()).toContain("正在查询");
    finish(available);
    await flushPromises();
    expect(button.attributes("disabled")).toBeUndefined();
  });

  it("显示预览版、当前版本和纯文本说明，用户点击后才打开指定发布页", async () => {
    vi.mocked(checkGithubRelease).mockResolvedValue(available);
    const wrapper = mount(ReleaseUpdateCard);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("发现新版本 0.3.6（预览版）");
    expect(wrapper.text()).toContain("当前 0.3.5");
    expect(wrapper.get("pre").text()).toBe(available.notes);
    expect(wrapper.find("script").exists()).toBe(false);
    expect(openGithubRelease).not.toHaveBeenCalled();
    expect(wrapper.get(".primary-button").text()).toBe("更新并重启");
    expect(installAppUpdate).not.toHaveBeenCalled();
    await wrapper.get(".release-page-button").trigger("click");
    expect(openGithubRelease).toHaveBeenCalledWith(available.releaseUrl);
  });

  it.each([
    ["up_to_date", "当前已是最新版本（0.3.5）"],
    ["no_release", "暂时没有已发布的版本。"],
  ] as const)("%s 不提供下载按钮", async (status, message) => {
    vi.mocked(checkGithubRelease).mockResolvedValue({ ...available, status });
    const wrapper = mount(ReleaseUpdateCard);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain(message);
    expect(wrapper.find(".primary-button").exists()).toBe(false);
  });

  it("网络错误不会显示已是最新，重试成功后清除旧错误", async () => {
    vi.mocked(checkGithubRelease).mockRejectedValueOnce("无法连接 GitHub，请检查网络后重试。").mockResolvedValueOnce(available);
    const wrapper = mount(ReleaseUpdateCard);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("无法连接 GitHub");
    expect(wrapper.text()).not.toContain("已是最新");
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.text()).toContain("发现新版本");
  });

  it("重新检查失败时不会留下旧下载结果", async () => {
    vi.mocked(checkGithubRelease).mockResolvedValueOnce(available).mockRejectedValueOnce(new Error("GitHub 暂时不可用"));
    const wrapper = mount(ReleaseUpdateCard);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    await wrapper.get(".secondary-button").trigger("click");
    await flushPromises();
    expect(wrapper.find(".primary-button").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("发现新版本");
    expect(wrapper.get('[role="alert"]').text()).toBe("GitHub 暂时不可用");
  });

  it("重新检查时保留上次时间和更新说明，但不能打开旧下载结果", async () => {
    let finish!: (value: GithubReleaseCheck) => void;
    vi.mocked(checkGithubRelease).mockResolvedValueOnce(available)
      .mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
    const wrapper = mount(ReleaseUpdateCard);
    await wrapper.get(".secondary-button").trigger("click");
    await flushPromises();
    const time = wrapper.get(".checked-time").text();
    await wrapper.get(".secondary-button").trigger("click");
    expect(wrapper.get('[role="status"]').text()).toContain("正在查询");
    expect(wrapper.get(".checked-time").text()).toBe(time);
    expect(wrapper.get("pre").text()).toBe(available.notes);
    expect(wrapper.get(".primary-button").attributes("disabled")).toBeDefined();
    await wrapper.get(".release-page-button").trigger("click");
    expect(openGithubRelease).not.toHaveBeenCalled();
    finish({ ...available, status: "up_to_date" });
    await flushPromises();
    expect(wrapper.find(".primary-button").exists()).toBe(false);
  });

  it("打开发布页失败会明确提示且可重试", async () => {
    vi.mocked(checkGithubRelease).mockResolvedValue(available);
    vi.mocked(openGithubRelease).mockRejectedValueOnce(new Error("opener failure")).mockResolvedValueOnce();
    const wrapper = mount(ReleaseUpdateCard);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    await wrapper.get(".release-page-button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("无法打开发布页");
    await wrapper.get(".release-page-button").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(openGithubRelease).toHaveBeenCalledTimes(2);
  });

  it("展示启动检查结果，切到关于页不会重复请求或自动打开下载", async () => {
    vi.mocked(checkGithubRelease).mockResolvedValue(available);
    await useReleaseCheck().checkAtStartup("0.3.5");
    const wrapper = mount(ReleaseUpdateCard);
    expect(wrapper.text()).toContain("启动时检查新版本");
    expect(wrapper.text()).toContain("发现新版本 0.3.6");
    expect(wrapper.text()).toContain("上次检查：");
    expect(openGithubRelease).not.toHaveBeenCalled();
    expect(checkGithubRelease).toHaveBeenCalledOnce();
  });

  it("缓存的最新结果明确标注为上次检查，不冒充本次实时检查", async () => {
    vi.mocked(checkGithubRelease).mockResolvedValue({ ...available, status: "up_to_date" });
    await useReleaseCheck().checkAtStartup("0.3.5");
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.5");
    const wrapper = mount(ReleaseUpdateCard);
    expect(wrapper.text()).toContain("上次检查时已是最新版本");
    expect(wrapper.text()).not.toContain("当前已是最新版本");
    expect(checkGithubRelease).toHaveBeenCalledOnce();
  });

  it("点击更新后在原按钮显示进度，切页不丢失，也不把下载完成说成安装完成", async () => {
    let progress!: (value: AppUpdateProgress) => void;
    let finish!: () => void;
    vi.mocked(checkGithubRelease).mockResolvedValue(available);
    vi.mocked(subscribeAppUpdateProgress).mockImplementation(async (handler) => { progress = handler; return () => {}; });
    vi.mocked(installAppUpdate).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const first = mount(ReleaseUpdateCard);
    await first.get(".secondary-button").trigger("click");
    await flushPromises();
    await first.get(".primary-button").trigger("click");
    await flushPromises();
    expect(first.get(".primary-button").text()).toBe("正在准备…");
    progress({ phase: "downloading", downloaded: 1048576, contentLength: 4194304, finished: false });
    await flushPromises();
    expect(first.get(".primary-button").text()).toBe("正在下载 25%");
    expect(first.get('[role="status"]').text()).toContain("1.0 / 4.0 MB");
    first.unmount();
    const second = mount(ReleaseUpdateCard);
    expect(second.get(".primary-button").text()).toBe("正在下载 25%");
    expect(second.get(".secondary-button").attributes("disabled")).toBeDefined();
    expect(second.get(".release-page-button").attributes("disabled")).toBeDefined();
    progress({ phase: "verifying", downloaded: 4194304, contentLength: 4194304, finished: true });
    await flushPromises();
    expect(second.get(".primary-button").text()).toBe("正在校验…");
    expect(second.text()).not.toContain("安装完成");
    finish();
    await flushPromises();
    expect(second.get(".primary-button").text()).toBe("正在安装…");
    expect(second.get('[role="status"]').text()).toBe("正在安装，即将重新打开无线麦…");
    expect(installAppUpdate).toHaveBeenCalledExactlyOnceWith("0.3.6");
  });

  it("更新失败保留说明、发布页和重试按钮，重试立即清除错误", async () => {
    vi.mocked(checkGithubRelease).mockResolvedValue(available);
    vi.mocked(installAppUpdate).mockRejectedValueOnce(new Error("下载失败，请稍后重试。"));
    const wrapper = mount(ReleaseUpdateCard);
    await wrapper.get(".secondary-button").trigger("click");
    await flushPromises();
    await wrapper.get(".primary-button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe("下载失败，请稍后重试。");
    expect(wrapper.get(".primary-button").text()).toBe("重试更新");
    expect(wrapper.get(".primary-button").attributes("disabled")).toBeUndefined();
    expect(wrapper.get(".release-page-button").attributes("disabled")).toBeUndefined();
    expect(wrapper.get("pre").text()).toBe(available.notes);
    await wrapper.get(".primary-button").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(installAppUpdate).toHaveBeenCalledTimes(2);
  });

  it("升级后的摘要保持纯文本，可关闭；不会自动展开弹窗或触发安装", async () => {
    localStorage.setItem("sayall.pending-update.v1", JSON.stringify({ fromVersion: "0.3.5", version: "0.3.6", notes: available.notes }));
    vi.mocked(checkGithubRelease).mockResolvedValue({ ...available, currentVersion: "0.3.6", status: "up_to_date" });
    await useReleaseCheck().checkAtStartup("0.3.6");
    const wrapper = mount(ReleaseUpdateCard);
    expect(wrapper.text()).toContain("已更新至 0.3.6");
    expect(wrapper.get(".completed-update pre").text()).toBe(available.notes);
    expect(wrapper.find("script").exists()).toBe(false);
    expect(wrapper.find("dialog").exists()).toBe(false);
    expect(installAppUpdate).not.toHaveBeenCalled();
    await wrapper.get(".completed-update button").trigger("click");
    expect(wrapper.find(".completed-update").exists()).toBe(false);
  });
});
