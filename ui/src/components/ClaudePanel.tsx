// Panel "Nhờ Claude" ở cột phải: lời nhắn Claude gửi vào app, nút yêu cầu sẵn,
// nút cho clip đang chọn và ô gõ yêu cầu tự do.

import { useState } from "react";
import { askClaude, getAskMode, setAskMode, type AskMode } from "../claudeAsk";
import { fmtTime, type Span } from "../timeline";
import type { ClaudeMessage, Project, Selection } from "../types";

const QUICK = [
  { label: "Dựng vlog", ask: "Dựng vlog cho project đang mở theo yêu cầu đã lưu ở tab Tự dựng." },
  { label: "Review & sửa", ask: "Review bản dựng hiện tại, ghi nhận xét vào app và đề xuất cách sửa." },
  { label: "Khớp nhịp nhạc", ask: "Chỉnh các điểm cắt cho khớp nhịp nhạc nền, giữ nguyên thứ tự cảnh." },
  { label: "Mở đầu hay hơn", ask: "Làm 3 giây mở đầu ấn tượng hơn: chọn cảnh mạnh nhất, thêm chữ tiêu đề và hiệu ứng phù hợp." },
];

const CLIP_ACTIONS = [
  { label: "Ấn tượng hơn", ask: "làm clip này ấn tượng hơn (zoom, màu, chuyển cảnh)" },
  { label: "Cắt gọn", ask: "cắt clip này gọn lại, chỉ giữ phần hay nhất" },
  { label: "Đổi cảnh khác", ask: "thay clip này bằng một cảnh khác hợp hơn trong thư viện" },
];

interface Props {
  project: Project;
  layout: Span[];
  selection: Selection;
  message?: ClaudeMessage | null;
  onDismissMessage: () => void;
  toast: (t: string, e?: boolean) => void;
}

export function ClaudePanel({ project, layout, selection, message, onDismissMessage, toast }: Props) {
  const [text, setText] = useState("");
  const [mode, setMode] = useState<AskMode>(getAskMode);
  const changeMode = (m: AskMode) => {
    setMode(m);
    setAskMode(m);
  };

  // Mô tả đối tượng đang chọn để Claude biết "clip này" là clip nào.
  let target = "";
  let targetLabel = "";
  if (selection?.kind === "clip") {
    const i = project.video.findIndex((c) => c.id === selection.id);
    if (i >= 0) {
      const c = project.video[i];
      const name = project.media.find((m) => m.id === c.media_id)?.name ?? "";
      target = `clip ${c.id} (${name}, giây ${layout[i].start.toFixed(1)}–${(layout[i].start + layout[i].duration).toFixed(1)} trên timeline)`;
      targetLabel = `${name} · ${fmtTime(layout[i].start)}`;
    }
  } else if (selection?.kind === "text") {
    const t = project.texts.find((x) => x.id === selection.id);
    if (t) {
      target = `chữ ${t.id} ("${t.text}", giây ${t.start.toFixed(1)}–${t.end.toFixed(1)})`;
      targetLabel = `Chữ "${t.text}"`;
    }
  } else if (selection?.kind === "audio") {
    const a = project.audio.find((x) => x.id === selection.id);
    if (a) {
      const name = project.media.find((m) => m.id === a.media_id)?.name ?? "";
      target = `âm thanh ${a.id} (${name})`;
      targetLabel = name;
    }
  }

  const send = (ask: string) => askClaude(ask, toast);
  const sendFree = () => {
    if (!text.trim()) return;
    send(target ? `Với ${target}: ${text.trim()}` : text.trim());
    setText("");
  };

  return (
    <div className="claude-panel">
      <div className="row between">
        <strong>Nhờ Claude</strong>
        <div className="seg" title="Chat mới: lệnh điền sẵn, chỉ bấm Enter. Chat đang mở: copy lệnh để dán vào chat cũ (Claude nhớ ngữ cảnh).">
          <button className={mode === "new" ? "on" : ""} onClick={() => changeMode("new")}>
            Chat mới
          </button>
          <button className={mode === "copy" ? "on" : ""} onClick={() => changeMode("copy")}>
            Chat đang mở
          </button>
        </div>
      </div>
      <span className="muted small">
        {mode === "new"
          ? "Bấm nút → Claude desktop mở chat mới, lệnh điền sẵn → bấm Enter."
          : "Bấm nút → lệnh được copy → dán vào chat đang mở (Ctrl+V, Enter)."}
      </span>

      {message && (
        <div className="claude-msg">
          <div className="claude-msg-text">{message.text}</div>
          <div className="row between">
            <span className="small" style={{ color: "#e8c894" }}>
              Claude · {new Date(message.at).toLocaleTimeString("vi-VN", { hour: "2-digit", minute: "2-digit" })}
            </span>
            <button className="ghost" style={{ height: 26 }} onClick={onDismissMessage}>
              Đã đọc
            </button>
          </div>
        </div>
      )}

      <div className="quick-grid">
        {QUICK.map((q) => (
          <button key={q.label} onClick={() => send(q.ask)}>
            {q.label}
          </button>
        ))}
      </div>

      {target && selection?.kind === "clip" && (
        <div className="field">
          <span className="muted small">Clip đang chọn: {targetLabel}</span>
          <div className="row" style={{ flexWrap: "wrap", gap: 6 }}>
            {CLIP_ACTIONS.map((a) => (
              <button key={a.label} className="chip" onClick={() => send(`Với ${target}: ${a.ask}.`)}>
                {a.label}
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="row" style={{ gap: 6 }}>
        <input
          style={{ flex: 1, minWidth: 0, userSelect: "text" }}
          placeholder={target ? `Yêu cầu cho ${targetLabel}…` : "Gõ yêu cầu, vd: thêm chữ cuối video"}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && sendFree()}
        />
        <button className="primary" onClick={sendFree} disabled={!text.trim()}>
          Gửi
        </button>
      </div>
    </div>
  );
}
