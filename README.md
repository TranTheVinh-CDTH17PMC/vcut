# vlogcut

App dựng video chạy trên máy (Windows), kiểu CapCut, có Claude làm trợ lý dựng. Đặc tả đầy đủ ở [CLAUDE.md](CLAUDE.md).

## Đã có (chặng 0–5)

- Mở/tạo project trong một thư mục bất kỳ.
- Nhập video, ảnh, nhạc: kéo thả từ Explorer, chọn file hoặc cả thư mục. Tự đọc thông tin (thời lượng, độ phân giải, ngày quay, GPS), tạo thumbnail và **proxy 540p bằng GPU** để xem trước mượt.
- Timeline 4 track (Chữ, Video, Giọng đọc, Nhạc nền): kéo media xuống, kéo đổi thứ tự, kéo mép để cắt, tách (S), xoá (Delete), hoàn tác/làm lại (Ctrl+Z / Ctrl+Shift+Z).
- Filter màu (Cinematic, Vintage, Lạnh, Ấm, Đen trắng, Rực rỡ), độ sáng/tương phản/bão hoà, motion blur, tốc độ 0.25–4×, 8 kiểu chuyển cảnh, Ken Burns, chữ có hiệu ứng vào, nhạc nền có fade và **tự giảm khi có tiếng nói**.
- **Keyframe** cho vị trí, zoom, xoay, độ mờ, màu, âm lượng (giao diện chỉnh keyframe chi tiết ở bản 2; hiện chỉnh qua nút Ken Burns và sau này qua Claude).
- Xem trước trong app, "Render thử clip này" để xem chính xác hiệu ứng nặng, **xuất MP4 bằng NVENC** (tự chuyển sang CPU nếu không có GPU).
- **Phân tích cảnh trên máy** (tự chạy sau khi nhập): tách cảnh, chấm điểm độ nét/độ sáng/độ rung, tìm cảnh trùng, đoán đoạn có người nói. Nhãn: Đẹp, Mờ, Tối, Cháy sáng, Rung, Trùng, Có người nói. Cảnh trời/mây ít chi tiết không bị coi là mờ.
- **Màn hình Tự dựng**: lưới cảnh theo ngày quay, bấm để giữ/bỏ, bấm đúp để xem; form yêu cầu (độ dài, tỉ lệ, mood, nhạc, tuỳ chọn, ghi chú) tự lưu vào `brief.json`.
- **Dựng nhanh không cần Claude**: chọn cảnh tốt nhất của mỗi cảnh quay, sắp theo thời gian, cắt theo nhịp của mood, thêm filter, chuyển cảnh, chữ "Ngày 1, 2…", nhạc nền. Kết quả hiện thành đề xuất để duyệt rồi Áp dụng.

- **Kết nối Claude desktop** (MCP server `vlogcut-mcp`): Claude xem contact sheet các cảnh, đọc yêu cầu dựng, đo nhịp nhạc, rồi gửi **đề xuất** vào app để bạn bấm Áp dụng. Bấm chip "Kết nối Claude" trên thanh trên cùng để tự cấu hình.
- **Claude review bản dựng**: render nháp nhỏ, Claude xem dải khung hình của kết quả thật cùng báo cáo tự động (độ dài, nhịp dựng, cắt lệch nhịp nhạc, cảnh mờ/lặp, chữ khó đọc, âm lượng LUFS) rồi nhận xét và đề xuất sửa. Gõ trong Claude desktop: *"Review bản dựng hiện tại trong vlogcut rồi đề xuất cách sửa"*.

- **Giọng nói & phụ đề** (nút trên timeline): tải Whisper một lần (bản GPU NVIDIA), nhận dạng lời nói tiếng Việt trên máy, **bỏ "ừm, à, ờ"**, **cắt khoảng lặng**, **cắt theo từ**, **phụ đề tự động** có **chữ chạy theo giọng** (karaoke). Claude cũng đọc được lời nói để chọn câu hay và tự đề xuất dọn lời/phụ đề.

