// Màn hình "Tự dựng": xem kết quả phân tích cảnh, đánh dấu giữ/bỏ, điền yêu cầu dựng.

import { useEffect, useMemo, useRef, useState } from "react";
import { api, fileUrl, type AnalysisProgress } from "../api";
import { askClaude } from "../claudeAsk";
import { ASPECTS, fmtTime } from "../timeline";
import type { Brief, MediaItem, Project, ProjectView, Scene, SceneTag } from "../types";
import { Icon } from "./Icon";

const TAG: Record<SceneTag, { label: string; bg: string; fg: string; ring: string }> = {
  good: { label: "Đẹp", bg: "#14532D", fg: "#BBF7D0", ring: "#34A574" },
  speech: { label: "Có người nói", bg: "#1E3A5F", fg: "#BFDBFE", ring: "#3E5F94" },
  photo: { label: "Ảnh", bg: "#2A2F3A", fg: "#E6E8EE", ring: "#3A4150" },
  blurry: { label: "Mờ", bg: "#4C1D1D", fg: "#FECACA", ring: "#262B36" },
  dark: { label: "Tối", bg: "#4C1D1D", fg: "#FECACA", ring: "#262B36" },
  overexposed: { label: "Cháy sáng", bg: "#4C1D1D", fg: "#FECACA", ring: "#262B36" },
  shaky: { label: "Rung", bg: "#4C1D1D", fg: "#FECACA", ring: "#262B36" },
  duplicate: { label: "Trùng", bg: "#3A3A3A", fg: "#E5E5E5", ring: "#262B36" },
};
const DEFECTS: SceneTag[] = ["blurry", "dark", "overexposed", "shaky", "duplicate"];

const LENGTHS = [
  { label: "30 giây", v: 30 },
  { label: "1 phút", v: 60 },
  { label: "3 phút", v: 180 },
  { label: "Tuỳ ý", v: null },
];
const MOODS = [
  { id: "chill", label: "Chill" },
  { id: "hype", label: "Sôi động" },
  { id: "emotional", label: "Cảm xúc" },
  { id: "fun", label: "Vui nhộn" },
];

export const DEFAULT_BRIEF: Brief = {
  target_secs: 60,
  width: 1080,
  height: 1920,
  mood: "chill",
  music_media_id: null,
  chronological: true,
  place_titles: true,
  keep_speech: true,
  beat_sync: true,
  notes: "",
};

export function isKept(s: Scene): boolean {
  if (s.mark === "keep") return true;
  if (s.mark === "drop") return false;
  return !s.tags.some((t) => DEFECTS.includes(t));
}

interface Item {
  media: MediaItem;
  scene: Scene;
  day: string;
}

interface Props {
  project: Project;
  analysis: AnalysisProgress | null;
  importing: boolean;
  onView: (v: ProjectView) => void;
  onBuilt: () => void;
  toast: (t: string, error?: boolean) => void;
}

