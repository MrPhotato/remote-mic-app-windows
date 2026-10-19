// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { checkGithubRelease, openGithubRelease, type GithubReleaseCheck } from "./github-release";
import { reportFrontendEvent } from "./frontend-diagnostics";
import { installAppUpdate, subscribeAppUpdateProgress, type AppUpdateProgress } from "./bridge";
import { resetReleaseCheckForTests, useReleaseCheck } from "./release-check-state";

vi.mock("./github-release", () => ({ checkGithubRelease: vi.fn(), openGithubRelease: vi.fn() }));
vi.mock("./frontend-diagnostics", () => ({ reportFrontendEvent: vi.fn() }));
vi.mock("./bridge", () => ({ installAppUpdate: vi.fn(), subscribeAppUpdateProgress: vi.fn() }));

const CACHE_KEY = "sayall.github-release-check.v1";
const available: GithubReleaseCheck = {
  currentVersion: "0.3.5", status: "available", version: "0.3.6", prerelease: true,
  releaseUrl: "https://github.com/MrPhotato/remote-mic-app-windows/releases/tag/v0.3.6",
  notes: "更新说明", publishedAt: "2026-10-19T00:00:00Z",
};

describe("启动检查与手动检查共享状态", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-19T08:00:00Z"));
    localStorage.clear();
    resetReleaseCheckForTests();
    vi.mocked(checkGithubRelease).mockResolvedValue(available);
  });
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

  it("启动只查一次，手动并发不会重复请求，不设置轮询或自动打开发布页", async () => {
    let finish!: (value: GithubReleaseCheck) => void;
    vi.mocked(checkGithubRelease).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const state = useReleaseCheck();
    const pending = state.checkAtStartup("0.3.5");
    await state.checkAtStartup("0.3.5");
    await state.check();
    expect(state.checking.value).toBe(true);
    expect(checkGithubRelease).toHaveBeenCalledOnce();
    finish(available);
    await pending;
    expect(useReleaseCheck().result.value).toEqual(available);
    await vi.advanceTimersByTimeAsync(24 * 60 * 60 * 1000);
    expect(checkGithubRelease).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
    expect(openGithubRelease).not.toHaveBeenCalled();
    expect(reportFrontendEvent).toHaveBeenLastCalledWith(expect.objectContaining({ reason: "startup_available", result: "passed" }));
  });

  it("成功结果在重载后复用六小时，手动检查始终绕过缓存", async () => {
    await useReleaseCheck().checkAtStartup("0.3.5");
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.5");
    expect(checkGithubRelease).toHaveBeenCalledOnce();
    expect(useReleaseCheck().fromCache.value).toBe(true);
    expect(useReleaseCheck().result.value).toEqual(available);
    await useReleaseCheck().check();
    expect(checkGithubRelease).toHaveBeenCalledTimes(2);
    expect(useReleaseCheck().fromCache.value).toBe(false);
    expect(reportFrontendEvent).toHaveBeenLastCalledWith(expect.objectContaining({ reason: "manual_available" }));
  });

  it("成功缓存过期后下次启动会重新检查", async () => {
    await useReleaseCheck().checkAtStartup("0.3.5");
    vi.setSystemTime(Date.now() + 6 * 60 * 60 * 1000);
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.5");
    expect(checkGithubRelease).toHaveBeenCalledTimes(2);
  });

  it("升级后不沿用旧版本判断，时钟回退也重新检查", async () => {
    await useReleaseCheck().checkAtStartup("0.3.5");
    resetReleaseCheckForTests();
    vi.mocked(checkGithubRelease).mockResolvedValue({ ...available, currentVersion: "0.3.6", status: "up_to_date" });
    await useReleaseCheck().checkAtStartup("0.3.6");
    expect(useReleaseCheck().result.value?.currentVersion).toBe("0.3.6");
    vi.setSystemTime(Date.now() - 1000);
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.6");
    expect(checkGithubRelease).toHaveBeenCalledTimes(3);
  });

  it("失败不缓存为最新；十五分钟内重载不重试，用户仍可立即手动检查", async () => {
    vi.mocked(checkGithubRelease).mockRejectedValueOnce(new Error("无法连接 GitHub"));
    await useReleaseCheck().checkAtStartup("0.3.5");
    expect(useReleaseCheck().result.value).toBeNull();
    expect(useReleaseCheck().error.value).toBe("无法连接 GitHub");
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.5");
    expect(checkGithubRelease).toHaveBeenCalledOnce();
    expect(useReleaseCheck().result.value).toBeNull();
    expect(useReleaseCheck().error.value).toContain("上次检查未完成");
    await useReleaseCheck().check();
    expect(useReleaseCheck().error.value).toBe("");
    expect(useReleaseCheck().result.value?.status).toBe("available");
  });

  it("失败冷却到期后下次启动重新检查", async () => {
    vi.mocked(checkGithubRelease).mockRejectedValueOnce("限流");
    await useReleaseCheck().checkAtStartup("0.3.5");
    vi.setSystemTime(Date.now() + 15 * 60 * 1000);
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.5");
    expect(checkGithubRelease).toHaveBeenCalledTimes(2);
  });

  it("请求开始时就写冷却记录，避免未完成便重载造成联网循环", async () => {
    let finish!: (value: GithubReleaseCheck) => void;
    vi.mocked(checkGithubRelease).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const pending = useReleaseCheck().checkAtStartup("0.3.5");
    expect(JSON.parse(localStorage.getItem(CACHE_KEY)!)).toMatchObject({ currentVersion: "0.3.5", result: null, checkedAt: Date.now() });
    finish(available);
    await pending;
  });

  it.each(["broken json", JSON.stringify({ schema: 1, currentVersion: "0.3.5", checkedAt: 0, result: "latest" }), JSON.stringify({ schema: 1, currentVersion: "0.3.5", checkedAt: new Date("2026-10-19T08:00:00Z").getTime(), result: { ...available, releaseUrl: "https://example.com/update" } })])("损坏或来源异常的缓存不影响真实检查", async (cache) => {
    localStorage.setItem(CACHE_KEY, cache);
    await useReleaseCheck().checkAtStartup("0.3.5");
    expect(checkGithubRelease).toHaveBeenCalledOnce();
    expect(useReleaseCheck().result.value).toEqual(available);
  });

  it("存储被禁用仍可检查，日志不含错误正文或发布正文", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("private path"); });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("private path"); });
    await useReleaseCheck().checkAtStartup("0.3.5");
    expect(useReleaseCheck().result.value).toEqual(available);
    const logs = JSON.stringify(vi.mocked(reportFrontendEvent).mock.calls);
    expect(logs).not.toContain("private path");
    expect(logs).not.toContain(available.releaseUrl);
    expect(logs).not.toContain(available.notes);
  });
});

