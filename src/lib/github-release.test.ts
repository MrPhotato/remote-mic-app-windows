import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isTauriRuntime } from "./bridge";
import { reportFrontendEvent } from "./frontend-diagnostics";
import { checkGithubRelease, openGithubRelease } from "./github-release";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("./bridge", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("./frontend-diagnostics", () => ({ reportFrontendEvent: vi.fn() }));

const releaseUrl = "https://github.com/MrPhotato/remote-mic-app-windows/releases/tag/v0.3.6";

describe("GitHub release bridge", () => {
  beforeEach(() => { vi.resetAllMocks(); vi.mocked(isTauriRuntime).mockReturnValue(true); });
  it("调用独立的只读检查命令", async () => {
    await checkGithubRelease();
    expect(invoke).toHaveBeenCalledExactlyOnceWith("check_github_release");
  });
  it("浏览器预览不伪造真实检查结果", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    await expect(checkGithubRelease()).rejects.toThrow("请在无线麦应用中检查更新");
    await expect(openGithubRelease(releaseUrl)).rejects.toThrow("请在无线麦应用中打开发布页");
    expect(invoke).not.toHaveBeenCalled();
    expect(openUrl).not.toHaveBeenCalled();
  });
  it.each([
    releaseUrl.replace("MrPhotato", "GetSayAll"),
    releaseUrl.replace("https:", "http:"),
    releaseUrl.replace("github.com", "github.com.evil.example"),
    releaseUrl + "?redirect=evil",
    releaseUrl + "/../../other",
    "file:///C:/Windows/System32/cmd.exe",
    "https://github.com/MrPhotato/remote-mic-app-windows/releases/tag/../latest",
  ])("拒绝非本仓库合法版本地址 %s", async (url) => {
    await expect(openGithubRelease(url)).rejects.toThrow("发布页地址无效");
    expect(openUrl).not.toHaveBeenCalled();
  });
  it.each([releaseUrl, releaseUrl + "-rc.1", releaseUrl + "+build.1"])("只打开验证过的发布页 %s", async (url) => {
    await openGithubRelease(url);
    expect(openUrl).toHaveBeenCalledExactlyOnceWith(url);
    expect(reportFrontendEvent).toHaveBeenLastCalledWith(expect.objectContaining({ result: "passed", reason: "open_url_accepted" }));
  });
  it("打开失败留下无内容的结构化日志", async () => {
    vi.mocked(openUrl).mockRejectedValue(new Error("failure"));
    await expect(openGithubRelease(releaseUrl)).rejects.toThrow("failure");
    expect(reportFrontendEvent).toHaveBeenLastCalledWith(expect.objectContaining({ result: "failed", reason: "open_url_failed" }));
    expect(JSON.stringify(vi.mocked(reportFrontendEvent).mock.calls)).not.toContain(releaseUrl);
  });
});
