import { onBeforeUnmount, watch, type Ref, type WatchSource } from "vue";

/**
 * 分段控件与侧栏的"滑动选中玻璃"：把选中项的位置和尺寸写到容器的 CSS 变量
 * （--indicator-x/y/w/h），由容器的 ::before 画出选中块并用 transform 滑过去
 * （样式见 styles.css 的 .sliding-indicator）。
 *
 * 首次就位不带过渡：先写 data-indicator="placed"，两帧后才切到 "animate"，
 * 否则选中块会从容器原点飞入。容器出现/消失（如编辑面板里的页签）与
 * 尺寸变化（字体加载、窗口缩放）都会重新测量。
 */
export function useSlidingIndicator(
  container: Ref<HTMLElement | null>,
  activeSelector: string,
  active: WatchSource<unknown>,
): void {
  let observer: ResizeObserver | null = null;
  let cancelFrame: (() => void) | null = null;

  function nextFrame(callback: () => void): void {
    if (typeof requestAnimationFrame === "function") {
      const id = requestAnimationFrame(callback);
      cancelFrame = () => cancelAnimationFrame(id);
    } else {
      const id = setTimeout(callback, 16);
      cancelFrame = () => clearTimeout(id);
    }
  }

  function measure(): void {
    const el = container.value;
    if (!el) return;
    const item = el.querySelector<HTMLElement>(activeSelector);
    if (!item) {
      delete el.dataset.indicator;
      return;
    }
    el.style.setProperty("--indicator-x", `${item.offsetLeft}px`);
    el.style.setProperty("--indicator-y", `${item.offsetTop}px`);
    el.style.setProperty("--indicator-w", `${item.offsetWidth}px`);
    el.style.setProperty("--indicator-h", `${item.offsetHeight}px`);
    if (el.dataset.indicator) return;
    el.dataset.indicator = "placed";
    cancelFrame?.();
    nextFrame(() => nextFrame(() => {
      cancelFrame = null;
      if (container.value === el && el.dataset.indicator === "placed") el.dataset.indicator = "animate";
    }));
  }

  function observe(el: HTMLElement | null): void {
    observer?.disconnect();
    observer = null;
    if (!el || typeof ResizeObserver === "undefined") return;
    observer = new ResizeObserver(() => measure());
    observer.observe(el);
    for (const child of Array.from(el.children)) observer.observe(child);
  }

  watch(container, (el) => {
    observe(el);
    measure();
  }, { flush: "post", immediate: true });
  watch(active, () => measure(), { flush: "post" });

  onBeforeUnmount(() => {
    observer?.disconnect();
    observer = null;
    cancelFrame?.();
    cancelFrame = null;
  });
}