describe("用户发起的一键更新", () => {
  let progress: (value: AppUpdateProgress) => void;
  const unsubscribe = vi.fn();
  beforeEach(async () => {
    resetReleaseCheckForTests();
    vi.resetAllMocks();
    localStorage.clear();
    vi.mocked(checkGithubRelease).mockResolvedValue(available);
    vi.mocked(subscribeAppUpdateProgress).mockImplementation(async (handler) => {
      progress = handler;
      return unsubscribe;
    });
    await useReleaseCheck().check();
  });
  afterEach(() => resetReleaseCheckForTests());

  it("检查本身不下载安装；点击后先订阅事件，安装中不能再次检查或安装", async () => {
    let finish!: () => void;
    let attach!: (unsubscribe: () => void) => void;
    vi.mocked(subscribeAppUpdateProgress).mockReturnValue(new Promise((resolve) => { attach = resolve; }));
    vi.mocked(installAppUpdate).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const state = useReleaseCheck();
    expect(installAppUpdate).not.toHaveBeenCalled();
    const pending = state.install();
    expect(state.installProgress.value?.phase).toBe("preparing");
    expect(state.installing.value).toBe(true);
    await state.install();
    await state.check();
    await state.openRelease();
    expect(checkGithubRelease).toHaveBeenCalledOnce();
    expect(openGithubRelease).not.toHaveBeenCalled();
    expect(installAppUpdate).not.toHaveBeenCalled();
    attach(unsubscribe);
    await Promise.resolve();
    expect(installAppUpdate).toHaveBeenCalledExactlyOnceWith("0.3.6");
    finish();
    await pending;
    expect(state.installing.value).toBe(true);
    expect(state.installProgress.value?.phase).toBe("installing");
    expect(unsubscribe).toHaveBeenCalledOnce();
    await state.install();
    expect(installAppUpdate).toHaveBeenCalledOnce();
  });

  it("下载、校验和安装进度跨页面共享，迟到的下载事件不能退回校验状态", async () => {
    let finish!: () => void;
    vi.mocked(installAppUpdate).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const pending = useReleaseCheck().install();
    await Promise.resolve();
    progress({ phase: "downloading", downloaded: 2048, contentLength: 4096, finished: false });
    expect(useReleaseCheck().installProgress.value?.downloaded).toBe(2048);
    progress({ phase: "downloading", downloaded: 1024, contentLength: 4096, finished: false });
    expect(useReleaseCheck().installProgress.value?.downloaded).toBe(2048);
    progress({ phase: "verifying", downloaded: 4096, contentLength: 4096, finished: true });
    progress({ phase: "downloading", downloaded: 2048, contentLength: 4096, finished: false });
    expect(useReleaseCheck().installProgress.value?.phase).toBe("verifying");
    finish();
    await pending;
  });

  it("下载或校验失败保留发布信息并解除订阅，旧事件无效，下一次点击可重试", async () => {
    vi.mocked(installAppUpdate).mockRejectedValueOnce(new Error("安装包校验失败，请重试。"));
    const state = useReleaseCheck();
    await state.install();
    const staleProgress = progress;
    expect(state.installing.value).toBe(false);
    expect(state.installError.value).toBe("安装包校验失败，请重试。");
    expect(state.result.value).toEqual(available);
    expect(unsubscribe).toHaveBeenCalledOnce();
    await state.openRelease();
    expect(openGithubRelease).toHaveBeenCalledWith(available.releaseUrl);
    expect(state.installError.value).toContain("校验失败");
    let finish!: () => void;
    vi.mocked(installAppUpdate).mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
    const retry = state.install();
    await Promise.resolve();
    staleProgress({ phase: "installing", downloaded: 4000, contentLength: 4000, finished: true });
    expect(state.installProgress.value?.phase).toBe("preparing");
    expect(state.installError.value).toBe("");
    finish();
    await retry;
    expect(installAppUpdate).toHaveBeenCalledTimes(2);
    expect(unsubscribe).toHaveBeenCalledTimes(2);
  });

  it("订阅失败不启动安装，错误日志不记录私有错误正文", async () => {
    vi.mocked(subscribeAppUpdateProgress).mockRejectedValueOnce(new Error("private path"));
    await useReleaseCheck().install();
    expect(installAppUpdate).not.toHaveBeenCalled();
    expect(useReleaseCheck().installing.value).toBe(false);
    expect(useReleaseCheck().installError.value).toBe("private path");
    expect(JSON.stringify(vi.mocked(reportFrontendEvent).mock.calls)).not.toContain("private path");
  });

  it("没有新版本或正在检查时不能更新", async () => {
    vi.mocked(checkGithubRelease).mockResolvedValueOnce({ ...available, status: "up_to_date" });
    await useReleaseCheck().check();
    await useReleaseCheck().install();
    expect(installAppUpdate).not.toHaveBeenCalled();
    let finish!: (value: GithubReleaseCheck) => void;
    vi.mocked(checkGithubRelease).mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
    const pending = useReleaseCheck().check();
    await useReleaseCheck().install();
    expect(subscribeAppUpdateProgress).not.toHaveBeenCalled();
    finish(available);
    await pending;
  });

  it("只在用户请求的目标版本真实启动后显示一次升级摘要", async () => {
    await useReleaseCheck().install();
    expect(useReleaseCheck().completedUpdate.value).toBeNull();
    resetReleaseCheckForTests();
    vi.mocked(checkGithubRelease).mockResolvedValue({ ...available, currentVersion: "0.3.6", status: "up_to_date" });
    await useReleaseCheck().checkAtStartup("0.3.6");
    expect(useReleaseCheck().completedUpdate.value).toEqual({ version: "0.3.6", notes: available.notes });
    expect(reportFrontendEvent).toHaveBeenCalledWith(expect.objectContaining({ reason: "target_version_running", result: "passed" }));
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.6");
    expect(useReleaseCheck().completedUpdate.value).toBeNull();
  });

  it("更新失败或旧版重新打开不误报升级成功", async () => {
    vi.mocked(installAppUpdate).mockRejectedValueOnce(new Error("下载失败"));
    await useReleaseCheck().install();
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.6");
    expect(useReleaseCheck().completedUpdate.value).toBeNull();
    await useReleaseCheck().install();
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.5");
    expect(useReleaseCheck().completedUpdate.value).toBeNull();
  });

  it("损坏的升级记录不影响启动检查", async () => {
    localStorage.setItem("sayall.pending-update.v1", "{broken");
    resetReleaseCheckForTests();
    await useReleaseCheck().checkAtStartup("0.3.6");
    expect(useReleaseCheck().completedUpdate.value).toBeNull();
    expect(checkGithubRelease).toHaveBeenCalledTimes(2);
  });
});