export function AutoEdit({ project, analysis, importing, onView, onBuilt, toast }: Props) {
  const [brief, setBrief] = useState<Brief>(DEFAULT_BRIEF);
  const [filter, setFilter] = useState<"all" | "keep" | "drop">("all");
  const [playing, setPlaying] = useState<Item | null>(null);
  const [busy, setBusy] = useState(false);
  const loaded = useRef(false);

  useEffect(() => {
    api
      .briefGet()
      .then((b) => {
        loaded.current = true;
        setBrief({ ...DEFAULT_BRIEF, ...b });
      })
      .catch(() => {});
  }, [project.name]);

  // Tự lưu yêu cầu sau khi người dùng ngừng sửa 0.6 giây.
  useEffect(() => {
    if (!loaded.current) return;
    const t = window.setTimeout(() => api.briefSave(brief).catch(() => {}), 600);
    return () => window.clearTimeout(t);
  }, [brief]);

  const visual = project.media.filter((m) => m.kind !== "audio");
  const music = project.media.filter((m) => m.kind === "audio");
  const analyzed = visual.filter((m) => m.analysis_version > 0).length;
  const totalDur = visual.reduce((s, m) => s + m.duration, 0);

  // Gom cảnh theo ngày quay.
  const groups = useMemo(() => {
    const items: Item[] = [];
    for (const m of visual) {
      const day = (m.created_at ?? m.modified_at ?? "").slice(0, 10);
      for (const s of m.scenes) items.push({ media: m, scene: s, day });
    }
    items.sort((a, b) => {
      const ka = (a.media.created_at ?? a.media.modified_at ?? "") + a.media.name;
      const kb = (b.media.created_at ?? b.media.modified_at ?? "") + b.media.name;
      return ka === kb ? a.scene.start - b.scene.start : ka < kb ? -1 : 1;
    });
    const map = new Map<string, Item[]>();
    for (const it of items) {
      const key = it.day || "Không rõ ngày";
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(it);
    }
    return [...map.entries()];
  }, [visual]);

  const all = groups.flatMap(([, v]) => v);
  const keptCount = all.filter((i) => isKept(i.scene)).length;
  const days = groups.map(([d]) => d).filter((d) => d !== "Không rõ ngày");

  const step = visual.length === 0 ? 0 : analysis || importing || analyzed < visual.length ? 1 : 2;
  const steps = ["Nhập video", "Phân tích trên máy", "Dựng", "Duyệt & xuất"];

  const toggle = (s: Scene) => {
    const next = isKept(s) ? "drop" : "keep";
    api.sceneMark(s.id, next).then(onView).catch((e) => toast(String(e), true));
  };
  const reset = (s: Scene) => api.sceneMark(s.id, null).then(onView).catch((e) => toast(String(e), true));

  const askBuild = async () => {
    await api.briefSave(brief).catch(() => {});
    askClaude("Dựng vlog cho project đang mở theo yêu cầu đã lưu ở tab Tự dựng.", toast);
  };
  const quick = async () => {
    setBusy(true);
    try {
      onView(await api.autoeditQuick(brief));
      toast("Đã tạo bản dựng nhanh. Xem đề xuất ở tab Dựng thủ công, bấm Áp dụng để đưa lên timeline.");
      onBuilt();
    } catch (e) {
      toast(String(e), true);
    } finally {
      setBusy(false);
    }
  };

  const pill = (on: boolean) => ({ className: on ? "on" : "", style: { flex: 1, height: 34, fontSize: 12 } });
  const ratioIdx = ASPECTS.findIndex((a) => a.width === brief.width && a.height === brief.height);

  return (
    <div className="auto">
      <ol className="steps">
        {steps.map((label, i) => (
          <li key={label} className={i < step ? "done" : i === step ? "active" : ""}>
            <span className="dot">{i < step ? "✓" : i + 1}</span>
            {label}
          </li>
        ))}
      </ol>
      <div className="auto-body">
        <main className="auto-main">
          <div className="row" style={{ gap: 14, flexWrap: "wrap", alignItems: "stretch" }}>
            <section className="card-box" style={{ flex: "1 1 260px" }}>
              <div className="row">
                <Icon name="folder" />
                <h3 style={{ margin: 0 }}>{project.name}</h3>
              </div>
              <div className="stats">
                <div>
                  <b>{visual.length}</b>
                  <span>video, ảnh</span>
                </div>
                <div>
                  <b>{fmtTime(totalDur).replace(/\.\d+$/, "")}</b>
                  <span>tổng thời lượng</span>
                </div>
                <div>
                  <b>{days.length || "–"}</b>
                  <span>{days.length ? `${fmtDay(days[0])} – ${fmtDay(days[days.length - 1])}` : "ngày quay"}</span>
                </div>
              </div>
              <span className="muted small">Nhập thêm video ở tab Dựng thủ công hoặc kéo thả vào cửa sổ.</span>
            </section>
            <section className="card-box" style={{ flex: "2 1 380px" }}>
              <div className="row between">
                <h3 style={{ margin: 0 }}>Phân tích trên máy</h3>
                <span className="muted small">Không dùng lượt Claude</span>
              </div>
              {analysis ? (
                <>
                  <div className="progress">
                    <div style={{ width: `${analysis.total ? (analysis.done / analysis.total) * 100 : 0}%` }} />
                  </div>
                  <div className="row between small">
                    <span className="muted">
                      Xong {analysis.done}/{analysis.total} file {analysis.name && `· ${analysis.name}`}
                    </span>
                    <button onClick={() => api.analysisCancel()}>Dừng</button>
                  </div>
                </>
              ) : importing ? (
                <span className="muted">Đang nhập file, phân tích sẽ tự chạy khi nhập xong…</span>
              ) : (
                <>
                  <span className="muted">
                    Đã phân tích {analyzed}/{visual.length} file · {all.length} cảnh, giữ {keptCount}, loại {all.length - keptCount}.
                  </span>
                  <div className="row">
                    {analyzed < visual.length && (
                      <button className="primary" onClick={() => api.analysisStart(false).catch((e) => toast(String(e), true))}>
                        Phân tích {visual.length - analyzed} file còn lại
                      </button>
                    )}
                    {analyzed > 0 && (
                      <button onClick={() => api.analysisStart(true).catch((e) => toast(String(e), true))}>Phân tích lại tất cả</button>
                    )}
                  </div>
                </>
              )}
              <span className="muted small" style={{ lineHeight: 1.5 }}>
                Chấm điểm độ nét, độ sáng, độ rung, tìm cảnh trùng và đoán đoạn có người nói. Bấm vào cảnh để giữ/bỏ, bấm đúp để xem.
              </span>
            </section>
          </div>

          <section style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <div className="row" style={{ flexWrap: "wrap" }}>
              <h3 style={{ margin: "0 8px 0 0" }}>Cảnh đã phân tích</h3>
              {(
                [
                  ["all", `Tất cả · ${all.length}`],
                  ["keep", `Giữ lại · ${keptCount}`],
                  ["drop", `Bị loại · ${all.length - keptCount}`],
                ] as const
              ).map(([id, label]) => (
                <button key={id} className={`chip ${filter === id ? "on" : ""}`} onClick={() => setFilter(id)}>
                  {label}
                </button>
              ))}
            </div>
            {all.length === 0 && <div className="muted">Chưa có cảnh nào. Nhập video rồi chờ phân tích xong.</div>}
            {groups.map(([day, items], gi) => {
              const shown = items.filter((i) => filter === "all" || (filter === "keep") === isKept(i.scene));
              if (!shown.length) return null;
              return (
                <div key={day} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                  <div className="muted small">
                    {day === "Không rõ ngày" ? day : `Ngày ${gi + 1} · ${fmtDay(day)}`} · {shown.length} cảnh
                  </div>
                  <div className="scene-grid">
                    {shown.map((it) => (
                      <SceneCard key={it.scene.id} it={it} onToggle={toggle} onReset={reset} onPlay={setPlaying} />
                    ))}
                  </div>
                </div>
              );
            })}
          </section>
        </main>

        <aside className="auto-side">
          <h3 style={{ margin: 0 }}>Yêu cầu dựng</h3>
          <div className="field">
            <span className="muted small">Độ dài</span>
            <div className="row">
              {LENGTHS.map((l) => (
                <button key={l.label} {...pill(brief.target_secs === l.v)} onClick={() => setBrief({ ...brief, target_secs: l.v })}>
                  {l.label}
                </button>
              ))}
            </div>
          </div>
          <div className="field">
            <span className="muted small">Tỉ lệ khung hình</span>
            <div className="row">
              {ASPECTS.slice(0, 3).map((a, i) => (
                <button key={a.label} {...pill(ratioIdx === i)} onClick={() => setBrief({ ...brief, width: a.width, height: a.height })}>
                  {a.label}
                </button>
              ))}
            </div>
          </div>
          <div className="field">
            <span className="muted small">Mood</span>
            <div className="mood-grid">
              {MOODS.map((m) => (
                <button key={m.id} {...pill(brief.mood === m.id)} onClick={() => setBrief({ ...brief, mood: m.id })}>
                  {m.label}
                </button>
              ))}
            </div>
          </div>
          <label className="field">
            Nhạc nền
            <select value={brief.music_media_id ?? ""} onChange={(e) => setBrief({ ...brief, music_media_id: e.target.value || null })}>
              <option value="">{music.length ? "Tự chọn trong thư viện" : "Chưa có nhạc (nhập file mp3 vào thư viện)"}</option>
              {music.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name} · {fmtTime(m.duration).replace(/\.\d+$/, "")}
                </option>
              ))}
            </select>
          </label>
          <div className="field">
            <span className="muted small">Tuỳ chọn</span>
            {(
              [
                ["chronological", "Theo thứ tự thời gian chuyến đi"],
                ["place_titles", "Chữ tiêu đề và \"Ngày 1, 2…\""],
                ["keep_speech", "Giữ đoạn có người nói"],
                ["beat_sync", "Cắt khớp nhịp nhạc (khi dựng bằng Claude)"],
              ] as const
            ).map(([k, label]) => (
              <label key={k} className="row" style={{ color: "var(--text)", fontSize: 13, whiteSpace: "normal" }}>
                <input type="checkbox" checked={brief[k]} onChange={(e) => setBrief({ ...brief, [k]: e.target.checked })} />
                {label}
              </label>
            ))}
          </div>
          <label className="field">
            Ghi chú thêm cho Claude
            <textarea
              rows={3}
              value={brief.notes}
              placeholder="Ví dụ: mở đầu bằng cảnh săn mây, kết bằng hoàng hôn ở hồ"
              onChange={(e) => setBrief({ ...brief, notes: e.target.value })}
            />
          </label>
          <div className="claude-box">
            <span style={{ fontWeight: 600 }}>Dựng bằng Claude</span>
            <span className="small" style={{ lineHeight: 1.5 }}>
              Bấm nút: Claude desktop mở chat mới với lệnh điền sẵn, bạn chỉ cần bấm Enter. Kết quả hiện thành đề xuất ở tab Dựng thủ công. Claude chỉ xem ảnh thu nhỏ, không xem video gốc.
            </span>
            <button className="primary" style={{ background: "#B45309", borderColor: "#B45309" }} disabled={keptCount === 0} onClick={askBuild}>
              Nhờ Claude dựng
            </button>
          </div>
          <button style={{ height: 40 }} disabled={busy || keptCount === 0} onClick={quick}>
            {busy ? "Đang dựng…" : "Dựng nhanh (không cần Claude)"}
          </button>
          <span className="muted small" style={{ lineHeight: 1.5 }}>
            Dựng nhanh chọn cảnh điểm cao nhất của mỗi cảnh quay, sắp theo thời gian, thêm filter, chuyển cảnh, chữ và nhạc theo mood. Chưa cắt theo nhịp nhạc.
          </span>
        </aside>
      </div>
      {playing && <ScenePlayer it={playing} onClose={() => setPlaying(null)} />}
    </div>
  );
}

