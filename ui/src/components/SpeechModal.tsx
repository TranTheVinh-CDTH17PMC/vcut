// Hộp thoại "Giọng nói": cài Whisper, nhận dạng lời nói, dọn "ừm à"/khoảng lặng, tạo phụ đề.

import { useEffect, useState } from "react";
import {
  api,
  listen,
  type CleanupOptions,
  type InstallProgress,
  type SpeechStatus,
  type SubtitleOptions,
  type TranscribeProgress,
} from "../api";
import type { Project, ProjectView } from "../types";

interface Props {
  project: Project;
  onView: (v: ProjectView) => void;
  onClose: () => void;
  toast: (t: string, e?: boolean) => void;
}

const MB = (b: number) => `${(b / 1e6).toFixed(0)} MB`;

export function SpeechModal({ project, onView, onClose, toast }: Props) {
  const [st, setSt] = useState<SpeechStatus | null>(null);
  const [install, setInstall] = useState<InstallProgress | null>(null);
  const [trans, setTrans] = useState<TranscribeProgress | null>(null);
  const [language, setLanguage] = useState("vi");
  const [clean, setClean] = useState({ fillers: true, pauses: true, maxPause: 0.7, words: "" });
  const [sub, setSub] = useState<SubtitleOptions>({
    karaoke: true,
    align: "bottom",
    size: null,
    highlight_color: "#FFD60A",
    color: "#FFFFFF",
    max_chars: null,
    hide_fillers: true,
  });
  const [busy, setBusy] = useState(false);

  const refresh = () => api.speechStatus().then(setSt).catch((e) => toast(String(e), true));
  useEffect(() => {
    refresh();
    const un = [
      listen<InstallProgress>("speech-install-progress", (e) => setInstall(e.payload)),
      listen<{ ok: boolean; error?: string }>("speech-install-done", (e) => {
        setInstall(null);
        if (e.payload.ok) toast("Đã cài xong Whisper.");
        else if (e.payload.error) toast(e.payload.error, true);
        refresh();
      }),
      listen<TranscribeProgress>("speech-progress", (e) => setTrans(e.payload)),
      listen<{ ok: number; errors: string[] }>("speech-done", (e) => {
        setTrans(null);
        if (e.payload.errors.length) toast(e.payload.errors.join("\n"), true);
        else if (e.payload.ok) toast(`Đã nhận dạng ${e.payload.ok} file.`);
        refresh();
      }),
    ];
    return () => un.forEach((u) => u.then((f) => f()));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const ready = !!st?.tools.cli && !!st?.tools.model;
  // File có tiếng đang dùng trên timeline.
  const usedIds = new Set(project.video.map((c) => c.media_id));
  const used = project.media.filter((m) => usedIds.has(m.id) && m.has_audio && m.kind === "video");
  const done = used.filter((m) => m.transcribed).length;

  const transcribe = (force = false) =>
    api
      .speechTranscribe(null, language, force)
      .then(() => setTrans({ done: 0, total: used.length, name: "", pct: 0 }))
      .catch((e) => toast(String(e), true));

  const run = async (f: () => Promise<ProjectView>) => {
    setBusy(true);
    try {
      onView(await f());
      toast("Đã tạo đề xuất. Xem khung vàng ở cột phải rồi bấm Áp dụng.");
      onClose();
    } catch (e) {
      toast(String(e), true);
    } finally {
      setBusy(false);
    }
  };
  const doClean = () => {
    const opts: CleanupOptions = {
      remove_fillers: clean.fillers,
      max_pause: clean.pauses ? clean.maxPause : null,
      remove_words: clean.words.split(",").map((s) => s.trim()).filter(Boolean),
      clip_ids: [],
    };
    run(() => api.speechCleanup(opts));
  };

  const sizeGuess = Math.round(Math.min(project.settings.width, project.settings.height) * (project.settings.height > project.settings.width ? 0.075 : 0.06));

  return (
    <div className="modal-back" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" style={{ width: "min(640px, 94vw)", maxHeight: "92vh", overflowY: "auto" }}>
        <h2>Giọng nói</h2>

        {/* 1. Cài đặt */}
        <section className="box">
          <div className="row between">
            <strong>1. Whisper (nhận dạng giọng nói, chạy trên máy)</strong>
            {ready && <span style={{ color: "#34A574", fontSize: 12 }}>Đã cài · {st?.tools.gpu ? "GPU" : "CPU"}</span>}
          </div>
          {!ready && !install && (
            <>
              <span className="muted small" style={{ lineHeight: 1.6 }}>
                Cần tải một lần (khoảng {st?.nvidia ? "850 MB, bản chạy GPU NVIDIA" : "580 MB, bản chạy CPU"}). Video không bị gửi đi đâu.
              </span>
              <div className="row">
                <button className="primary" onClick={() => api.speechInstall().then(() => setInstall({ phase: "whisper", done: 0, total: 0 })).catch((e) => toast(String(e), true))}>
                  Tải Whisper
                </button>
              </div>
            </>
          )}
          {install && (
            <>
              <span className="muted small">
                {install.phase === "whisper" ? "Đang tải Whisper" : install.phase === "model" ? "Đang tải model ngôn ngữ" : "Đang giải nén"}
                {install.total > 0 && ` · ${MB(install.done)} / ${MB(install.total)}`}
              </span>
              <div className="progress">
                <div style={{ width: `${install.total ? (install.done / install.total) * 100 : 5}%` }} />
              </div>
              <div className="row">
                <button onClick={() => api.speechCancel()}>Huỷ</button>
              </div>
            </>
          )}
        </section>

        {/* 2. Nhận dạng */}
        <section className="box" style={{ opacity: ready ? 1 : 0.5 }}>
          <div className="row between">
            <strong>2. Nhận dạng lời nói</strong>
            <span className="muted small">
              {done}/{used.length} file trên timeline
            </span>
          </div>
          {trans ? (
            <>
              <span className="muted small">
                File {Math.min(trans.done + 1, trans.total)}/{trans.total} {trans.name && `· ${trans.name}`} · {Math.round(trans.pct * 100)}%
              </span>
              <div className="progress">
                <div style={{ width: `${trans.total ? ((trans.done + trans.pct) / trans.total) * 100 : 0}%` }} />
              </div>
              <div className="row">
                <button onClick={() => api.speechCancel()}>Dừng</button>
              </div>
            </>
          ) : (
            <div className="row" style={{ flexWrap: "wrap" }}>
              <label className="row">
                Ngôn ngữ
                <select value={language} onChange={(e) => setLanguage(e.target.value)}>
                  <option value="vi">Tiếng Việt</option>
                  <option value="en">English</option>
                  <option value="auto">Tự nhận</option>
                </select>
              </label>
              <button className="primary" disabled={!ready || used.length === 0 || done === used.length} onClick={() => transcribe(false)}>
                Nhận dạng {used.length - done} file
              </button>
              {done > 0 && (
                <button disabled={!ready} onClick={() => transcribe(true)}>
                  Làm lại tất cả
                </button>
              )}
            </div>
          )}
          {used.length === 0 && <span className="muted small">Kéo video có tiếng nói lên timeline trước.</span>}
        </section>

        {/* 3. Dọn lời nói */}
        <section className="box" style={{ opacity: done ? 1 : 0.5 }}>
          <strong>3. Dọn lời nói</strong>
          <label className="row" style={{ color: "var(--text)", fontSize: 13 }}>
            <input type="checkbox" checked={clean.fillers} onChange={(e) => setClean({ ...clean, fillers: e.target.checked })} />
            Bỏ tiếng đệm "ừm, à, ờ"
          </label>
          <label className="row" style={{ color: "var(--text)", fontSize: 13 }}>
            <input type="checkbox" checked={clean.pauses} onChange={(e) => setClean({ ...clean, pauses: e.target.checked })} />
            Cắt khoảng lặng giữa câu dài hơn
            <select value={clean.maxPause} disabled={!clean.pauses} onChange={(e) => setClean({ ...clean, maxPause: Number(e.target.value) })}>
              {[0.4, 0.5, 0.7, 1, 1.5].map((v) => (
                <option key={v} value={v}>
                  {v} giây
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            Cắt các từ (cách nhau bằng dấu phẩy)
            <input value={clean.words} placeholder="vd: vãi, kiểu như là" onChange={(e) => setClean({ ...clean, words: e.target.value })} style={{ userSelect: "text" }} />
          </label>
          <div className="row">
            <button className="primary" disabled={!done || busy} onClick={doClean}>
              Tạo đề xuất dọn lời
            </button>
          </div>
        </section>

        {/* 4. Phụ đề */}
        <section className="box" style={{ opacity: done ? 1 : 0.5 }}>
          <strong>4. Phụ đề tự động</strong>
          <div className="row" style={{ flexWrap: "wrap" }}>
            <label className="row">
              Vị trí
              <select value={sub.align} onChange={(e) => setSub({ ...sub, align: e.target.value as SubtitleOptions["align"] })}>
                <option value="bottom">Dưới</option>
                <option value="center">Giữa</option>
                <option value="top">Trên</option>
              </select>
            </label>
            <label className="row">
              Cỡ chữ
              <input type="number" min={24} max={200} value={sub.size ?? sizeGuess} onChange={(e) => setSub({ ...sub, size: Number(e.target.value) || null })} style={{ width: 70 }} />
            </label>
            <label className="row">
              Màu
              <input type="color" value={sub.color} onChange={(e) => setSub({ ...sub, color: e.target.value.toUpperCase() })} />
            </label>
          </div>
          <label className="row" style={{ color: "var(--text)", fontSize: 13 }}>
            <input type="checkbox" checked={sub.karaoke} onChange={(e) => setSub({ ...sub, karaoke: e.target.checked })} />
            Chữ chạy theo giọng, tô màu
            <input type="color" value={sub.highlight_color} disabled={!sub.karaoke} onChange={(e) => setSub({ ...sub, highlight_color: e.target.value.toUpperCase() })} />
          </label>
          <label className="row" style={{ color: "var(--text)", fontSize: 13 }}>
            <input type="checkbox" checked={sub.hide_fillers} onChange={(e) => setSub({ ...sub, hide_fillers: e.target.checked })} />
            Không ghi "ừm, à" lên phụ đề
          </label>
          <div className="row">
            <button className="primary" disabled={!done || busy} onClick={() => run(() => api.speechSubtitles(sub))}>
              Tạo phụ đề
            </button>
          </div>
          <span className="muted small" style={{ lineHeight: 1.5 }}>
            Nên dọn lời nói và áp dụng trước, rồi mới tạo phụ đề. Tạo lại phụ đề sẽ thay phụ đề cũ.
          </span>
        </section>

        <div className="row" style={{ justifyContent: "flex-end" }}>
          <button onClick={onClose}>Đóng</button>
        </div>
      </div>
    </div>
  );
}
