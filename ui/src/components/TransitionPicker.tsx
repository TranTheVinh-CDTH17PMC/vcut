// Bảng chọn chuyển cảnh kiểu CapCut: tab theo nhóm, lưới ô có hình xem thử
// chuyển động (rê chuột vào ô để xem), thời lượng, áp dụng cho mọi clip.

import { useEffect, useRef, useState, type CSSProperties } from "react";
import { mergeStyle, transitionStyles } from "../transitionPreview";
import type { TransitionInfo } from "../types";

/** Ô xem thử: hai khối màu A → B, chạy hiệu ứng khi rê chuột (hoặc khi đang chọn). */
function Thumb({ id, active }: { id: string; active: boolean }) {
  const [p, setP] = useState(0.35);
  const raf = useRef(0);
  useEffect(() => {
    if (!active) {
      setP(0.35);
      return;
    }
    const t0 = performance.now();
    const loop = (now: number) => {
      // 0.25 s đứng yên, 0.9 s chạy hiệu ứng, 0.35 s đứng ở clip sau.
      const x = ((now - t0) / 1500) % 1;
      setP(Math.min(1, Math.max(0, (x - 0.17) / 0.6)));
      raf.current = requestAnimationFrame(loop);
    };
    raf.current = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf.current);
  }, [active]);
  const st = transitionStyles(id, p);
  const layer: CSSProperties = { position: "absolute", inset: 0, display: "grid", placeItems: "center", fontWeight: 700, fontSize: 13 };
  return (
    <div className="tr-thumb">
      <div style={mergeStyle({ ...layer, background: "linear-gradient(135deg,#2563eb,#1e3a8a)", color: "#c7d7ff" }, st.out)}>A</div>
      <div style={mergeStyle({ ...layer, background: "linear-gradient(135deg,#f59e0b,#b45309)", color: "#fff7e6" }, st.in)}>B</div>
    </div>
  );
}

interface Props {
  list: TransitionInfo[];
  groups: string[];
  value: string | null;
  duration: number;
  onPick: (id: string | null) => void;
  onDuration: (d: number) => void;
  onApplyAll: () => void;
}

export function TransitionPicker({ list, groups, value, duration, onPick, onDuration, onApplyAll }: Props) {
  const current = list.find((t) => t.id === value);
  const [group, setGroup] = useState(current?.group ?? groups[0] ?? "");
  const [hover, setHover] = useState<string | null>(null);
  const items = list.filter((t) => t.group === group);
  // Kéo thanh trượt chỉ đổi số hiển thị; thả chuột mới ghi (tránh tạo hàng chục bước hoàn tác).
  const [d, setD] = useState(duration);
  useEffect(() => setD(duration), [duration]);
  const commit = () => d !== duration && onDuration(d);

  return (
    <div className="tr-picker">
      <div className="row between">
        <span className="small">
          Chuyển cảnh vào: <b>{current?.name ?? "Không"}</b>
        </span>
        {value && (
          <button className="ghost small" onClick={() => onPick(null)}>
            Bỏ
          </button>
        )}
      </div>
      <div className="tr-tabs">
        {groups.map((g) => (
          <button key={g} className={g === group ? "on" : ""} onClick={() => setGroup(g)}>
            {g}
          </button>
        ))}
      </div>
      <div className="tr-grid">
        {items.map((t) => (
          <button
            key={t.id}
            className={`tr-item${t.id === value ? " on" : ""}`}
            title={t.name}
            onMouseEnter={() => setHover(t.id)}
            onMouseLeave={() => setHover(null)}
            onClick={() => onPick(t.id)}
          >
            <Thumb id={t.id} active={hover === t.id || (hover === null && t.id === value)} />
            <span>{t.name}</span>
          </button>
        ))}
      </div>
      {value && (
        <div className="row" style={{ gap: 8, alignItems: "center" }}>
          <span className="small muted" style={{ whiteSpace: "nowrap" }}>
            Thời lượng
          </span>
          <input
            type="range"
            min={0.1}
            max={2}
            step={0.05}
            value={d}
            onChange={(e) => setD(Number(e.target.value))}
            onPointerUp={commit}
            onKeyUp={commit}
            style={{ flex: 1 }}
          />
          <span className="small" style={{ width: 40, textAlign: "right" }}>
            {d.toFixed(2)}s
          </span>
        </div>
      )}
      {value && (
        <button className="ghost small" onClick={onApplyAll}>
          Áp dụng cho mọi clip
        </button>
      )}
    </div>
  );
}
