import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, nextTick, ref } from "vue";
import { mount } from "@vue/test-utils";
import { useSlidingIndicator } from "./sliding-indicator";

// jsdom 不做布局：用 data-x/w 模拟每个选项的几何位置。
const offsetProps = {
  offsetLeft: "x",
  offsetTop: "y",
  offsetWidth: "w",
  offsetHeight: "h",
} as const;

const Segments = defineComponent({
  props: { active: { type: String, required: true }, show: { type: Boolean, default: true } },
  setup(props) {
    const container = ref<HTMLElement | null>(null);
    useSlidingIndicator(container, ".selected", () => props.active);
    return () => props.show
      ? h("div", { ref: container, class: "sliding-indicator" }, ["a", "b"].map((id, index) =>
        h("button", {
          class: { selected: props.active === id },
          "data-x": String(3 + index * 80),
          "data-y": "3",
          "data-w": String(70 + index * 10),
          "data-h": "30",
        }, id)))
      : h("p", "empty");
  },
});

function vars(el: HTMLElement) {
  return ["x", "y", "w", "h"].map((key) => el.style.getPropertyValue(`--indicator-${key}`));
}

describe("useSlidingIndicator", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 16));
    vi.stubGlobal("cancelAnimationFrame", (id: number) => clearTimeout(id));
    for (const [prop, key] of Object.entries(offsetProps)) {
      Object.defineProperty(HTMLElement.prototype, prop, {
        configurable: true,
        get(this: HTMLElement) { return Number(this.dataset[key] ?? 0); },
      });
    }
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    for (const prop of Object.keys(offsetProps)) delete (HTMLElement.prototype as unknown as Record<string, unknown>)[prop];
  });

  it("places the indicator on the selected item without animating the first placement", async () => {
    const wrapper = mount(Segments, { props: { active: "a" } });
    await nextTick();
    const el = wrapper.get(".sliding-indicator").element as HTMLElement;
    expect(vars(el)).toEqual(["3px", "3px", "70px", "30px"]);
    expect(el.dataset.indicator).toBe("placed");
    vi.advanceTimersByTime(40);
    expect(el.dataset.indicator).toBe("animate");
    wrapper.unmount();
  });

  it("follows the selection after the DOM updates", async () => {
    const wrapper = mount(Segments, { props: { active: "a" } });
    await nextTick();
    vi.advanceTimersByTime(40);
    await wrapper.setProps({ active: "b" });
    const el = wrapper.get(".sliding-indicator").element as HTMLElement;
    expect(vars(el)).toEqual(["83px", "3px", "80px", "30px"]);
    expect(el.dataset.indicator).toBe("animate");
    wrapper.unmount();
  });

  it("re-places without animation when the container is recreated", async () => {
    const wrapper = mount(Segments, { props: { active: "b", show: false } });
    await nextTick();
    await wrapper.setProps({ show: true });
    const el = wrapper.get(".sliding-indicator").element as HTMLElement;
    expect(vars(el)).toEqual(["83px", "3px", "80px", "30px"]);
    expect(el.dataset.indicator).toBe("placed");
    wrapper.unmount();
    vi.advanceTimersByTime(40);
    expect(el.dataset.indicator).toBe("placed");
  });
});
