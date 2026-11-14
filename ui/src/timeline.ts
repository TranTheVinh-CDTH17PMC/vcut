// Tính vị trí clip trên timeline: bản TypeScript của Project::video_layout (Rust).

import { times } from "./keyframe";
import type { Project, VideoClip } from "./types";

export interface Span {
  start: number;
  duration: number;
  transition: number;
}

export const clipDuration = (c: VideoClip) => (c.src_out - c.src_in) / c.speed;

export function videoLayout(p: Project): Span[] {
  const out: Span[] = [];
  let cursor = 0;
  p.video.forEach((c, i) => {
    let tr = 0;
    if (i > 0 && c.transition_in && c.transition_in.duration > 0) {
      const max = Math.min(clipDuration(p.video[i - 1]), clipDuration(c)) / 2;
      tr = Math.max(0, Math.min(c.transition_in.duration, max));
    }
    const start = Math.max(0, cursor - tr);
    const duration = clipDuration(c);
    out.push({ start, duration, transition: tr });
    cursor = start + duration;
  });
  return out;
}

export function projectDuration(p: Project, layout = videoLayout(p)): number {
  const last = layout[layout.length - 1];
  return last ? last.start + last.duration : 0;
}

/** Clip đang hiện tại thời điểm t (lúc chuyển cảnh lấy clip sau). */
export function clipIndexAt(layout: Span[], t: number): number {
  for (let i = layout.length - 1; i >= 0; i--) {
    if (t >= layout[i].start && t < layout[i].start + layout[i].duration) return i;
  }
  return -1;
}

export function clipKeyframeTimes(c: VideoClip): number[] {
  const all = [
    ...times(c.transform.position),
    ...times(c.transform.scale),
    ...times(c.transform.rotation),
    ...times(c.transform.opacity),
    ...times(c.color.brightness),
    ...times(c.color.contrast),
    ...times(c.color.saturation),
    ...times(c.volume),
  ].sort((a, b) => a - b);
  return all.filter((t, i) => i === 0 || Math.abs(t - all[i - 1]) > 1e-3);
}

export function fmtTime(s: number): string {
  if (!isFinite(s) || s < 0) s = 0;
  const m = Math.floor(s / 60);
  const sec = s - m * 60;
  return `${m}:${sec.toFixed(2).padStart(5, "0")}`;
}

export const ASPECTS = [
  { label: "9:16 dọc", width: 1080, height: 1920 },
  { label: "16:9 ngang", width: 1920, height: 1080 },
  { label: "1:1 vuông", width: 1080, height: 1080 },
  { label: "4:5 Instagram", width: 1080, height: 1350 },
];
