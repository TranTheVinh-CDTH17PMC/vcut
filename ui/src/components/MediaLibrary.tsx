import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { fileUrl, type ImportProgress } from "../api";
import type { MediaItem, Project } from "../types";
import { fmtTime } from "../timeline";
import { Icon } from "./Icon";

const EXT = ["mp4", "mov", "m4v", "mkv", "avi", "mts", "m2ts", "webm", "3gp", "jpg", "jpeg", "png", "webp", "bmp", "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"];

interface Props {
  project: Project;
  importing: ImportProgress | null;
  onImport: (paths: string[]) => void;
  onRemove: (mediaId: string) => void;
  onAdd: (m: MediaItem) => void;
  onDragStart: (m: MediaItem, x: number, y: number) => void;
}

export function MediaLibrary({ project, importing, onImport, onRemove, onAdd, onDragStart }: Props) {
  const [dropping, setDropping] = useState(false);

  // Kéo file/thư mục từ Explorer thả vào cửa sổ app.
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") setDropping(true);
      else if (p.type === "leave") setDropping(false);
      else if (p.type === "drop") {
        setDropping(false);
        onImport(p.paths);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [onImport]);

  const pickFiles = async () => {
    const r = await openDialog({ multiple: true, title: "Chọn video, ảnh, nhạc", filters: [{ name: "Media", extensions: EXT }] });
    if (r) onImport(Array.isArray(r) ? r : [r]);
  };
  const pickFolder = async () => {
    const r = await openDialog({ directory: true, title: "Chọn thư mục chứa video" });
    if (r && !Array.isArray(r)) onImport([r]);
  };

  const used = new Set([...project.video.map((c) => c.media_id), ...project.audio.map((a) => a.media_id)]);

  return (
    <aside className="side">
      <div className="section">
        <div className="row">
          <button style={{ flex: 1 }} onClick={pickFiles}>
            <Icon name="plus" /> Nhập file
          </button>
          <button style={{ flex: 1 }} onClick={pickFolder}>
            <Icon name="folder" /> Thư mục
          </button>
        </div>
        {importing ? (
          <div className="field">
            <div className="row between small">
              <span className="muted">{importing.phase === "scan" ? "Đọc thông tin" : "Tạo bản nhẹ (proxy)"}</span>
              <span>
                {importing.done}/{importing.total}
              </span>
            </div>
            <div className="progress">
              <div style={{ width: `${importing.total ? (importing.done / importing.total) * 100 : 0}%` }} />
            </div>
            <span className="muted small" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {importing.name}
            </span>
          </div>
        ) : (
          <span className="muted small">Kéo thả file hoặc thư mục vào cửa sổ. Kéo thẻ media xuống timeline, hoặc bấm đúp để thêm vào cuối.</span>
        )}
      </div>
      <div className="media-grid">
        {project.media.map((m) => (
          <div
            key={m.id}
            className="media-item"
            title={m.path}
            onDoubleClick={() => onAdd(m)}
            onPointerDown={(e) => {
              if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
              const sx = e.clientX;
              const sy = e.clientY;
              // Chỉ bắt đầu kéo khi chuột đã di chuyển một chút (để bấm đúp vẫn hoạt động).
              const move = (ev: PointerEvent) => {
                if (Math.hypot(ev.clientX - sx, ev.clientY - sy) > 6) {
                  cleanup();
                  onDragStart(m, ev.clientX, ev.clientY);
                }
              };
              const cleanup = () => {
                window.removeEventListener("pointermove", move);
                window.removeEventListener("pointerup", cleanup);
              };
              window.addEventListener("pointermove", move);
              window.addEventListener("pointerup", cleanup);
            }}
          >
            <div className="media-thumb" style={{ backgroundImage: m.thumb ? `url("${fileUrl(m.thumb)}")` : undefined }}>
              {!m.thumb && (
                <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--muted)" }}>
                  <Icon name={m.kind === "audio" ? "music" : m.kind === "image" ? "image" : "film"} size={22} />
                </div>
              )}
              {m.kind !== "image" && <span className="badge dur">{fmtTime(m.duration).replace(/\.\d+$/, "")}</span>}
              {used.has(m.id) && <span className="badge used">Đã dùng</span>}
              {m.kind === "video" && !m.proxy && <span className="badge" style={{ left: 4, bottom: 4 }}>chưa có proxy</span>}
              {!used.has(m.id) && (
                <button className="remove" title="Xoá khỏi thư viện" onClick={() => onRemove(m.id)}>
                  ×
                </button>
              )}
            </div>
            <span className="media-name">{m.name}</span>
          </div>
        ))}
      </div>
      {dropping && <div className="drop-overlay">Thả để nhập vào project</div>}
    </aside>
  );
}
