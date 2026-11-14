// Bảng thuộc tính bên phải: chỉnh clip / chữ / âm thanh đang chọn,
// hoặc cài đặt khung hình khi chưa chọn gì.

import { useEffect, useState } from "react";
import { fileUrl } from "../api";
import { isAnimated, numAt } from "../keyframe";
import { ASPECTS, clipDuration, type Span } from "../timeline";
import type { Animatable, AudioClip, Op, PresetInfo, Project, Selection, TextClip, TransitionInfo, VideoClip } from "../types";
import { TransitionPicker } from "./TransitionPicker";

interface Props {
  project: Project;
  layout: Span[];
  selection: Selection;
  presets: PresetInfo[];
  transitions: TransitionInfo[];
  transitionGroups: string[];
  time: number;
  apply: (ops: Op[]) => void;
  onPreviewClip: (clipId: string) => void;
}

export function Inspector({ project, layout, selection, presets, transitions, transitionGroups, time, apply, onPreviewClip }: Props) {
  if (selection?.kind === "clip") {
    const i = project.video.findIndex((c) => c.id === selection.id);
    if (i >= 0) return <ClipPanel key={selection.id} project={project} clip={project.video[i]} index={i} span={layout[i]} presets={presets} transitions={transitions} transitionGroups={transitionGroups} time={time} apply={apply} onPreview={onPreviewClip} />;
  }
  if (selection?.kind === "text") {
    const t = project.texts.find((x) => x.id === selection.id);
    if (t) return <TextPanel key={t.id} t={t} apply={apply} />;
  }
  if (selection?.kind === "audio") {
    const a = project.audio.find((x) => x.id === selection.id);
    if (a) return <AudioPanel key={a.id} a={a} project={project} apply={apply} />;
  }
  return <ProjectPanel project={project} apply={apply} />;
}

