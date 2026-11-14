// Timeline 4 track: Chữ, Video, Giọng đọc, Nhạc nền.
// Kéo thân clip để di chuyển / đổi thứ tự, kéo mép để cắt. Bấm thước để tua.

import { useEffect, useRef, useState, type PointerEvent as RPointerEvent } from "react";
import { fileUrl } from "../api";
import type { AudioClip, Op, Project, Selection, TextClip, VideoClip } from "../types";
import { clipKeyframeTimes, fmtTime, type Span } from "../timeline";
import { Icon } from "./Icon";

interface Props {
  project: Project;
  layout: Span[];
  duration: number;
  time: number;
  zoom: number;
  setZoom: (z: number) => void;
  selection: Selection;
  affected: string[];
  dragging: boolean;
  playing: boolean;
  onSeek: (t: number) => void;
  onSelect: (s: Selection) => void;
  apply: (ops: Op[]) => void;
  onSplit: () => void;
  onRemove: () => void;
  onSpeech: () => void;
}

type DragKind = "move" | "l" | "r";
interface DragState {
  id: string;
  kind: DragKind;
  dx: number; // giây
}

const LANES = { ruler: 24, text: 34, video: 58, voice: 38, music: 44 };
const PRESET_LABEL: Record<string, string> = {
  cinematic: "Cinematic",
  vintage: "Vintage",
  cool: "Lạnh",
  warm: "Ấm",
  black_white: "Đen trắng",
  vivid: "Rực rỡ",
};

