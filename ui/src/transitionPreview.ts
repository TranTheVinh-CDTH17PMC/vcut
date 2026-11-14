// Mô phỏng chuyển cảnh bằng CSS để xem trước trong app (gần đúng, bản xuất dùng FFmpeg).
// Dùng chung cho khung xem trước và ô xem thử trong bảng chọn chuyển cảnh.
//
// p: tiến độ 0 → 1. Trả về style cho clip ra (out) và clip vào (in).

import type { CSSProperties } from "react";

export interface TrStyle {
  transform?: string;
  opacity?: number;
  filter?: string;
  clipPath?: string;
  maskImage?: string;
  /** Clip ra nằm trên clip vào (các kiểu "đóng", "kéo ra", "ép"). */
  zIndex?: number;
}

const smooth = (x: number) => x * x * (3 - 2 * x);

function dirOf(id: string): "left" | "right" | "up" | "down" | null {
  if (id.endsWith("_left")) return "left";
  if (id.endsWith("_right")) return "right";
  if (id.endsWith("_up")) return "up";
  if (id.endsWith("_down")) return "down";
  return null;
}

/** Dịch chuyển theo hướng: nội dung đi về phía `d` một đoạn `k` (0..1 khung). */
function shift(d: "left" | "right" | "up" | "down", k: number): string {
  const v = k * 100;
  switch (d) {
    case "left":
      return `translateX(${-v}%)`;
    case "right":
      return `translateX(${v}%)`;
    case "up":
      return `translateY(${-v}%)`;
    case "down":
      return `translateY(${v}%)`;
  }
}

/** Phần còn hiện của clip vào khi quét theo hướng (clip-path inset). */
function wipeInset(d: string, p: number): string {
  const r = (1 - p) * 100;
  switch (d) {
    case "left": // quét từ phải sang trái
      return `inset(0 0 0 ${r}%)`;
    case "right":
      return `inset(0 ${r}% 0 0)`;
    case "up":
      return `inset(${r}% 0 0 0)`;
    default:
      return `inset(0 0 ${r}% 0)`;
  }
}