/** Thanh trượt: cập nhật hiển thị khi kéo, chỉ ghi vào project khi thả tay. */
function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format?: (v: number) => string;
  onCommit: (v: number) => void;
  disabled?: boolean;
}) {
  const [v, setV] = useState(props.value);
  useEffect(() => setV(props.value), [props.value]);
  const commit = () => {
    if (v !== props.value) props.onCommit(v);
  };
  return (
    <label className="field">
      <span className="row between">
        <span>{props.label}</span>
        <span>{props.format ? props.format(v) : v}</span>
      </span>
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={v}
        disabled={props.disabled}
        onChange={(e) => setV(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
    </label>
  );
}

/** Thuộc tính có keyframe: hiện ghi chú + nút bỏ keyframe thay cho thanh trượt. */
function AnimNote({ anim, onClear }: { anim: Animatable<unknown>; onClear: () => void }) {
  if (!isAnimated(anim)) return null;
  return (
    <div className="row between">
      <span className="kf-note">Đang có keyframe ({anim.length} điểm)</span>
      <button className="ghost small" onClick={onClear}>
        Bỏ keyframe
      </button>
    </div>
  );
}


function ClipPanel({ project, clip, index, span, presets, transitions, transitionGroups, time, apply, onPreview }: {
  project: Project;
  clip: VideoClip;
  index: number;
  span: Span;
  presets: PresetInfo[];
  transitions: TransitionInfo[];
  transitionGroups: string[];
  time: number;
  apply: (ops: Op[]) => void;
  onPreview: (id: string) => void;
}) {
  const media = project.media.find((m) => m.id === clip.media_id);
  const set = (props: Record<string, unknown>) => apply([{ op: "set_clip_props", clip_id: clip.id, ...props }]);
  const clear = (property: string) => apply([{ op: "remove_keyframes", target: { kind: "clip", id: clip.id }, property }]);
  const local = Math.min(Math.max(0, time - span.start), span.duration);
  const d = clipDuration(clip);
  const target = { kind: "clip", id: clip.id };

  const kenBurns = (zoomIn: boolean) =>
    apply([
      { op: "set_keyframes", target, property: "scale", keyframes: [{ t: 0, value: zoomIn ? 1 : 1.2, easing: "ease_in_out" }, { t: d, value: zoomIn ? 1.2 : 1 }] },
      { op: "set_keyframes", target, property: "position", keyframes: [{ t: 0, value: { x: 0, y: 0 }, easing: "ease_in_out" }, { t: d, value: { x: zoomIn ? 0.04 : -0.04, y: 0 } }] },
    ]);

  const fmtPct = (v: number) => `${v > 0 ? "+" : ""}${Math.round(v * 100)}`;

  return (
    <>
      <div className="section">
        <div className="row between">
          <h3>Thuộc tính clip</h3>
          <span className="muted small" title={media?.path}>
            {media?.name}
          </span>
        </div>
        <span className="muted small">
          Dài {d.toFixed(2)}s · nguồn {clip.src_in.toFixed(2)}–{clip.src_out.toFixed(2)}s
        </span>
        <div className="field">
          <span className="muted small">Filter màu</span>
          <div className="preset-grid">
            {presets.map((p) => (
              <button key={p.id} className={`preset ${clip.color.preset === p.id ? "on" : ""}`} onClick={() => set({ preset: p.id })}>
                <span className="sw" style={{ backgroundImage: media?.thumb ? `url("${fileUrl(media.thumb)}")` : undefined, filter: p.css }} />
                {p.label}
              </button>
            ))}
          </div>
        </div>
        <AnimNote anim={clip.color.brightness} onClear={() => clear("brightness")} />
        <Slider label="Độ sáng" value={numAt(clip.color.brightness, local)} min={-0.5} max={0.5} step={0.01} format={fmtPct} disabled={isAnimated(clip.color.brightness)} onCommit={(v) => set({ brightness: v })} />
        <AnimNote anim={clip.color.contrast} onClear={() => clear("contrast")} />
        <Slider label="Tương phản" value={numAt(clip.color.contrast, local)} min={0.5} max={1.5} step={0.01} format={(v) => fmtPct(v - 1)} disabled={isAnimated(clip.color.contrast)} onCommit={(v) => set({ contrast: v })} />
        <AnimNote anim={clip.color.saturation} onClear={() => clear("saturation")} />
        <Slider label="Bão hoà" value={numAt(clip.color.saturation, local)} min={0} max={2} step={0.01} format={(v) => fmtPct(v - 1)} disabled={isAnimated(clip.color.saturation)} onCommit={(v) => set({ saturation: v })} />
      </div>

      <div className="section">
        <h3>Chuyển động</h3>
        <div className="box">
          <label className="row between" style={{ color: "var(--text)", fontSize: 13 }}>
            Motion blur
            <input type="checkbox" checked={clip.motion_blur > 0} onChange={(e) => set({ motion_blur: e.target.checked ? 4 : 0 })} />
          </label>
          {clip.motion_blur > 0 && (
            <Slider label="Cường độ" value={clip.motion_blur} min={2} max={10} step={1} format={(v) => `${v} khung`} onCommit={(v) => set({ motion_blur: v })} />
          )}
        </div>
        <AnimNote anim={clip.transform.scale} onClear={() => apply([
          { op: "remove_keyframes", target, property: "scale" },
          { op: "remove_keyframes", target, property: "position" },
          { op: "set_clip_props", clip_id: clip.id, scale: 1, position: { x: 0, y: 0 } },
        ])} />
        {!isAnimated(clip.transform.scale) && (
          <Slider label="Phóng to" value={numAt(clip.transform.scale, 0)} min={0.5} max={2} step={0.01} format={(v) => `${Math.round(v * 100)}%`} onCommit={(v) => set({ scale: v })} />
        )}
        <div className="row">
          <button style={{ flex: 1 }} onClick={() => kenBurns(true)} title="Từ từ phóng to suốt clip">
            Ken Burns vào
          </button>
          <button style={{ flex: 1 }} onClick={() => kenBurns(false)} title="Từ từ thu nhỏ suốt clip">
            Ken Burns ra
          </button>
        </div>
        <div className="row">
          <label className="field" style={{ flex: 1 }}>
            Tốc độ
            <select value={clip.speed} onChange={(e) => set({ speed: Number(e.target.value) })}>
              {[0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4].map((s) => (
                <option key={s} value={s}>
                  {s}×
                </option>
              ))}
            </select>
          </label>
        </div>
        {index > 0 && (
          <TransitionPicker
            list={transitions}
            groups={transitionGroups}
            value={clip.transition_in?.kind ?? null}
            duration={clip.transition_in?.duration ?? 0.5}
            onPick={(id) => (id ? set({ transition_in: { kind: id, duration: clip.transition_in?.duration ?? 0.5 } }) : set({ remove_transition: true }))}
            onDuration={(v) => clip.transition_in && set({ transition_in: { kind: clip.transition_in.kind, duration: v } })}
            onApplyAll={() =>
              clip.transition_in &&
              apply(project.video.slice(1).map((c) => ({ op: "set_clip_props", clip_id: c.id, transition_in: { ...clip.transition_in! } })))
            }
          />
        )}
        {media?.has_audio && (
          <>
            <AnimNote anim={clip.volume} onClear={() => clear("volume")} />
            <Slider label="Âm lượng" value={numAt(clip.volume, local)} min={0} max={2} step={0.05} format={(v) => `${Math.round(v * 100)}%`} disabled={isAnimated(clip.volume)} onCommit={(v) => set({ volume: v })} />
          </>
        )}
        <button onClick={() => onPreview(clip.id)}>Render thử clip này</button>
      </div>
    </>
  );
}

function TextPanel({ t, apply }: { t: TextClip; apply: (ops: Op[]) => void }) {
  const [text, setText] = useState(t.text);
  useEffect(() => setText(t.text), [t.text]);
  const style = (patch: Partial<TextClip["style"]>) => apply([{ op: "update_text", text_id: t.id, style: { ...t.style, ...patch } }]);
  return (
    <div className="section">
      <h3>Chữ</h3>
      <textarea rows={3} value={text} onChange={(e) => setText(e.target.value)} onBlur={() => text !== t.text && text.trim() && apply([{ op: "update_text", text_id: t.id, text }])} />
      <span className="muted small">
        {t.start.toFixed(2)}s – {t.end.toFixed(2)}s
      </span>
      <Slider label="Cỡ chữ" value={t.style.size} min={24} max={200} step={1} onCommit={(v) => style({ size: v })} />
      <div className="row">
        <label className="field">
          Màu chữ
          <input type="color" value={t.style.color} onChange={(e) => style({ color: e.target.value.toUpperCase() })} />
        </label>
        <label className="field">
          Màu viền
          <input type="color" value={t.style.outline_color} onChange={(e) => style({ outline_color: e.target.value.toUpperCase() })} />
        </label>
        <label className="row" style={{ marginTop: 16 }}>
          <input type="checkbox" checked={t.style.bold} onChange={(e) => style({ bold: e.target.checked })} /> Đậm
        </label>
      </div>
      <Slider label="Độ dày viền" value={t.style.outline} min={0} max={12} step={0.5} onCommit={(v) => style({ outline: v })} />
      {t.words && t.words.length > 0 && (
        <label className="row" style={{ color: "var(--text)", fontSize: 13 }}>
          <input type="checkbox" checked={!!t.style.karaoke} onChange={(e) => style({ karaoke: e.target.checked })} />
          Chữ chạy theo giọng
          <input type="color" value={t.style.highlight_color ?? "#FFD60A"} disabled={!t.style.karaoke} onChange={(e) => style({ highlight_color: e.target.value.toUpperCase() })} />
        </label>
      )}
      <div className="row">
        <label className="field" style={{ flex: 1 }}>
          Vị trí
          <select value={t.style.align} onChange={(e) => style({ align: e.target.value as TextClip["style"]["align"] })}>
            <option value="top">Trên</option>
            <option value="center">Giữa</option>
            <option value="bottom">Dưới</option>
          </select>
        </label>
        <label className="field" style={{ flex: 1 }}>
          Hiệu ứng vào
          <select value={t.style.anim_in} onChange={(e) => style({ anim_in: e.target.value as TextClip["style"]["anim_in"] })}>
            <option value="none">Không</option>
            <option value="fade">Mờ dần</option>
            <option value="pop">Nảy</option>
            <option value="slide_up">Trượt lên</option>
          </select>
        </label>
      </div>
      {(t.transform.position || t.transform.scale || t.transform.opacity) && (
        <div className="row between">
          <span className="kf-note">Chữ đang có keyframe</span>
          <button
            className="ghost"
            onClick={() =>
              apply(
                (["position", "scale", "opacity"] as const)
                  .filter((p) => t.transform[p])
                  .map((property) => ({ op: "remove_keyframes", target: { kind: "text", id: t.id }, property })),
              )
            }
          >
            Bỏ keyframe
          </button>
        </div>
      )}
    </div>
  );
}

function AudioPanel({ a, project, apply }: { a: AudioClip; project: Project; apply: (ops: Op[]) => void }) {
  const media = project.media.find((m) => m.id === a.media_id);
  const set = (props: Record<string, unknown>) => apply([{ op: "set_audio_props", audio_id: a.id, ...props }]);
  const len = a.src_out - a.src_in;
  return (
    <div className="section">
      <div className="row between">
        <h3>Âm thanh</h3>
        <span className="muted small">{media?.name}</span>
      </div>
      <label className="field">
        Loại
        <select value={a.role} onChange={(e) => set({ role: e.target.value })}>
          <option value="music">Nhạc nền</option>
          <option value="voice">Giọng đọc</option>
        </select>
      </label>
      <AnimNote anim={a.volume} onClear={() => apply([{ op: "remove_keyframes", target: { kind: "audio", id: a.id }, property: "volume" }])} />
      <Slider label="Âm lượng" value={numAt(a.volume, 0)} min={0} max={2} step={0.05} format={(v) => `${Math.round(v * 100)}%`} disabled={isAnimated(a.volume)} onCommit={(v) => set({ volume: v })} />
      <Slider label="Fade vào" value={a.fade_in} min={0} max={Math.min(5, len)} step={0.1} format={(v) => `${v.toFixed(1)}s`} onCommit={(v) => set({ fade_in: v })} />
      <Slider label="Fade ra" value={a.fade_out} min={0} max={Math.min(5, len)} step={0.1} format={(v) => `${v.toFixed(1)}s`} onCommit={(v) => set({ fade_out: v })} />
      <label className="row" style={{ color: "var(--text)" }}>
        <input type="checkbox" checked={a.duck} onChange={(e) => set({ duck: e.target.checked })} />
        Tự giảm nhạc khi video có tiếng nói
      </label>
      <span className="muted small">Tự giảm nhạc chỉ nghe được ở bản xuất, chưa có khi xem trước.</span>
    </div>
  );
}

function ProjectPanel({ project, apply }: { project: Project; apply: (ops: Op[]) => void }) {
  const s = project.settings;
  const cur = ASPECTS.findIndex((a) => a.width === s.width && a.height === s.height);
  return (
    <div className="section">
      <h3>Cài đặt video</h3>
      <label className="field">
        Khung hình
        <select
          value={cur}
          onChange={(e) => {
            const a = ASPECTS[Number(e.target.value)];
            apply([{ op: "set_settings", width: a.width, height: a.height }]);
          }}
        >
          {cur < 0 && <option value={-1}>{s.width}×{s.height}</option>}
          {ASPECTS.map((a, i) => (
            <option key={a.label} value={i}>
              {a.label} ({a.width}×{a.height})
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        Số khung/giây
        <select value={s.fps} onChange={(e) => apply([{ op: "set_settings", fps: Number(e.target.value) }])}>
          {[24, 25, 30, 50, 60].map((f) => (
            <option key={f} value={f}>
              {f} fps
            </option>
          ))}
        </select>
      </label>
      <span className="muted small" style={{ lineHeight: 1.6 }}>
        Chọn một clip, chữ hoặc nhạc trên timeline để chỉnh. Phím tắt: Space phát/dừng, S tách, Delete xoá, Ctrl+Z hoàn tác, ← → tua từng khung.
      </span>
    </div>
  );
}
