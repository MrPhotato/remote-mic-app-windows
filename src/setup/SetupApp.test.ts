// @vitest-environment jsdom
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import SetupApp from "./SetupApp.vue";

const api = vi.hoisted(() => ({ info: vi.fn(), browse: vi.fn(), install: vi.fn(), finish: vi.fn() }));
const native = vi.hoisted(() => ({ minimize: vi.fn(), close: vi.fn() }));
vi.mock("./api", () => ({ setupApi: api, preview: false }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native }));
let wrapper: VueWrapper | undefined;

beforeEach(() => {
  vi.resetAllMocks();
  api.info.mockResolvedValue({ version: "0.3.12", directory: "C:\\Apps\\SayAll", existing: false });
  api.browse.mockResolvedValue(null);
  api.install.mockResolvedValue({ version: "0.3.12", directory: "D:\\软件\\无线麦" });
  api.finish.mockResolvedValue(undefined);
});
afterEach(() => { wrapper?.unmount(); wrapper = undefined; });

async function start(): Promise<VueWrapper> {
  wrapper = mount(SetupApp);
  await flushPromises();
  return wrapper;
}
function button(label: string) {
  const match = wrapper!.findAll("button").find(element => element.text() === label);
  if (!match) throw new Error(`Missing button: ${label}`);
  return match;
}

describe("安装界面", () => {
  it("保留浏览取消前的路径，选择路径和桌面选项原样交给安装", async () => {
    await start();
    await wrapper!.get("#setup-directory").setValue("D:\\软件\\无线麦");
    await button("浏览…").trigger("click"); await flushPromises();
    expect(api.browse).toHaveBeenLastCalledWith("D:\\软件\\无线麦");
    expect((wrapper!.get("#setup-directory").element as HTMLInputElement).value).toBe("D:\\软件\\无线麦");
    api.browse.mockResolvedValueOnce("E:\\Applications\\SayAll");
    await button("浏览…").trigger("click"); await flushPromises();
    await wrapper!.get('[data-testid="desktop-option"]').setValue(false);
    await button("开始安装").trigger("click"); await flushPromises();
    expect(api.install).toHaveBeenCalledWith("E:\\Applications\\SayAll", false);
    expect(wrapper!.text()).toContain("安装完成");
  });

  it("浏览失败保留输入并允许手动选择路径", async () => {
    api.browse.mockRejectedValueOnce(new Error("dialog failed"));
    await start();
    await button("浏览…").trigger("click"); await flushPromises();
    expect(wrapper!.get('[role="alert"]').text()).toContain("也可以直接填写安装位置");
    expect(wrapper!.get("#setup-directory").attributes("disabled")).toBeUndefined();
    await wrapper!.get("#setup-directory").setValue("D:\\SayAll");
    await button("开始安装").trigger("click"); await flushPromises();
    expect(api.install).toHaveBeenCalledWith("D:\\SayAll", true);
  });

  it("安装进行中不重复提交、不允许关闭且不捏造百分比", async () => {
    let resolve!: (value: { version: string; directory: string }) => void;
    api.install.mockReturnValue(new Promise(r => { resolve = r; }));
    await start();
    const install = button("开始安装");
    await install.trigger("click");
    await install.trigger("click");
    expect(api.install).toHaveBeenCalledOnce();
    expect(wrapper!.get('[aria-label="关闭窗口"]').attributes("disabled")).toBeDefined();
    await wrapper!.get('[aria-label="关闭窗口"]').trigger("click");
    expect(native.close).not.toHaveBeenCalled();
    expect(wrapper!.get('[role="progressbar"]').attributes("aria-valuenow")).toBeUndefined();
    expect(wrapper!.text()).not.toContain("%");
    resolve({ version: "0.3.12", directory: "C:\\Apps\\SayAll" }); await flushPromises();
    expect(wrapper!.get('[aria-label="关闭窗口"]').attributes("disabled")).toBeUndefined();
  });

  it("失败后保留路径与选项，重试成功后才显示完成", async () => {
    api.install.mockRejectedValueOnce("所选位置无法写入，请选择其他文件夹。");
    await start();
    await wrapper!.get("#setup-directory").setValue("D:\\软件\\无线麦");
    await wrapper!.get('[data-testid="desktop-option"]').setValue(false);
    await button("开始安装").trigger("click"); await flushPromises();
    expect(wrapper!.text()).not.toContain("安装完成");
    expect(wrapper!.get('[role="alert"]').text()).toContain("所选位置无法写入");
    expect((wrapper!.get("#setup-directory").element as HTMLInputElement).value).toBe("D:\\软件\\无线麦");
    expect((wrapper!.get('[data-testid="desktop-option"]').element as HTMLInputElement).checked).toBe(false);
    await button("开始安装").trigger("click"); await flushPromises();
    expect(api.install).toHaveBeenCalledTimes(2);
    expect(wrapper!.text()).toContain("安装完成");
  });

  it("升级锁定原路径，并保留快捷方式选项", async () => {
    api.info.mockResolvedValueOnce({ version: "0.3.12", directory: "E:\\原安装目录\\无线麦", existing: true });
    await start();
    expect(wrapper!.get("#setup-directory").attributes("readonly")).toBeDefined();
    expect(wrapper!.text()).toContain("更换位置需卸载后重装");
    expect(wrapper!.findAll("button").some(el => el.text() === "浏览…")).toBe(false);
    await button("开始安装").trigger("click"); await flushPromises();
    expect(api.install).toHaveBeenCalledWith("E:\\原安装目录\\无线麦", true);
  });

  it("空路径不提交，安装信息读取失败可重试", async () => {
    api.info.mockRejectedValueOnce(new Error("info failed"));
    await start();
    expect(wrapper!.text()).toContain("无法读取安装信息");
    await button("重试").trigger("click"); await flushPromises();
    await wrapper!.get("#setup-directory").setValue("  ");
    expect(button("开始安装").attributes("disabled")).toBeDefined();
    await button("开始安装").trigger("click");
    expect(api.install).not.toHaveBeenCalled();
  });

  it("默认勾选打开，取消后完成不会启动应用；失败仍可重试", async () => {
    await start();
    await button("开始安装").trigger("click"); await flushPromises();
    expect((wrapper!.get('[data-testid="launch-option"]').element as HTMLInputElement).checked).toBe(true);
    await wrapper!.get('[data-testid="launch-option"]').setValue(false);
    api.finish.mockRejectedValueOnce("操作未完成，请重试。");
    await button("完成").trigger("click"); await flushPromises();
    expect(api.finish).toHaveBeenLastCalledWith(false);
    expect(wrapper!.get('[role="alert"]').text()).toContain("请重试");
    await button("完成").trigger("click"); await flushPromises();
    expect(api.finish).toHaveBeenCalledTimes(2);
  });
});
