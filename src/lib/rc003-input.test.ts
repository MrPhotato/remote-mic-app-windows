import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./bridge";
import { getRc003InputStatus, rc003InputAvailable, rc003InputErrorMessage, startRc003Input, stopRc003Input, stoppedRc003InputStatus } from "./rc003-input";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./bridge", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("./frontend-diagnostics", () => ({ reportFrontendEvent: vi.fn() }));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(isTauriRuntime).mockReturnValue(true);
  vi.stubEnv("VITE_SAYALL_RUNTIME_SIMULATION", "0");
});
afterEach(() => vi.unstubAllEnvs());

describe("RC003 input commands", () => {
  it.each(["browser", "simulation"])("never invokes the helper in %s mode", async (mode) => {
    vi.mocked(isTauriRuntime).mockReturnValue(mode !== "browser");
    if (mode === "simulation") vi.stubEnv("VITE_SAYALL_RUNTIME_SIMULATION", "1");
    expect(rc003InputAvailable()).toBe(false);
    expect(await getRc003InputStatus()).toEqual(stoppedRc003InputStatus());
    await expect(startRc003Input()).rejects.toThrow("preview_unavailable");
    await expect(stopRc003Input()).rejects.toThrow("preview_unavailable");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("coalesces start clicks across views and blocks conflicting commands until completion", async () => {
    let resolve!: (value: ReturnType<typeof stoppedRc003InputStatus>) => void;
    vi.mocked(invoke).mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const first = startRc003Input();
    const second = startRc003Input();
    expect(first).toBe(second);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("start_rc003_input");
    await expect(stopRc003Input()).rejects.toThrow("operation_pending");
    resolve({ ...stoppedRc003InputStatus(), phase: "waiting", generation: 1 });
    expect((await first).phase).toBe("waiting");
    vi.mocked(invoke).mockResolvedValueOnce(stoppedRc003InputStatus());
    await stopRc003Input();
    expect(invoke).toHaveBeenLastCalledWith("stop_rc003_input");
  });

  it("allows an explicit retry after a rejected command", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("elevation_cancelled"));
    await expect(startRc003Input()).rejects.toThrow("elevation_cancelled");
    vi.mocked(invoke).mockResolvedValueOnce(stoppedRc003InputStatus());
    await startRc003Input();
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("shows actionable errors without exposing helper paths or device details", () => {
    expect(rc003InputErrorMessage("elevation_cancelled: private-helper-path")).toContain("管理员授权");
    expect(rc003InputErrorMessage("helper_not_found: private-helper-path")).toContain("组件不完整");
    expect(rc003InputErrorMessage("private-helper-path")).not.toContain("private-helper-path");
    expect(rc003InputErrorMessage("helper_heartbeat_timeout")).toContain("响应超时");
    expect(rc003InputErrorMessage("helper_exited")).toContain("已退出");
    expect(rc003InputErrorMessage("ipc_authentication_failed")).toContain("安全校验未通过");
  });

  it.each([
    "请先连接 RC003 遥控器并启动按键监听。",
    new Error("请先连接 RC003 遥控器并启动按键监听。"),
  ])("preserves the connection and listener prerequisites for %s", (cause) => {
    expect(rc003InputErrorMessage(cause)).toBe(
      "请先在「连接与语音」中连接 RC003，并在「按键映射」中启动按键监听，然后再启用三键增强。",
    );
  });
});
