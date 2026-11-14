import type { AppInfo } from "../types";
import { Icon } from "./Icon";

export function Welcome({ info, onOpen }: { info: AppInfo | null; onOpen: (dir?: string) => void }) {
  return (
    <div className="welcome">
      <div className="card">
        <div className="row">
          <div className="logo">
            <Icon name="play" size={14} />
          </div>
          <h1>vlogcut</h1>
        </div>
        <p className="muted" style={{ margin: 0, lineHeight: 1.6 }}>
          Chọn một thư mục làm project. Nếu thư mục chưa có project, app sẽ tạo mới. Video gốc có thể nằm ở bất kỳ đâu
          (kể cả ổ ngoài), app chỉ lưu thumbnail, bản nhẹ và file project trong thư mục này.
        </p>
        {info && !info.ffmpeg.found && (
          <div className="warn">
            Không tìm thấy <b>FFmpeg</b>. Mở PowerShell chạy <code>winget install Gyan.FFmpeg</code>, rồi tắt và mở lại app.
          </div>
        )}
        {info?.ffmpeg.found && (
          <div className="muted small">
            {info.ffmpeg.encoder === "nvenc" ? "Xuất video bằng GPU NVIDIA (NVENC)" : "Xuất video bằng CPU (không thấy NVENC)"} ·{" "}
            {info.ffmpeg.version.split(" ").slice(0, 3).join(" ")}
          </div>
        )}
        <div className="row">
          <button className="primary" onClick={() => onOpen()}>
            <Icon name="folder" /> Mở hoặc tạo project
          </button>
          {info?.last_project && (
            <button onClick={() => onOpen(info.last_project!)} title={info.last_project}>
              Mở lại project gần nhất
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
