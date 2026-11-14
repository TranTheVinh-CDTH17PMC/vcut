import type { Proposal } from "../types";

export function ProposalPanel({ proposal, onAccept, onDismiss }: { proposal: Proposal; onAccept: () => void; onDismiss: () => void }) {
  const d = proposal.diff;
  return (
    <div className="proposal">
      <div className="row">
        <span style={{ width: 8, height: 8, borderRadius: "50%", background: "var(--claude)" }} />
        <strong>Claude đề xuất {d.lines.length} thay đổi</strong>
      </div>
      {proposal.description && <span style={{ color: "#fcd9a0", fontSize: 12 }}>{proposal.description}</span>}
      {proposal.notes && <div className="claude-msg-text" style={{ fontSize: 12 }}>{proposal.notes}</div>}
      <ul>
        {d.lines.slice(0, 12).map((l, i) => (
          <li key={i}>{l}</li>
        ))}
        {d.lines.length > 12 && <li>… và {d.lines.length - 12} thay đổi khác</li>}
      </ul>
      <span className="small" style={{ color: "#e8c894" }}>
        Thời lượng: {d.duration_before.toFixed(1)}s → {d.duration_after.toFixed(1)}s. Clip viền cam là phần sẽ đổi.
      </span>
      <div className="row">
        <button className="primary" style={{ flex: 1 }} onClick={onAccept}>
          Áp dụng
        </button>
        <button style={{ flex: 1 }} onClick={onDismiss}>
          Bỏ qua
        </button>
      </div>
    </div>
  );
}
