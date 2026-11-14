import { fileUrl, type ExportDone } from "../api";

export function ExportModal({ state, onCancel, onClose }: { state: { pct: number; done?: ExportDone }; onCancel: () => void; onClose: () => void }) {
  const { pct, done } = state;
  return (
    <div className="modal-back">
      <div className="modal">
        <h2>{!done ? "Đang xuất video…" : done.ok ? "Xuất xong" : done.cancelled ? "Đã huỷ" : "Xuất lỗi"}</h2>
        {!done && (
          <>
            <div className="progress">
              <div style={{ width: `${pct * 100}%` }} />
            </div>
            <div className="row between">
              <span className="muted">{Math.round(pct * 100)}%</span>
              <button onClick={onCancel}>Huỷ</button>
            </div>
          </>
        )}
        {done?.ok && (
          <>
            <video src={fileUrl(done.path)} controls autoPlay />
            <span className="muted small" style={{ userSelect: "text" }}>
              Đã lưu: {done.path}
            </span>
          </>
        )}
        {done?.error && (
          <pre className="warn" style={{ whiteSpace: "pre-wrap", maxHeight: 260, overflow: "auto", userSelect: "text", margin: 0 }}>
            {done.error}
          </pre>
        )}
        {done && (
          <div className="row" style={{ justifyContent: "flex-end" }}>
            <button className="primary" onClick={onClose}>
              Đóng
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

export function VideoModal({ state, onClose }: { state: { path: string; busy: boolean; pct: number }; onClose: () => void }) {
  return (
    <div className="modal-back" onClick={(e) => e.target === e.currentTarget && !state.busy && onClose()}>
      <div className="modal">
        <h2>Render thử</h2>
        {state.busy ? (
          <>
            <div className="progress">
              <div style={{ width: `${state.pct * 100}%` }} />
            </div>
            <span className="muted">Đang render đúng như bản xuất (độ phân giải thấp)…</span>
          </>
        ) : (
          // ?v= để webview không dùng bản cũ trong cache.
          <video src={`${fileUrl(state.path)}?v=${Date.now()}`} controls autoPlay loop />
        )}
        <div className="row" style={{ justifyContent: "flex-end" }}>
          <button className="primary" disabled={state.busy} onClick={onClose}>
            Đóng
          </button>
        </div>
      </div>
    </div>
  );
}
