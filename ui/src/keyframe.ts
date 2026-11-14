// Nội suy keyframe: bản TypeScript của crates/core/src/keyframe.rs.
// Phải cho kết quả giống hệt bản Rust để xem trước khớp với video xuất ra.

import type { Animatable, Easing, Keyframe, Vec2 } from "./types";

export function ease(e: Easing | undefined, u: number): number {
  u = Math.min(1, Math.max(0, u));
  switch (e ?? "linear") {
    case "ease_in":
      return u * u;
    case "ease_out":
      return u * (2 - u);
    case "ease_in_out":
      return u * u * (3 - 2 * u);
    case "hold":
      return 0;
    default:
      return u;
  }
}

export function isAnimated<T>(a: Animatable<T> | null | undefined): a is Keyframe<T>[] {
  return Array.isArray(a) && a.length > 0;
}

type Lerp<T> = (a: T, b: T, u: number) => T;
const lerpNum: Lerp<number> = (a, b, u) => a + (b - a) * u;
const lerpVec: Lerp<Vec2> = (a, b, u) => ({ x: lerpNum(a.x, b.x, u), y: lerpNum(a.y, b.y, u) });

function valueAt<T>(a: Animatable<T>, t: number, lerp: Lerp<T>): T {
  if (!Array.isArray(a)) return a;
  const k = a;
  if (k.length === 0) throw new Error("keyframe rỗng");
  if (t <= k[0].t) return k[0].value;
  const last = k[k.length - 1];
  if (t >= last.t) return last.value;
  for (let i = 0; i < k.length - 1; i++) {
    const p = k[i];
    const q = k[i + 1];
    if (t >= p.t && t <= q.t) {
      const span = q.t - p.t;
      if (span <= 1e-9) return q.value;
      return lerp(p.value, q.value, ease(p.easing, (t - p.t) / span));
    }
  }
  return last.value;
}

export const numAt = (a: Animatable<number>, t: number) => valueAt(a, t, lerpNum);
export const vecAt = (a: Animatable<Vec2>, t: number) => valueAt(a, t, lerpVec);

export function times<T>(a: Animatable<T> | null | undefined): number[] {
  return isAnimated(a) ? a.map((k) => k.t) : [];
}