/** Theo dõi kéo chuột: gọi onMove liên tục, onEnd khi thả. dx tính bằng pixel. */
function trackDrag(e: RPointerEvent, onMove: (dx: number) => void, onEnd: (dx: number, moved: boolean) => void) {
  e.stopPropagation();
  const sx = e.clientX;
  let moved = false;
  const move = (ev: PointerEvent) => {
    const dx = ev.clientX - sx;
    if (Math.abs(dx) > 3) moved = true;
    if (moved) onMove(dx);
  };
  const up = (ev: PointerEvent) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    onEnd(ev.clientX - sx, moved);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

export function Timeline(props: Props) {
  const { project, layout, duration, time, zoom, setZoom, selection, affected, dragging, playing, onSeek, onSelect, apply } = props;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [hoverLane, setHoverLane] = useState<string | null>(null);

  const width = Math.max((duration + 20) * zoom, 800);
  const totalH = LANES.ruler + LANES.text + LANES.video + LANES.voice + LANES.music;

  // Tự cuộn theo playhead khi đang phát.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !playing) return;
    const x = time * zoom;
    if (x < el.scrollLeft || x > el.scrollLeft + el.clientWidth - 40) el.scrollLeft = Math.max(0, x - 100);
  }, [time, zoom, playing]);

  const isSel = (kind: string, id: string) => selection?.kind === kind && selection.id === id;
  const mediaOf = (id: string) => project.media.find((m) => m.id === id);

  // Tua bằng cách bấm/kéo trên thước hoặc vùng trống.
  const seekFrom = (e: RPointerEvent) => {
    const el = scrollRef.current!;
    const rect = el.getBoundingClientRect();
    const toT = (cx: number) => (cx - rect.left + el.scrollLeft) / zoom;
    onSeek(toT(e.clientX));
    const move = (ev: PointerEvent) => onSeek(toT(ev.clientX));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  // ---------- Clip video ----------
  const onVideoDown = (e: RPointerEvent, clip: VideoClip, i: number, kind: DragKind) => {
    if (e.button !== 0) return;
    const span = layout[i];
    const media = mediaOf(clip.media_id);
    const maxOut = media?.kind === "image" ? Infinity : media?.duration ?? clip.src_out;
    const clamp = (dxSec: number) => {
      if (kind === "l") return Math.min(Math.max(dxSec, -clip.src_in / clip.speed), span.duration - 0.1);
      if (kind === "r") return Math.min(Math.max(dxSec, -(span.duration - 0.1)), (maxOut - clip.src_out) / clip.speed);
      return dxSec;
    };
    trackDrag(
      e,
      (dx) => setDrag({ id: clip.id, kind, dx: clamp(dx / zoom) }),
      (dx, moved) => {
        setDrag(null);
        if (!moved) {
          onSelect({ kind: "clip", id: clip.id });
          return;
        }
        const d = clamp(dx / zoom);
        if (kind === "l") apply([{ op: "trim_clip", clip_id: clip.id, src_in: clip.src_in + d * clip.speed }]);
        else if (kind === "r") apply([{ op: "trim_clip", clip_id: clip.id, src_out: clip.src_out + d * clip.speed }]);
        else {
          const center = span.start + span.duration / 2 + d;
          const to = layout.filter((s, j) => j !== i && s.start + s.duration / 2 < center).length;
          if (to !== i) apply([{ op: "move_clip", clip_id: clip.id, to_index: to }]);
        }
        onSelect({ kind: "clip", id: clip.id });
      },
    );
  };

  // ---------- Chữ ----------
  const onTextDown = (e: RPointerEvent, t: TextClip, kind: DragKind) => {
    if (e.button !== 0) return;
    const clamp = (d: number) => {
      if (kind === "move") return Math.max(d, -t.start);
      if (kind === "l") return Math.min(Math.max(d, -t.start), t.end - t.start - 0.1);
      return Math.max(d, -(t.end - t.start - 0.1));
    };
    trackDrag(
      e,
      (dx) => setDrag({ id: t.id, kind, dx: clamp(dx / zoom) }),
      (dx, moved) => {
        setDrag(null);
        onSelect({ kind: "text", id: t.id });
        if (!moved) return;
        const d = clamp(dx / zoom);
        const start = kind === "r" ? t.start : t.start + d;
        const end = kind === "l" ? t.end : t.end + d;
        apply([{ op: "update_text", text_id: t.id, start, end }]);
      },
    );
  };

  // ---------- Âm thanh ----------
  const onAudioDown = (e: RPointerEvent, a: AudioClip, kind: DragKind) => {
    if (e.button !== 0) return;
    const media = mediaOf(a.media_id);
    const maxOut = media?.duration ?? a.src_out;
    const clamp = (d: number) => {
      if (kind === "move") return Math.max(d, -a.start);
      if (kind === "l") return Math.min(Math.max(d, -a.src_in, -a.start), a.src_out - a.src_in - 0.1);
      return Math.min(Math.max(d, -(a.src_out - a.src_in - 0.1)), maxOut - a.src_out);
    };
    trackDrag(
      e,
      (dx) => setDrag({ id: a.id, kind, dx: clamp(dx / zoom) }),
      (dx, moved) => {
        setDrag(null);
        onSelect({ kind: "audio", id: a.id });
        if (!moved) return;
        const d = clamp(dx / zoom);
        if (kind === "move") apply([{ op: "set_audio_props", audio_id: a.id, start: a.start + d }]);
        else if (kind === "l") apply([{ op: "set_audio_props", audio_id: a.id, start: a.start + d, src_in: a.src_in + d }]);
        else apply([{ op: "set_audio_props", audio_id: a.id, src_out: a.src_out + d }]);
      },
    );
  };

  /** Vị trí hiển thị (đã cộng phần đang kéo). */
  const box = (id: string, start: number, len: number) => {
    if (!drag || drag.id !== id) return { left: start * zoom, width: len * zoom };
    if (drag.kind === "move") return { left: (start + drag.dx) * zoom, width: len * zoom };
    if (drag.kind === "l") return { left: (start + drag.dx) * zoom, width: (len - drag.dx) * zoom };
    return { left: start * zoom, width: (len + drag.dx) * zoom };
  };

  const clsFor = (kind: string, id: string, base: string) =>
    [base, isSel(kind, id) ? "selected" : "", affected.includes(id) ? "claude" : "", drag?.id === id ? "dragging" : ""].join(" ");

  // Bước chia vạch thước tuỳ mức zoom.
  const step = [0.5, 1, 2, 5, 10, 15, 30, 60, 120].find((s) => s * zoom >= 70) ?? 300;
  const ticks: number[] = [];
  for (let t = 0; t <= width / zoom; t += step) ticks.push(t);

  const laneProps = (lane: string) => ({
    "data-lane": lane,
    className: `lane ${dragging && hoverLane === lane ? "drop-target" : ""}`,
    onPointerEnter: () => setHoverLane(lane),
    onPointerLeave: () => setHoverLane((h) => (h === lane ? null : h)),
    onPointerDown: (e: RPointerEvent) => {
      if (e.target === e.currentTarget) {
        onSelect(null);
        seekFrom(e);
      }
    },
  });

  const voices = project.audio.filter((a) => a.role === "voice");
  const music = project.audio.filter((a) => a.role === "music");

  const renderAudio = (a: AudioClip) => {
    const b = box(a.id, a.start, a.src_out - a.src_in);
    const m = mediaOf(a.media_id);
    return (
      <div
        key={a.id}
        className={clsFor("audio", a.id, `clip ${a.role}`)}
        style={b}
        onPointerDown={(e) => onAudioDown(e, a, "move")}
        title={m?.name}
      >
        <div className="handle l" onPointerDown={(e) => onAudioDown(e, a, "l")} />
        {m?.name ?? "?"}
        {a.duck ? " · tự giảm khi có tiếng nói" : ""}
        <div className="handle r" onPointerDown={(e) => onAudioDown(e, a, "r")} />
      </div>
    );
  };

  return (
    <section className="timeline">
      <div className="tl-toolbar">
        <button onClick={props.onSplit} title="Tách clip tại playhead (S)">
          <Icon name="scissors" /> Tách
        </button>
        <button onClick={props.onRemove} disabled={!selection} title="Xoá (Delete)">
          <Icon name="trash" /> Xoá
        </button>
        <button
          onClick={() => {
            const id = `text_${Math.random().toString(16).slice(2, 10)}`;
            apply([{ op: "add_text", id, text: "Chữ mới", start: time, end: time + 3 }]);
            onSelect({ kind: "text", id });
          }}
          title="Thêm chữ tại playhead"
        >
          <Icon name="text" /> Thêm chữ
        </button>
        <button onClick={props.onSpeech} title="Nhận dạng giọng nói, bỏ ừm/à, cắt khoảng lặng, phụ đề tự động">
          <Icon name="mic" /> Giọng nói &amp; phụ đề
        </button>
        <span className="muted small" style={{ marginLeft: 8, fontVariantNumeric: "tabular-nums" }}>
          {fmtTime(time)}
        </span>
        <div className="spacer" />
        <button
          className="ghost"
          onClick={() => {
            const w = scrollRef.current?.clientWidth ?? 800;
            if (duration > 0) setZoom(Math.min(300, Math.max(5, (w - 60) / duration)));
          }}
        >
          Vừa khung
        </button>
        <label className="row" style={{ width: 200 }}>
          Thu phóng
          <input type="range" min={5} max={300} value={zoom} onChange={(e) => setZoom(Number(e.target.value))} />
        </label>
      </div>
      <div className="tl-body">
        <div className="tl-heads">
          <div style={{ height: LANES.ruler }} />
          <div style={{ height: LANES.text }}>Chữ</div>
          <div style={{ height: LANES.video }}>Video</div>
          <div style={{ height: LANES.voice }}>Giọng đọc</div>
          <div style={{ height: LANES.music }}>Nhạc nền</div>
        </div>
        <div className="tl-scroll" ref={scrollRef}>
          <div className="tl-inner" style={{ width, height: totalH }}>
            <div className="ruler" onPointerDown={seekFrom}>
              {ticks.map((t) => (
                <div key={t} className="tick" style={{ left: t * zoom }}>
                  {fmtTime(t).replace(/\.00$/, "")}
                </div>
              ))}
            </div>

            <div {...laneProps("text")} style={{ height: LANES.text }}>
              {project.texts.map((t) => (
                <div key={t.id} className={clsFor("text", t.id, "clip text")} style={box(t.id, t.start, t.end - t.start)} onPointerDown={(e) => onTextDown(e, t, "move")} title={t.text}>
                  <div className="handle l" onPointerDown={(e) => onTextDown(e, t, "l")} />
                  {t.text}
                  <div className="handle r" onPointerDown={(e) => onTextDown(e, t, "r")} />
                </div>
              ))}
            </div>

            <div {...laneProps("video")} style={{ height: LANES.video }}>
              {project.video.map((c, i) => {
                const span = layout[i];
                const media = mediaOf(c.media_id);
                const b = box(c.id, span.start, span.duration);
                const fx = [
                  c.color.preset !== "none" ? PRESET_LABEL[c.color.preset] : "",
                  c.motion_blur > 0 ? "Motion blur" : "",
                  c.speed !== 1 ? `${c.speed}×` : "",
                ]
                  .filter(Boolean)
                  .join(" · ");
                return (
                  <div
                    key={c.id}
                    className={clsFor("clip", c.id, "clip video")}
                    style={{ ...b, backgroundImage: media?.thumb ? `url("${fileUrl(media.thumb)}")` : undefined }}
                    onPointerDown={(e) => onVideoDown(e, c, i, "move")}
                    title={media?.name}
                  >
                    <div className="handle l" onPointerDown={(e) => onVideoDown(e, c, i, "l")} />
                    <span className="label">{media?.name ?? "?"}</span>
                    {fx && <span className="fx">{fx}</span>}
                    {clipKeyframeTimes(c).map((t, k) => (
                      <span key={k} className="kf" style={{ left: t * zoom }} title={`keyframe ${t.toFixed(2)}s`} />
                    ))}
                    <div className="handle r" onPointerDown={(e) => onVideoDown(e, c, i, "r")} />
                  </div>
                );
              })}
              {project.video.map((c, i) =>
                i > 0 && layout[i].transition > 0 ? (
                  <div key={`tr-${c.id}`} className="transition-mark" style={{ left: (layout[i].start + layout[i].transition / 2) * zoom }} />
                ) : null,
              )}
            </div>

            <div {...laneProps("voice")} style={{ height: LANES.voice }}>
              {voices.map(renderAudio)}
            </div>
            <div {...laneProps("music")} style={{ height: LANES.music }}>
              {music.map(renderAudio)}
            </div>

            <div className="playhead" style={{ left: time * zoom }} />
          </div>
        </div>
      </div>
    </section>
  );
}
