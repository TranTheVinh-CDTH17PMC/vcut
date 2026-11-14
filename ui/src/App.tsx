import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, listen, type AnalysisProgress, type ExportDone, type ImportProgress } from "./api";
import type { AppInfo, MediaItem, Op, ProjectView, Selection } from "./types";
import { clipIndexAt, projectDuration, videoLayout } from "./timeline";
import { MediaLibrary } from "./components/MediaLibrary";
import { Preview } from "./components/Preview";
import { Timeline } from "./components/Timeline";
import { Inspector } from "./components/Inspector";
import { ProposalPanel } from "./components/ProposalPanel";
import { ExportModal, VideoModal } from "./components/Modals";
import { Welcome } from "./components/Welcome";
import { Icon } from "./components/Icon";
import { AutoEdit } from "./components/AutoEdit";
import { ClaudeChip, ClaudeModal } from "./components/ClaudeConnect";
import { ClaudePanel } from "./components/ClaudePanel";
import { SpeechModal } from "./components/SpeechModal";

type Tab = "edit" | "auto" | "replicate";
interface Toast {
  id: number;
  text: string;
  error?: boolean;
}
export interface MediaDrag {
  media: MediaItem;
  x: number;
  y: number;
}

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [view, setView] = useState<ProjectView | null>(null);
  const [tab, setTab] = useState<Tab>("edit");
  const [selection, setSelection] = useState<Selection>(null);
  const [time, setTime] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [zoom, setZoom] = useState(60); // pixel mỗi giây
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [importing, setImporting] = useState<ImportProgress | null>(null);
  const [analysis, setAnalysis] = useState<AnalysisProgress | null>(null);
  const [exportState, setExportState] = useState<{ pct: number; done?: ExportDone } | null>(null);
  const [previewUrl, setPreviewUrl] = useState<{ path: string; busy: boolean; pct: number } | null>(null);
  const [mediaDrag, setMediaDrag] = useState<MediaDrag | null>(null);
  const [claudeOpen, setClaudeOpen] = useState(false);
  const [speechOpen, setSpeechOpen] = useState(false);

  const project = view?.project ?? null;
  const layout = useMemo(() => (project ? videoLayout(project) : []), [project]);
  const duration = useMemo(() => (project ? projectDuration(project, layout) : 0), [project, layout]);

  // Giữ bản mới nhất trong ref để các listener không bị "đóng băng" giá trị cũ.
  const viewRef = useRef(view);
  viewRef.current = view;

  const toast = useCallback((text: string, error = false) => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, error }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), error ? 8000 : 3500);
  }, []);

  // ---- Khởi động ----
  useEffect(() => {
    api.appInfo().then(setInfo).catch((e) => toast(String(e), true));
  }, [toast]);

  // ---- Sự kiện từ Rust ----
  useEffect(() => {
    let timer: number | undefined;
    const unsubs = [
      listen("project-changed", () => {
        // Gom nhiều sự kiện liên tiếp thành một lần tải lại.
        window.clearTimeout(timer);
        timer = window.setTimeout(() => {
          if (!viewRef.current) return;
          api
            .get()
            .then((v) => {
              const cur = viewRef.current;
              if (
                !cur ||
                v.project.revision !== cur.project.revision ||
                v.proposal?.id !== cur.proposal?.id ||
                v.claude_message?.id !== cur.claude_message?.id
              ) {
                const newProposal = v.proposal && v.proposal.id !== cur?.proposal?.id;
                const newMessage = v.claude_message && v.claude_message.id !== cur?.claude_message?.id;
                if (newProposal) toast(`Claude vừa đề xuất: ${v.proposal!.description}`);
                else if (newMessage) toast("Claude vừa gửi nhận xét, xem ở cột phải.");
                // Có kết quả từ Claude thì chuyển về màn hình dựng để thấy ngay.
                if (newProposal || newMessage) setTab("edit");
                setView(v);
              }
            })
            .catch(() => {});
        }, 150);
      }),
      listen<ImportProgress>("import-progress", (e) => setImporting(e.payload)),
      listen("import-done", () => {
        setImporting(null);
        api.get().then(setView).catch(() => {});
      }),
      listen<AnalysisProgress>("analysis-progress", (e) => setAnalysis(e.payload)),
      listen("analysis-done", () => {
        setAnalysis(null);
        api.get().then(setView).catch(() => {});
      }),
      listen<{ name: string; error: string }>("import-error", (e) => toast(`Không nhập được ${e.payload.name}:\n${e.payload.error}`, true)),
      listen<number>("export-progress", (e) => setExportState((s) => (s ? { ...s, pct: e.payload } : s))),
      listen<ExportDone>("export-done", (e) => setExportState((s) => ({ pct: s?.pct ?? 0, done: e.payload }))),
      listen<number>("preview-progress", (e) => setPreviewUrl((p) => (p ? { ...p, pct: e.payload } : p))),
    ];
    return () => {
      window.clearTimeout(timer);
      unsubs.forEach((u) => u.then((f) => f()));
    };
  }, [toast]);

  // ---- Thao tác project ----
  const openProject = useCallback(
    async (dir?: string) => {
      try {
        const chosen = dir ?? (await openDialog({ directory: true, title: "Chọn thư mục project" }));
        if (!chosen || Array.isArray(chosen)) return;
        const v = await api.open(chosen);
        setView(v);
        setSelection(null);
        setTime(0);
      } catch (e) {
        toast(String(e), true);
      }
    },
    [toast],
  );

  const apply = useCallback(
    async (ops: Op[]) => {
      const cur = viewRef.current;
      if (!cur || ops.length === 0) return;
      try {
        setView(await api.apply(ops, cur.project.revision));
      } catch (e) {
        const msg = String(e);
        toast(msg, true);
        // Xung đột (Claude vừa sửa): tải lại bản mới nhất.
        if (msg.includes("đã thay đổi")) api.get().then(setView).catch(() => {});
      }
    },
    [toast],
  );

  const undo = useCallback(() => api.undo().then(setView).catch((e) => toast(String(e), true)), [toast]);
  const redo = useCallback(() => api.redo().then(setView).catch((e) => toast(String(e), true)), [toast]);

  const importPaths = useCallback(
    (paths: string[]) => {
      if (paths.length) api.importMedia(paths).catch((e) => toast(String(e), true));
    },
    [toast],
  );

  const startExport = useCallback(async () => {
    if (!project) return;
    if (project.video.length === 0) return toast("Timeline chưa có clip nào", true);
    const out = await saveDialog({
      title: "Lưu video",
      defaultPath: `${project.name}.mp4`,
      filters: [{ name: "Video MP4", extensions: ["mp4"] }],
    });
    if (!out) return;
    setPlaying(false);
    setExportState({ pct: 0 });
    api.exportStart(out).catch((e) => {
      setExportState(null);
      toast(String(e), true);
    });
  }, [project, toast]);

  const previewClip = useCallback(
    async (clipId: string) => {
      setPreviewUrl({ path: "", busy: true, pct: 0 });
      try {
        const path = await api.previewClip(clipId);
        setPreviewUrl({ path, busy: false, pct: 1 });
      } catch (e) {
        setPreviewUrl(null);
        toast(String(e), true);
      }
    },
    [toast],
  );

  // ---- Phát video: đồng hồ chạy theo requestAnimationFrame ----
  useEffect(() => {
    if (!playing) return;
    let last = performance.now();
    let raf = 0;
    const tick = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      setTime((t) => {
        const next = t + dt;
        if (next >= duration) {
          setPlaying(false);
          return duration;
        }
        return next;
      });
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing, duration]);

  useEffect(() => {
    if (time > duration) setTime(duration);
  }, [duration, time]);

  // ---- Tách / xoá ----
  const splitAtPlayhead = useCallback(() => {
    if (!project) return;
    let i = selection?.kind === "clip" ? project.video.findIndex((c) => c.id === selection.id) : -1;
    if (i < 0 || time < layout[i].start || time > layout[i].start + layout[i].duration) i = clipIndexAt(layout, time);
    if (i < 0) return;
    apply([{ op: "split_clip", clip_id: project.video[i].id, at: time - layout[i].start }]);
  }, [project, selection, time, layout, apply]);

  const removeSelected = useCallback(() => {
    if (!selection) return;
    const op = selection.kind === "clip" ? "remove_clip" : selection.kind === "text" ? "remove_text" : "remove_audio";
    const key = selection.kind === "clip" ? "clip_id" : selection.kind === "text" ? "text_id" : "audio_id";
    apply([{ op, [key]: selection.id }]);
    setSelection(null);
  }, [selection, apply]);

  // ---- Phím tắt ----
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (!project || tab !== "edit") return;
      const fps = project.settings.fps;
      if (e.code === "Space") {
        e.preventDefault();
        if (!playing && time >= duration - 0.01) setTime(0);
        setPlaying((p) => !p);
      } else if (e.key === "s" || e.key === "S") {
        if (!e.ctrlKey) splitAtPlayhead();
      } else if (e.key === "Delete" || e.key === "Backspace") {
        removeSelected();
      } else if (e.ctrlKey && (e.key === "z" || e.key === "Z")) {
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
      } else if (e.ctrlKey && (e.key === "y" || e.key === "Y")) {
        e.preventDefault();
        redo();
      } else if (e.key === "ArrowLeft") {
        setTime((t) => Math.max(0, t - (e.shiftKey ? 1 : 1 / fps)));
      } else if (e.key === "ArrowRight") {
        setTime((t) => Math.min(duration, t + (e.shiftKey ? 1 : 1 / fps)));
      } else if (e.key === "Home") {
        setTime(0);
      } else if (e.key === "End") {
        setTime(duration);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [project, tab, playing, time, duration, splitAtPlayhead, removeSelected, undo, redo]);

  // ---- Kéo media từ thư viện xuống timeline (dùng pointer, không dùng HTML5 drag) ----
  const dropRef = useRef<(m: MediaItem, lane: string, t: number) => void>(() => {});
  useEffect(() => {
    if (!mediaDrag) return;
    const move = (e: PointerEvent) => setMediaDrag((d) => (d ? { ...d, x: e.clientX, y: e.clientY } : d));
    const up = (e: PointerEvent) => {
      const el = document.elementFromPoint(e.clientX, e.clientY)?.closest("[data-lane]") as HTMLElement | null;
      if (el) {
        const rect = el.getBoundingClientRect();
        const t = Math.max(0, (e.clientX - rect.left) / zoom);
        dropRef.current(mediaDrag.media, el.dataset.lane!, t);
      }
      setMediaDrag(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mediaDrag !== null, zoom]);

  dropRef.current = (m, lane, t) => {
    if (!project) return;
    if (lane === "video") {
      if (m.kind === "audio") return toast("Kéo file âm thanh vào track Giọng đọc hoặc Nhạc nền", true);
      const index = layout.filter((s) => s.start + s.duration / 2 < t).length;
      apply([{ op: "add_clip", media_id: m.id, index }]);
    } else if (lane === "music" || lane === "voice") {
      if (m.kind === "image") return toast("Ảnh không có âm thanh", true);
      if (m.kind === "video" && !m.has_audio) return toast("Video này không có âm thanh", true);
      apply([{ op: "add_audio", media_id: m.id, start: t, role: lane, duck: lane === "music" }]);
    }
  };

  const addToTimeline = useCallback(
    (m: MediaItem) => {
      if (m.kind === "audio") apply([{ op: "add_audio", media_id: m.id, start: 0, role: "music", duck: true, fade_out: 2 }]);
      else apply([{ op: "add_clip", media_id: m.id }]);
    },
    [apply],
  );

  if (!view || !project) {
    return (
      <div className="app">
        <Welcome info={info} onOpen={openProject} />
        <Toasts toasts={toasts} />
      </div>
    );
  }

  const affected = view.proposal?.diff.affected ?? [];

  return (
    <div className="app">
      <header className="topbar">
        <div className="logo">
          <Icon name="play" size={14} />
        </div>
        <strong>vlogcut</strong>
        <span className="muted">/</span>
        <span className="name">{project.name}</span>
        <nav className="tabs">
          <button className={tab === "edit" ? "on" : ""} onClick={() => setTab("edit")}>
            Dựng thủ công
          </button>
          <button className={tab === "auto" ? "on" : ""} onClick={() => setTab("auto")}>
            Tự dựng
          </button>
          <button className={tab === "replicate" ? "on" : ""} onClick={() => setTab("replicate")}>
            Bắt chước video mẫu
          </button>
        </nav>
        <div className="spacer" />
        <ClaudeChip onClick={() => setClaudeOpen(true)} />
        <button className="icon" title="Hoàn tác (Ctrl+Z)" disabled={!view.can_undo} onClick={undo}>
          <Icon name="undo" />
        </button>
        <button className="icon" title="Làm lại (Ctrl+Shift+Z)" disabled={!view.can_redo} onClick={redo}>
          <Icon name="redo" />
        </button>
        <button className="ghost" title="Mở project khác" onClick={() => openProject()}>
          Mở project
        </button>
        <button className="primary" onClick={startExport}>
          <Icon name="download" /> Xuất MP4
        </button>
      </header>

      {tab === "edit" && (
        <>
          <div className="main">
            <MediaLibrary
              project={project}
              importing={importing}
              onImport={importPaths}
              onRemove={(id) => api.removeMedia(id).then(setView).catch((e) => toast(String(e), true))}
              onAdd={addToTimeline}
              onDragStart={(m, x, y) => setMediaDrag({ media: m, x, y })}
            />
            <section className="center">
              <Preview
                project={project}
                layout={layout}
                time={time}
                playing={playing}
                presets={info?.presets ?? []}
                selectedClipId={selection?.kind === "clip" ? selection.id : null}
              />
              <div className="transport">
                <span>{fmt(time)}</span>
                <span className="muted">/ {fmt(duration)}</span>
                <button className="icon ghost" title="Về đầu (Home)" onClick={() => setTime(0)}>
                  <Icon name="start" />
                </button>
                <button
                  className="play"
                  title="Phát / dừng (Space)"
                  onClick={() => {
                    if (!playing && time >= duration - 0.01) setTime(0);
                    setPlaying((p) => !p);
                  }}
                >
                  <Icon name={playing ? "pause" : "play"} size={16} />
                </button>
                <span className="muted small">
                  {project.settings.width}×{project.settings.height} · {project.settings.fps} fps
                </span>
              </div>
            </section>
            <aside className="side right">
              {view.proposal && (
                <ProposalPanel
                  proposal={view.proposal}
                  onAccept={() => api.acceptProposal().then(setView).catch((e) => toast(String(e), true))}
                  onDismiss={() => api.dismissProposal().then(setView).catch((e) => toast(String(e), true))}
                />
              )}
              <ClaudePanel
                project={project}
                layout={layout}
                selection={selection}
                message={view.claude_message}
                onDismissMessage={() => api.claudeMessageDismiss().then(setView).catch((e) => toast(String(e), true))}
                toast={toast}
              />
              <Inspector
                project={project}
                layout={layout}
                selection={selection}
                presets={info?.presets ?? []}
                time={time}
                apply={apply}
                onPreviewClip={previewClip}
              />
            </aside>
          </div>
          <Timeline
            project={project}
            layout={layout}
            duration={duration}
            time={time}
            zoom={zoom}
            setZoom={setZoom}
            selection={selection}
            affected={affected}
            dragging={!!mediaDrag}
            playing={playing}
            onSeek={(t) => setTime(Math.max(0, Math.min(duration, t)))}
            onSelect={setSelection}
            apply={apply}
            onSplit={splitAtPlayhead}
            onRemove={removeSelected}
            onSpeech={() => setSpeechOpen(true)}
          />
        </>
      )}

      {tab === "auto" && (
        <AutoEdit
          project={project}
          analysis={analysis}
          importing={!!importing}
          onView={setView}
          onBuilt={() => setTab("edit")}
          toast={toast}
        />
      )}
      {tab === "replicate" && (
        <div className="placeholder">
          <div className="card">
            <h2>Bắt chước video mẫu</h2>
            <p>Sắp có ở bản 2: phân tích nhịp cắt, màu và hiệu ứng của video mẫu rồi áp lên footage của bạn.</p>
          </div>
        </div>
      )}

      {mediaDrag && <div className="drag-ghost" style={{ left: mediaDrag.x, top: mediaDrag.y }}>{mediaDrag.media.name}</div>}
      {exportState && (
        <ExportModal
          state={exportState}
          onCancel={() => api.exportCancel()}
          onClose={() => setExportState(null)}
        />
      )}
      {previewUrl && <VideoModal state={previewUrl} onClose={() => setPreviewUrl(null)} />}
      {claudeOpen && <ClaudeModal onClose={() => setClaudeOpen(false)} toast={toast} />}
      {speechOpen && <SpeechModal project={project} onView={setView} onClose={() => setSpeechOpen(false)} toast={toast} />}
      <Toasts toasts={toasts} />
    </div>
  );
}

function fmt(s: number) {
  const m = Math.floor(s / 60);
  return `${m}:${(s - m * 60).toFixed(2).padStart(5, "0")}`;
}

function Toasts({ toasts }: { toasts: Toast[] }) {
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.error ? "error" : ""}`}>
          {t.text}
        </div>
      ))}
    </div>
  );
}
