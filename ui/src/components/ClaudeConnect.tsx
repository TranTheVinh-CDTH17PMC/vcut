// Chip trạng thái Claude trên thanh trên cùng + hộp thoại kết nối Claude desktop.

import { useEffect, useState } from "react";
import { api, type ClaudeStatus } from "../api";

/** Claude gọi tool trong vòng 30 phút gần đây thì coi là "đang dùng". */
function recent(iso?: string | null) {
  if (!iso) return false;
  return Date.now() - Date.parse(iso) < 30 * 60 * 1000;
}

function clock(iso: string) {
  const d = new Date(iso);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

export function ClaudeChip({ onClick }: { onClick: () => void }) {
  const [st, setSt] = useState<ClaudeStatus | null>(null);
  useEffect(() => {
    const load = () => api.claudeStatus().then(setSt).catch(() => {});
    load();
    const t = window.setInterval(load, 5000);
    return () => window.clearInterval(t);
  }, []);
  let dot = "#4b5563";
  let label = "Kết nối Claude";
  if (st?.configured) {
    dot = recent(st.last_call) ? "#F59E0B" : "#3B82F6";
    label = recent(st.last_call) ? `Claude đang dùng · ${clock(st.last_call!)}` : "Claude đã cài";
  }
  return (
    <button className="claude-chip" onClick={onClick} title="Kết nối với Claude desktop">
      <span className="dot" style={{ background: dot }} />
      {label}
    </button>
  );
}

const EXAMPLES = [
  "Dựng vlog cho project đang mở trong vlogcut theo yêu cầu đã lưu",
  "Xem các cảnh trong vlogcut rồi gợi ý cảnh mở đầu và kết thúc hay nhất",
  "Review bản dựng hiện tại trong vlogcut rồi đề xuất cách sửa",
  "Cắt lại timeline vlogcut cho khớp nhịp bài nhạc, giữ nguyên thứ tự",
  "Thêm zoom chậm vào cảnh hoàng hôn và chữ \"Hẹn gặp lại Đà Lạt\" ở cuối",
];

export function ClaudeModal({ onClose, toast }: { onClose: () => void; toast: (t: string, e?: boolean) => void }) {
  const [st, setSt] = useState<ClaudeStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [ext, setExt] = useState<{ path: string; opened: boolean } | null>(null);
  useEffect(() => {
    api.claudeStatus().then(setSt).catch((e) => toast(String(e), true));
  }, [toast]);

  const connect = async () => {
    setBusy(true);
    try {
      setSt(await api.claudeConnect());
      // Cài thêm dạng tiện ích (.mcpb): cách chính thức, chạy cả với bản Microsoft Store.
      const r = await api.claudeInstallExtension();
      setExt(r);
      toast(
        r.opened
          ? "Claude desktop đang hiện hộp thoại cài vlogcut: bấm Install."
          : "Đã chọn sẵn file vlogcut.mcpb: kéo nó vào Claude desktop > Settings > Desktop app > Extensions.",
      );
    } catch (e) {
      toast(String(e), true);
    } finally {
      setBusy(false);
    }
  };
  const copy = (t: string) => navigator.clipboard.writeText(t).then(() => toast("Đã copy")).catch(() => {});

  return (
    <div className="modal-back" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" style={{ width: "min(620px, 94vw)" }}>
        <h2>Kết nối Claude desktop</h2>
        <p className="muted" style={{ margin: 0, lineHeight: 1.6 }}>
          vlogcut dùng Claude qua ứng dụng <b>Claude desktop</b> trên máy bạn (không tốn thêm tiền API). Claude chỉ xem ảnh thu nhỏ và
          số liệu cảnh, không xem video gốc. Mọi thay đổi của Claude hiện thành <b>đề xuất</b> để bạn duyệt.
        </p>
        <ol style={{ margin: 0, paddingLeft: 18, lineHeight: 1.8, color: "var(--text-2)" }}>
          <li>
            Bấm <b>Kết nối</b> bên dưới. {st?.configured && <span style={{ color: "#34A574" }}>Đã kết nối ✓</span>}
          </li>
          <li>
            Claude desktop hiện hộp thoại cài tiện ích <b>vlogcut</b>: bấm <b>Install</b>. Nếu không thấy hộp thoại, mở Claude desktop &gt;{" "}
            <b>Settings &gt; Desktop app &gt; Extensions</b> rồi kéo file <code>vlogcut.mcpb</code> vào đó.
            {ext && (
              <>
                {" "}
                <a href="#" onClick={(e) => (e.preventDefault(), api.claudeReveal(ext.path))}>
                  Mở thư mục chứa file
                </a>
              </>
            )}
          </li>
          <li>
            Kiểm tra: trong trang Extensions có <b>vlogcut</b> đang bật. Lần đầu Claude hỏi quyền dùng công cụ, chọn <b>Allow always</b>.
          </li>
          <li>Mở một cuộc trò chuyện mới trong Claude desktop và gõ yêu cầu, ví dụ:</li>
        </ol>
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {EXAMPLES.map((ex) => (
            <div key={ex} className="row between box" style={{ padding: "6px 10px", flexDirection: "row", gap: 8 }}>
              <span style={{ fontSize: 12, userSelect: "text" }}>{ex}</span>
              <button className="ghost" style={{ height: 26 }} onClick={() => copy(ex)}>
                Copy
              </button>
            </div>
          ))}
        </div>
        {st && !st.mcp_found && (
          <div className="warn">
            Chưa có <code>vlogcut-mcp.exe</code>. Chạy lại <b>kiem-tra-build.bat</b> (đã build kèm MCP server) rồi mở lại app.
          </div>
        )}
        {st?.last_call && (
          <span className="muted small">
            Claude dùng vlogcut lần cuối lúc {new Date(st.last_call).toLocaleString("vi-VN")} ({st.last_tool}).
          </span>
        )}
        <span className="muted small" style={{ userSelect: "text", wordBreak: "break-all" }}>
          File cấu hình: {st?.config_path}
        </span>
        <div className="row" style={{ justifyContent: "flex-end" }}>
          <button onClick={onClose}>Đóng</button>
          <button className="primary" disabled={busy || !st?.mcp_found} onClick={connect}>
            {busy ? "Đang kết nối…" : st?.configured ? "Kết nối lại" : "Kết nối"}
          </button>
        </div>
      </div>
    </div>
  );
}