function fmtDay(d: string) {
  return d.length >= 10 ? `${d.slice(8, 10)}/${d.slice(5, 7)}` : d;
}

function SceneCard({ it, onToggle, onReset, onPlay }: { it: Item; onToggle: (s: Scene) => void; onReset: (s: Scene) => void; onPlay: (i: Item) => void }) {
  const s = it.scene;
  const kept = isKept(s);
  const main = s.tags.find((t) => DEFECTS.includes(t)) ?? s.tags.find((t) => t === "good") ?? s.tags[0];
  const t = main ? TAG[main] : null;
  const dur = it.media.kind === "image" ? null : s.end - s.start;
  const title = [
    `${it.media.name} · ${fmtTime(s.start)}–${fmtTime(s.end)}`,
    `Điểm ${s.score.toFixed(0)} · nét ${s.sharpness?.toFixed(0) ?? "–"} · sáng ${s.brightness?.toFixed(0) ?? "–"} · rung ${s.shake?.toFixed(2) ?? "–"}`,
    s.tags.map((x) => TAG[x].label).join(", "),
    kept ? "Bấm để loại" : "Bấm để giữ",
  ].join("\n");
  return (
    <div className="scene-card" style={{ opacity: kept ? 1 : 0.5 }} title={title}>
      <button
        className="scene-thumb"
        style={{ backgroundImage: s.thumb ? `url("${fileUrl(s.thumb)}")` : undefined, borderColor: kept ? t?.ring ?? "#3A4150" : "#262B36" }}
        onClick={() => onToggle(s)}
        onDoubleClick={() => it.media.kind === "video" && onPlay(it)}
        aria-label={`${kept ? "Loại" : "Giữ"} cảnh ${it.media.name} ${fmtTime(s.start)}`}
      >
        {t && (
          <span className="tag" style={{ background: t.bg, color: t.fg }}>
            {t.label}
          </span>
        )}
        {s.tags.includes("speech") && main !== "speech" && (
          <span className="tag" style={{ background: TAG.speech.bg, color: TAG.speech.fg, top: 24 }}>
            Có người nói
          </span>
        )}
        {s.mark && <span className="mark">{s.mark === "keep" ? "Bạn giữ" : "Bạn bỏ"}</span>}
        {dur !== null && <span className="dur">{fmtTime(dur).replace(/^0:0?/, "").replace(/\.(\d)\d$/, ".$1")}s</span>}
      </button>
      <div className="row between" style={{ gap: 4 }}>
        <span className="scene-name">
          {it.media.name}
          {dur !== null && ` · ${fmtTime(s.start).replace(/\.\d+$/, "")}`}
        </span>
        {s.mark && (
          <button className="ghost tiny" title="Trả về tự động" onClick={() => onReset(s)}>
            ↺
          </button>
        )}
      </div>
    </div>
  );
}

function ScenePlayer({ it, onClose }: { it: Item; onClose: () => void }) {
  const ref = useRef<HTMLVideoElement>(null);
  const { scene: s, media: m } = it;
  // Phát lặp đúng đoạn của cảnh.
  useEffect(() => {
    const v = ref.current;
    if (!v) return;
    const onTime = () => {
      if (v.currentTime >= s.end) v.currentTime = s.start;
    };
    v.addEventListener("timeupdate", onTime);
    return () => v.removeEventListener("timeupdate", onTime);
  }, [s]);
  return (
    <div className="modal-back" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal">
        <h2>
          {m.name} · {fmtTime(s.start)}–{fmtTime(s.end)}
        </h2>
        <video
          ref={ref}
          src={fileUrl(m.proxy ?? m.path)}
          controls
          autoPlay
          onLoadedMetadata={(e) => (e.currentTarget.currentTime = s.start)}
        />
        <span className="muted small">
          Điểm {s.score.toFixed(0)} · {s.tags.map((t) => TAG[t].label).join(", ") || "không có nhãn"}
        </span>
        <div className="row" style={{ justifyContent: "flex-end" }}>
          <button className="primary" onClick={onClose}>
            Đóng
          </button>
        </div>
      </div>
    </div>
  );
}
