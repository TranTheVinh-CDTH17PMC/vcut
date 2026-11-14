// Gửi yêu cầu sang Claude desktop.
//
// Hai cách:
// - "new" (mặc định): mở chat mới bằng link claude://claude.ai/new?q=... — lệnh đã
//   nằm sẵn trong ô nhập, người dùng chỉ bấm Enter. Claude desktop không cho link
//   tự gửi lệnh (cơ chế an toàn), nên bước Enter là của người dùng.
// - "copy": copy lệnh rồi bật Claude desktop, để dán vào chat đang mở (Ctrl+V, Enter).
//   Dùng khi muốn Claude nhớ những gì đã trao đổi trong chat đó.
// Lệnh vẫn được copy trong cả hai cách, để lỡ link không chạy thì dán tay được.

import { api } from "./api";

const PREFIX = "[vlogcut] ";
const MODE_KEY = "vlogcut.claudeMode";

export type AskMode = "new" | "copy";

export function getAskMode(): AskMode {
  try {
    return localStorage.getItem(MODE_KEY) === "copy" ? "copy" : "new";
  } catch {
    return "new";
  }
}

export function setAskMode(mode: AskMode) {
  try {
    localStorage.setItem(MODE_KEY, mode);
  } catch {
    /* không lưu được thì thôi, lần sau dùng mặc định */
  }
}

export async function askClaude(request: string, toast: (t: string, e?: boolean) => void): Promise<void> {
  const prompt = PREFIX + request.trim();
  let copied = true;
  try {
    await navigator.clipboard.writeText(prompt);
  } catch {
    copied = false;
  }

  if (getAskMode() === "new") {
    const opened = await api.claudeOpenChat(prompt).catch(() => false);
    if (opened) {
      toast("Claude desktop đã mở chat mới, lệnh điền sẵn: bấm Enter. Kết quả sẽ hiện ở đây.");
      return;
    }
    // Link không chạy (Claude desktop cũ hoặc chưa cài): chuyển sang cách copy.
  }

  const opened = await api.claudeOpen().catch(() => false);
  if (!copied) {
    toast(`Không copy được. Hãy gõ trong Claude desktop:\n${prompt}`, true);
  } else if (opened) {
    toast("Đã copy lệnh và mở Claude desktop: bấm Ctrl+V rồi Enter. Kết quả sẽ hiện ở đây.");
  } else {
    toast("Đã copy lệnh. Mở Claude desktop, bấm Ctrl+V rồi Enter. Kết quả sẽ hiện ở đây.");
  }
}
