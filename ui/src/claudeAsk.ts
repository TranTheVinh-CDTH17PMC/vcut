// Gửi yêu cầu sang Claude desktop: copy câu lệnh rồi bật Claude desktop lên.
// Người dùng chỉ cần Ctrl+V và Enter. (Không thể tự gõ vào app khác thay người dùng.)

import { api } from "./api";

const PREFIX = "[vlogcut] ";

export async function askClaude(request: string, toast: (t: string, e?: boolean) => void): Promise<void> {
  const prompt = PREFIX + request.trim();
  let copied = true;
  try {
    await navigator.clipboard.writeText(prompt);
  } catch {
    copied = false;
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
