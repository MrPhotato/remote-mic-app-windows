// @vitest-environment jsdom
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ReleaseUpdateCard from "./ReleaseUpdateCard.vue";
import { checkGithubRelease, openGithubRelease, type GithubReleaseCheck } from "../lib/github-release";
import { useReleaseCheck } from "../lib/release-check-state";

vi.mock("../lib/github-release", () => ({ checkGithubRelease: vi.fn(), openGithubRelease: vi.fn() }));

const available: GithubReleaseCheck = {
  currentVersion: "0.3.5", status: "available", version: "0.3.6", prerelease: true,
  releaseUrl: "https://github.com/MrPhotato/remote-mic-app-windows/releases/tag/v0.3.6",
  notes: "修复说明\n<script>alert('untrusted')</script>", publishedAt: "2026-10-19T00:00:00Z",
};

describe("手动检查 Release", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    const state = useReleaseCheck();
    state.checking.value = false;
    state.opening.value = false;
    state.result.value = null;
    state.error.value = "";
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
    await wrapper.get(".primary-button").trigger("click");
    expect(openGithubRelease).toHaveBeenCalledWith(available.releaseUrl);
    expect(wrapper.text()).not.toContain("下载并安装");
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

  it("打开发布页失败会明确提示且可重试", async () => {
    vi.mocked(checkGithubRelease).mockResolvedValue(available);
    vi.mocked(openGithubRelease).mockRejectedValueOnce(new Error("opener failure")).mockResolvedValueOnce();
    const wrapper = mount(ReleaseUpdateCard);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    await wrapper.get(".primary-button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("无法打开发布页");
    await wrapper.get(".primary-button").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(openGithubRelease).toHaveBeenCalledTimes(2);
  });
});
