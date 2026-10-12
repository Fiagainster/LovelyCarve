// 节点图画布的平移/缩放状态机：CSS transform 方案——
// viewport = translate(panX,panY) scale(zoom)，世界坐标 → 屏幕：screen = pan + world*zoom。
// 滚轮以光标为锚缩放，拖拽平移，fit 把整图居中铺满视口。纯 DOM/指针数学，不碰 Tauri。
import { ref } from 'vue';

export interface Size {
  width: number;
  height: number;
}

const MIN_ZOOM = 0.1;
const MAX_ZOOM = 3;

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

export function useGraphPanZoom() {
  const panX = ref(0);
  const panY = ref(0);
  const zoom = ref(1);
  const isPanning = ref(false);

  let dragging = false;
  let lastX = 0;
  let lastY = 0;

  // 以某屏幕点为锚把缩放改到 newZoom，保持该点下的世界坐标不动。
  function zoomAround(sx: number, sy: number, newZoom: number): void {
    const nz = clamp(newZoom, MIN_ZOOM, MAX_ZOOM);
    const worldX = (sx - panX.value) / zoom.value;
    const worldY = (sy - panY.value) / zoom.value;
    panX.value = sx - worldX * nz;
    panY.value = sy - worldY * nz;
    zoom.value = nz;
  }

  function onWheel(e: WheelEvent): void {
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
    zoomAround(e.clientX - rect.left, e.clientY - rect.top, zoom.value * factor);
  }

  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    dragging = true;
    isPanning.value = true;
    lastX = e.clientX;
    lastY = e.clientY;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function onPointerMove(e: PointerEvent): void {
    if (!dragging) return;
    panX.value += e.clientX - lastX;
    panY.value += e.clientY - lastY;
    lastX = e.clientX;
    lastY = e.clientY;
  }

  function onPointerUp(e: PointerEvent): void {
    if (!dragging) return;
    dragging = false;
    isPanning.value = false;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
  }

  // 整图居中铺满视口（留一点边距）。
  function fit(content: Size, viewport: Size): void {
    if (content.width <= 0 || content.height <= 0 || viewport.width <= 0 || viewport.height <= 0) {
      return;
    }
    const z = clamp(
      Math.min(viewport.width / content.width, viewport.height / content.height) * 0.94,
      MIN_ZOOM,
      MAX_ZOOM,
    );
    zoom.value = z;
    panX.value = (viewport.width - content.width * z) / 2;
    panY.value = (viewport.height - content.height * z) / 2;
  }

  // 工具栏按钮：以视口中心为锚缩放。
  function zoomByButton(factor: number, viewport: Size): void {
    zoomAround(viewport.width / 2, viewport.height / 2, zoom.value * factor);
  }

  function reset(): void {
    zoom.value = 1;
    panX.value = 0;
    panY.value = 0;
  }

  return {
    panX,
    panY,
    zoom,
    isPanning,
    onWheel,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    fit,
    zoomByButton,
    reset,
  };
}