Sắp làm: xuất nhiều tỉ lệ + khung dọc bám người + khớp màu (chặng 6), âm thanh (chặng 7), hiểu chuyến đi + bắt chước video mẫu (chặng 8). Xem lộ trình trong CLAUDE.md.

## Dùng với Claude desktop

Lần đầu: bấm chip **Kết nối Claude** (góc trên bên phải) → **Kết nối**, rồi thoát hẳn Claude desktop (chuột phải icon ở khay hệ thống → Quit) và mở lại.

Hằng ngày, bạn ở trong vlogcut là chính:

1. Bấm một nút ở panel **Nhờ Claude** (cột phải): *Dựng vlog*, *Review & sửa*, *Khớp nhịp nhạc*, *Mở đầu hay hơn*; hoặc chọn một clip rồi bấm *Ấn tượng hơn / Cắt gọn / Đổi cảnh khác*; hoặc gõ yêu cầu vào ô rồi **Gửi**.
2. Claude desktop tự bật lên, lệnh đã được copy sẵn: bấm **Ctrl+V** rồi **Enter**. (Lần đầu Claude hỏi quyền dùng công cụ: chọn *Allow always*.)
3. Quay lại vlogcut: nhận xét của Claude hiện ở panel Nhờ Claude, đề xuất hiện ở khung vàng phía trên → **Áp dụng** hoặc **Bỏ qua**. Ctrl+Z để hoàn tác.

Nên giữ một cuộc trò chuyện Claude cho mỗi video để Claude nhớ ngữ cảnh.

Thử MCP server mà không cần Claude: `npx @modelcontextprotocol/inspector target\debug\vlogcut-mcp.exe`.

## Chạy lần đầu

Cần: Rust, Node.js, Visual Studio Build Tools (C++), FFmpeg trong PATH. Mở **PowerShell** tại thư mục project:

```powershell
cd D:\Project\vlogcut
npm install
npm --prefix ui install
cargo build -p vlogcut-mcp
npm run dev
```

Lần đầu Rust phải biên dịch khá nhiều thư viện, mất khoảng 5–10 phút. Các lần sau chỉ vài giây.

### Tạo file cài đặt (.exe)

```powershell
npm run build
```

File cài nằm ở `target\release\bundle\nsis\`.

### Kiểm tra phần lõi

```powershell
cargo test -p vlogcut-core
```

Có một test xuất video thật bằng FFmpeg (mất khoảng 30 giây).

### Chuyến đi mẫu để thử màn hình Tự dựng

```powershell
cargo run -p vlogcut-core --example demo_trip -- D:\Project\vlogcut-trip
```

Tạo 10 clip qua 2 ngày (có cảnh mờ, tối, rung, trùng), nhập và phân tích sẵn. Thêm `build` ở cuối lệnh để dựng nhanh và xuất luôn video.

### Project mẫu để thử nhanh

```powershell
cargo run -p vlogcut-core --example demo_project -- D:\Project\vlogcut-demo
```

Tạo sẵn video mẫu, thư viện và timeline. Mở app, chọn thư mục `D:\Project\vlogcut-demo`.

## Cấu trúc

```
crates/core/   Lõi Rust: model, keyframe, thao tác timeline, lưu & undo, media, render FFmpeg
src-tauri/     App desktop Tauri 2: lệnh cho giao diện, theo dõi file, sự kiện tiến độ
ui/            Giao diện React + TypeScript
```

## Sự cố thường gặp

- **"Không tìm thấy ffmpeg"**: chạy `winget install Gyan.FFmpeg`, rồi **tắt hẳn PowerShell và mở lại** để PATH được cập nhật.
- **`link.exe not found`** khi build: thiếu Visual Studio Build Tools, chạy lại lệnh cài Build Tools với workload C++.
- **Build chậm hoặc lỗi khoá file**: không để project trong OneDrive.
- **Video quay bằng iPhone (HEVC) không xem trước được**: app xem trước bằng proxy H.264 nên sẽ chạy sau khi proxy tạo xong (xem tiến độ ở cột trái).