export function transitionStyles(id: string, p: number): { out: TrStyle; in: TrStyle } {
  p = Math.min(1, Math.max(0, p));
  const s = smooth(p);
  const d = dirOf(id);
  const fadeIn = { out: {}, in: { opacity: s } };

  // Động (kiểu CapCut)
  if (id === "zoom_through") return { out: { transform: `scale(${1 + 2.5 * p * p})` }, in: { transform: `scale(${1 + 2.5 * (1 - p) ** 2})`, opacity: s } };
  if (id === "spin" || id === "spin_ccw") {
    const sg = id === "spin" ? 1 : -1;
    return {
      out: { transform: `rotate(${sg * p * p * 60}deg) scale(${1 + 1.3 * p})` },
      in: { transform: `rotate(${-sg * (1 - p) ** 2 * 60}deg) scale(${1 + 1.3 * (1 - p)})`, opacity: s },
    };
  }
  if (id === "shake") {
    const a = Math.sin(Math.PI * p) * 6;
    const t = `translate(${Math.sin(p * 53) * a}%, ${Math.cos(p * 41) * a}%) scale(${1 + 0.15 * Math.sin(Math.PI * p)})`;
    return { out: { transform: t }, in: { transform: t, opacity: s } };
  }
  if (id.startsWith("whip_") && d) {
    const blur = `blur(${Math.sin(Math.PI * p) * 6}px)`;
    return { out: { transform: shift(d, s), filter: blur }, in: { transform: shift(d, s - 1), filter: blur } };
  }
  if (id === "glitch") {
    const on = Math.sin(Math.PI * p) > 0.3;
    const j = on ? `translateX(${(Math.sin(p * 97) * 4).toFixed(1)}%)` : "";
    return {
      out: { transform: j, filter: on ? "hue-rotate(90deg) saturate(2)" : "", opacity: p < 0.5 ? 1 : 0 },
      in: { transform: j, filter: on ? "hue-rotate(-90deg) saturate(2)" : "", opacity: p < 0.5 ? 0 : 1 },
    };
  }
  if (id === "flash" || id === "fade_white") {
    const b = 1 + 3 * Math.sin(Math.PI * p);
    return { out: { filter: `brightness(${b})` }, in: { filter: `brightness(${b})`, opacity: s } };
  }
  if (id === "fade_black") return { out: { opacity: 1 - Math.min(1, p * 2) }, in: { opacity: Math.max(0, p * 2 - 1) } };
  if (id === "fade_grays") return { out: { filter: `grayscale(${Math.min(1, p * 2)})` }, in: { filter: `grayscale(${Math.max(0, 1 - p * 2) + (p < 0.5 ? 1 : 0)})`, opacity: s } };
  if (id === "fade_fast") return { out: {}, in: { opacity: Math.min(1, p * 2) } };
  if (id === "fade_slow") return { out: {}, in: { opacity: p * p } };

  // Trượt / lướt / phủ / kéo ra
  if (d && (id.startsWith("slide_") || id.startsWith("smooth_"))) return { out: { transform: shift(d, s) }, in: { transform: shift(d, s - 1) } };
  if (d && id.startsWith("cover_")) return { out: {}, in: { transform: shift(d, s - 1) } };
  if (d && id.startsWith("reveal_")) return { out: { transform: shift(d, s), zIndex: 2 }, in: {} };

  // Quét / chéo / hình
  if (d && id.startsWith("wipe_")) return { out: {}, in: { clipPath: wipeInset(d, s) } };
  if (id.startsWith("wipe_") || id.startsWith("diag_")) {
    const k = s * 200;
    const c: Record<string, string> = {
      tl: `polygon(100% 100%, ${100 - k}% 100%, 100% ${100 - k}%)`,
      tr: `polygon(0 100%, ${k}% 100%, 0 ${100 - k}%)`,
      bl: `polygon(100% 0, ${100 - k}% 0, 100% ${k}%)`,
      br: `polygon(0 0, ${k}% 0, 0 ${k}%)`,
    };
    return { out: {}, in: { clipPath: c[id.slice(-2)] ?? "none" } };
  }
  if (id === "circle_open" || id === "circle_crop") return { out: {}, in: { clipPath: `circle(${s * 75}% at 50% 50%)` } };
  if (id === "circle_close") return { out: { clipPath: `circle(${(1 - s) * 75}% at 50% 50%)`, zIndex: 2 }, in: {} };
  if (id === "rect_crop") return { out: {}, in: { clipPath: `inset(${(1 - s) * 50}%)` } };
  if (id === "vert_open") return { out: {}, in: { clipPath: `inset(0 ${(1 - s) * 50}%)` } };
  if (id === "horz_open") return { out: {}, in: { clipPath: `inset(${(1 - s) * 50}% 0)` } };
  if (id === "vert_close") return { out: { clipPath: `inset(0 ${s * 50}%)`, zIndex: 2 }, in: {} };
  if (id === "horz_close") return { out: { clipPath: `inset(${s * 50}% 0)`, zIndex: 2 }, in: {} };
  if (id === "squeeze_h") return { out: { transform: `scaleX(${1 - s})`, zIndex: 2 }, in: {} };
  if (id === "squeeze_v") return { out: { transform: `scaleY(${1 - s})`, zIndex: 2 }, in: {} };
  if (id === "radial") return { out: {}, in: { maskImage: `conic-gradient(#000 ${s * 360}deg, transparent 0)` } };

  // Hiệu ứng
  if (id === "zoom_in") return { out: { transform: `scale(${1 + p})`, opacity: 1 - s }, in: { opacity: s } };
  if (id === "pixelize" || id === "hblur" || id.startsWith("slice_") || id.startsWith("wind_")) {
    const b = Math.sin(Math.PI * p) * 8;
    return { out: { filter: `blur(${b}px)` }, in: { filter: `blur(${b}px)`, opacity: s } };
  }
  return fadeIn;
}

/** Gộp style chuyển cảnh vào style sẵn có của lớp (transform/filter nối thêm, opacity nhân). */
export function mergeStyle(base: CSSProperties, tr: TrStyle | undefined): CSSProperties {
  if (!tr) return base;
  return {
    ...base,
    transform: [base.transform, tr.transform].filter(Boolean).join(" ") || undefined,
    filter: [base.filter, tr.filter].filter(Boolean).join(" ") || undefined,
    opacity: (typeof base.opacity === "number" ? base.opacity : 1) * (tr.opacity ?? 1),
    clipPath: tr.clipPath ?? base.clipPath,
    maskImage: tr.maskImage,
    WebkitMaskImage: tr.maskImage,
    zIndex: tr.zIndex ?? base.zIndex,
  };
}
