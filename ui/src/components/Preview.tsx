// Khung xem trước. Dùng proxy (bản nhẹ) + CSS filter để mô phỏng gần đúng
// bản xuất. Hiệu ứng nặng (motion blur) xem chính xác bằng nút "Render thử".

import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { fileUrl } from "../api";
import { numAt, vecAt } from "../keyframe";
import { clipIndexAt, type Span } from "../timeline";
import type { AudioClip, PresetInfo, Project, TextClip, VideoClip } from "../types";

interface Props {
  project: Project;
  layout: Span[];
  time: number;
  playing: boolean;
  presets: PresetInfo[];
  selectedClipId: string | null;
}

export function Preview({ project, layout, time, playing, presets }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  const { width: W, height: H } = project.settings;

  // Tính kích thước khung vừa với vùng trống, giữ đúng tỉ lệ.
  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const aw = el.clientWidth - 28;
      const ah = el.clientHeight - 28;
      const s = Math.min(aw / W, ah / H);
      setBox({ w: Math.max(0, Math.floor(W * s)), h: Math.max(0, Math.floor(H * s)) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [W, H]);

  let idx = clipIndexAt(layout, time);
  if (idx < 0 && layout.length && time > 0) idx = layout.length - 1;
  const visible = [idx, idx + 1].filter((i) => i >= 0 && i < project.video.length);
  const videoRefs = useRef(new Map<string, HTMLVideoElement>());
  const audioRefs = useRef(new Map<string, HTMLAudioElement>());

  // Đồng bộ thẻ <video> với đồng hồ timeline.
  useEffect(() => {
    visible.forEach((i) => {
      const clip = project.video[i];
      const el = videoRefs.current.get(clip.id);
      if (!el) return;
      const active = i === idx;
      if (!active) {
        el.pause();
        if (Math.abs(el.currentTime - clip.src_in) > 0.05) el.currentTime = clip.src_in;
        return;
      }
      const local = Math.max(0, time - layout[i].start);
      const desired = clip.src_in + local * clip.speed;
      el.playbackRate = clip.speed;
      el.volume = Math.min(1, Math.max(0, numAt(clip.volume, local)));
      if (playing) {
        if (el.paused) {
          el.currentTime = desired;
          el.play().catch(() => {});
        } else if (Math.abs(el.currentTime - desired) > 0.3) {
          el.currentTime = desired;
        }
      } else {
        el.pause();
        if (Math.abs(el.currentTime - desired) > 0.03) el.currentTime = desired;
      }
    });
  });

  // Đồng bộ nhạc nền / giọng đọc.
  const activeAudio = project.audio.filter((a) => time >= a.start - 2 && time < a.start + (a.src_out - a.src_in));
  useEffect(() => {
    activeAudio.forEach((a) => {
      const el = audioRefs.current.get(a.id);
      if (!el) return;
      const local = time - a.start;
      const len = a.src_out - a.src_in;
      if (local < 0 || local >= len) {
        el.pause();
        return;
      }
      let vol = numAt(a.volume, local);
      if (a.fade_in > 0) vol *= Math.min(1, local / a.fade_in);
      if (a.fade_out > 0) vol *= Math.min(1, (len - local) / a.fade_out);
      el.volume = Math.min(1, Math.max(0, vol));
      const desired = a.src_in + local;
      if (playing) {
        if (el.paused) {
          el.currentTime = desired;
          el.play().catch(() => {});
        } else if (Math.abs(el.currentTime - desired) > 0.3) el.currentTime = desired;
      } else {
        el.pause();
        if (Math.abs(el.currentTime - desired) > 0.05) el.currentTime = desired;
      }
    });
  });

  const presetCss = (id: string) => presets.find((p) => p.id === id)?.css ?? "";
  const scaleText = box.h / H;

  const activeClip = idx >= 0 ? project.video[idx] : null;
  const texts = project.texts.filter((t) => time >= t.start && time < t.end);

  return (
    <div className="preview-wrap" ref={wrapRef}>
      <div className="frame" style={{ width: box.w, height: box.h }}>
        {visible.map((i) => {
          const clip = project.video[i];
          const media = project.media.find((m) => m.id === clip.media_id);
          if (!media) return null;
          const local = Math.max(0, time - layout[i].start);
          const style: CSSProperties = { ...clipStyle(clip, local, presetCss(clip.color.preset)), visibility: i === idx ? "visible" : "hidden" };
          if (media.kind === "image") {
            return <img key={clip.id} className="layer" src={fileUrl(media.path)} style={style} draggable={false} />;
          }
          return (
            <video
              key={clip.id}
              ref={(el) => {
                if (el) videoRefs.current.set(clip.id, el);
                else videoRefs.current.delete(clip.id);
              }}
              src={fileUrl(media.proxy ?? media.path)}
              style={style}
              preload="auto"
              playsInline
            />
          );
        })}
        {texts.map((t) => (
          <TextLayer key={t.id} t={t} time={time} W={W} H={H} k={scaleText} />
        ))}
        {activeAudio.map((a) => (
          <AudioEl key={a.id} a={a} project={project} refs={audioRefs.current} />
        ))}
        {activeClip && activeClip.motion_blur > 0 && (
          <div className="hint">
            <span>Motion blur: bấm "Render thử" để xem chính xác</span>
          </div>
        )}
        {project.video.length === 0 && (
          <div className="empty">Kéo video hoặc ảnh từ thư viện xuống track Video để bắt đầu</div>
        )}
      </div>
    </div>
  );
}

function clipStyle(clip: VideoClip, local: number, preset: string): CSSProperties {
  const b = numAt(clip.color.brightness, local);
  const c = numAt(clip.color.contrast, local);
  const s = numAt(clip.color.saturation, local);
  const pos = vecAt(clip.transform.position, local);
  const sc = numAt(clip.transform.scale, local);
  const rot = numAt(clip.transform.rotation, local);
  const op = numAt(clip.transform.opacity, local);
  return {
    filter: `${preset} brightness(${1 + b}) contrast(${c}) saturate(${s})`,
    transform: `translate(${pos.x * 100}%, ${pos.y * 100}%) scale(${sc}) rotate(${rot}deg)`,
    opacity: op,
  };
}

function TextLayer({ t, time, W, H, k }: { t: TextClip; time: number; W: number; H: number; k: number }) {
  const local = time - t.start;
  const base = { x: 0.5, y: t.style.align === "top" ? 0.12 : t.style.align === "center" ? 0.5 : 0.82 };
  const pos = t.transform.position ? vecAt(t.transform.position, local) : base;
  let sc = t.transform.scale ? numAt(t.transform.scale, local) : 1;
  let op = t.transform.opacity ? numAt(t.transform.opacity, local) : 1;
  let dy = 0;
  // Mô phỏng hiệu ứng xuất hiện (chỉ khi không có keyframe).
  if (!t.transform.position && !t.transform.scale && !t.transform.opacity) {
    const len = t.end - t.start;
    if (t.style.anim_in === "fade" || t.style.anim_in === "slide_up") op *= Math.min(1, local / 0.25, (len - local) / 0.15);
    if (t.style.anim_in === "pop") sc *= 0.6 + 0.4 * Math.min(1, local / 0.2);
    if (t.style.anim_in === "slide_up") dy = H * 0.04 * Math.max(0, 1 - local / 0.3);
  }
  const style: CSSProperties = {
    left: pos.x * W * k,
    top: (pos.y * H + dy) * k,
    fontFamily: `"${t.style.font}", Arial, sans-serif`,
    fontSize: t.style.size * k,
    fontWeight: t.style.bold ? 700 : 400,
    color: t.style.color,
    WebkitTextStroke: `${t.style.outline * 2 * k}px ${t.style.outline_color}`,
    opacity: Math.max(0, op),
    transform: `translate(-50%, -50%) scale(${sc})`,
  };
  // Chữ chạy theo giọng: từ nào đã được nói thì đổi sang màu tô.
  if (t.style.karaoke && t.words && t.words.length) {
    return (
      <div className="text-layer" style={style}>
        {t.words.map((w, i) => (
          <span key={i} style={{ color: local >= w.start ? t.style.highlight_color ?? "#FFD60A" : t.style.color }}>
            {w.text}
            {i < t.words!.length - 1 ? " " : ""}
          </span>
        ))}
      </div>
    );
  }
  return (
    <div className="text-layer" style={style}>
      {t.text}
    </div>
  );
}

function AudioEl({ a, project, refs }: { a: AudioClip; project: Project; refs: Map<string, HTMLAudioElement> }) {
  const m = project.media.find((x) => x.id === a.media_id);
  if (!m) return null;
  const src = m.kind === "video" ? m.proxy ?? m.path : m.path;
  return (
    <audio
      ref={(el) => {
        if (el) refs.set(a.id, el);
        else refs.delete(a.id);
      }}
      src={fileUrl(src)}
      preload="auto"
    />
  );
}
