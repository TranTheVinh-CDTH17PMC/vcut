# vlogcut

App dựng video chạy hoàn toàn trên máy (Windows), dùng kiểu CapCut, có **Claude desktop** làm trợ lý dựng qua MCP. Miễn phí, không cần Claude API, không cần cài Python.

> Tên `vlogcut` là tên tạm. Đặc tả kỹ thuật đầy đủ (kiến trúc, model dữ liệu, quyết định đã chốt) ở [CLAUDE.md](CLAUDE.md).

**Mục lục**

1. [vlogcut làm được gì](#1-vlogcut-làm-được-gì)
2. [Cần chuẩn bị](#2-cần-chuẩn-bị)
3. [Cài đặt và chạy lần đầu](#3-cài-đặt-và-chạy-lần-đầu)
4. [Hướng dẫn sử dụng](#4-hướng-dẫn-sử-dụng)
5. [Dùng với Claude desktop](#5-dùng-với-claude-desktop)
6. [Giọng nói và phụ đề](#6-giọng-nói-và-phụ-đề)
7. [Phím tắt](#7-phím-tắt)
8. [Thư mục project và dữ liệu](#8-thư-mục-project-và-dữ-liệu)
9. [Cho người phát triển](#9-cho-người-phát-triển)
10. [Sự cố thường gặp](#10-sự-cố-thường-gặp)
11. [Lộ trình](#11-lộ-trình)

---

## 1. vlogcut làm được gì

Ba cách dựng, cùng một timeline:

| Cách | Mô tả |
|---|---|
| **Dựng thủ công** | Timeline kéo thả 4 track như CapCut: cắt, tách, filter, motion blur, zoom, chuyển cảnh, chữ, nhạc nền. |
| **Tự dựng** | Thả cả thư mục video đi chơi vào → app tự phân tích trên máy → chọn cảnh, cắt theo nhịp, thêm filter/chữ/nhạc. Dựng bằng thuật toán có sẵn hoặc nhờ Claude. |
| **Bắt chước video mẫu** | Sắp có (chặng 8). |

### Tính năng đã có (chặng 0–5)

**Nhập media**
- Mở/tạo project ở thư mục bất kỳ. Kéo thả từ Explorer, chọn file hoặc cả thư mục.
- Video, ảnh, nhạc. Tự đọc thời lượng, độ phân giải, fps, ngày quay, GPS.
- Tạo thumbnail và **proxy 540p bằng GPU** để xem trước mượt (video gốc có thể nằm trên ổ ngoài).

**Timeline**
- 4 track: Chữ, Video, Giọng đọc, Nhạc nền.
- Kéo media xuống, kéo đổi thứ tự, kéo mép để cắt, tách, xoá, hoàn tác/làm lại không giới hạn.

**Hiệu ứng**
- Filter màu: Cinematic, Vintage, Lạnh, Ấm, Đen trắng, Rực rỡ + chỉnh sáng/tương phản/bão hoà.
- Motion blur, tốc độ 0.25–4×, Ken Burns (zoom chậm).
- **68 kiểu chuyển cảnh** chia 6 nhóm như CapCut: *Động* (zoom xuyên, xoay, rung, lia nhanh 4 hướng, nhiễu sóng, chớp sáng), *Cơ bản* (mờ dần, hoà tan, qua màn đen/trắng…), *Trượt*, *Quét*, *Hình* (mở/đóng tròn, khung vuông…), *Hiệu ứng* (vỡ điểm ảnh, cắt lát, gió…). Rê chuột để xem thử, xem trước ngay trên khung hình, nút *Áp dụng cho mọi clip*.
- Chữ có hiệu ứng vào (mờ dần, trượt lên, bật), viền, màu, vị trí.
- Nhạc nền có fade vào/ra và **tự nhỏ lại khi có tiếng nói** (ducking).
- **Keyframe** cho vị trí, zoom, xoay, độ mờ, màu, âm lượng. Hiện tạo qua nút Ken Burns hoặc nhờ Claude ("zoom chậm vào từ giây 2 đến 4", "nhạc nhỏ dần ở cuối"); giao diện chỉnh keyframe chi tiết ở bản sau.

**Xem trước và xuất**
- Xem trước ngay trong app (gần đúng). Nút **Render thử clip này** để xem chính xác hiệu ứng nặng.
- **Xuất MP4 bằng NVENC** (card NVIDIA), tự chuyển sang CPU (x264) nếu không có. Âm lượng chuẩn hoá -14 LUFS như các nền tảng mạng xã hội.

**Phân tích cảnh (chạy trên máy, không dùng AI)**
- Tự chạy sau khi nhập: tách cảnh, chấm độ nét, độ sáng, độ rung, chuyển động; tìm cảnh trùng; đoán đoạn có người nói.
- Nhãn: **Đẹp, Mờ, Tối, Cháy sáng, Rung, Trùng, Có người nói**. Cảnh trời/mây ít chi tiết không bị coi là mờ.

**Tự dựng**
- Lưới cảnh theo ngày quay, bấm để giữ/bỏ, bấm đúp để xem.
- Form yêu cầu: độ dài, tỉ lệ khung (9:16, 16:9, 1:1), mood, nhạc, tuỳ chọn, ghi chú → tự lưu `brief.json`.
- **Dựng nhanh không cần Claude**: chọn cảnh tốt nhất mỗi cảnh quay, sắp theo thời gian, cắt theo nhịp nhạc, thêm filter, chuyển cảnh, chữ "Ngày 1, 2…", nhạc nền.

**Claude (qua Claude desktop)**
- Claude xem contact sheet các cảnh (ảnh ghép thumbnail, không xem video thô), đọc yêu cầu dựng, đo nhịp nhạc, đọc lời nói, rồi gửi **đề xuất** vào app.
- Mọi thay đổi của Claude hiện thành đề xuất, clip bị ảnh hưởng viền cam → bạn bấm **Áp dụng** hoặc **Bỏ qua**.
- **Review bản dựng**: Claude render bản nháp nhỏ, xem dải khung hình của kết quả thật cùng báo cáo tự động (độ dài so với yêu cầu, nhịp dựng, cắt lệch nhịp nhạc, cảnh mờ/lặp, chữ khó đọc/chồng nhau, âm lượng) rồi nhận xét và đề xuất sửa.
- Panel **Nhờ Claude** trong app: nút yêu cầu sẵn, không phải gõ lệnh.

**Giọng nói và phụ đề**
- Nhận dạng tiếng Việt trên máy bằng Whisper (GPU NVIDIA).
- **Bỏ "ừm, à, ờ"**, **cắt khoảng lặng**, **cắt theo từ**.
- **Phụ đề tự động** có **chữ chạy theo giọng** (karaoke).

---

## 2. Cần chuẩn bị

Máy tham khảo: laptop i5-13500HX, RTX 4050 6GB, RAM 16GB. Card NVIDIA không bắt buộc (sẽ chạy bằng CPU, chậm hơn).

| Phần mềm | Dùng để | Cài |
|---|---|---|
| **FFmpeg** (bản 7 trở lên) | Đọc/xử lý/xuất video | `winget install Gyan.FFmpeg` |
| **Rust** | Biên dịch app | https://rustup.rs |
| **Visual Studio Build Tools** (workload C++) | Trình liên kết cho Rust | `winget install Microsoft.VisualStudio.2022.BuildTools` rồi chọn *Desktop development with C++* |
| **Node.js** LTS | Build giao diện | `winget install OpenJS.NodeJS.LTS` |
| **Claude desktop** (tuỳ chọn) | Trợ lý dựng | https://claude.ai/download |

Whisper (cho giọng nói) **không cần cài tay**: app tự tải khi bạn dùng lần đầu.

Sau khi cài, **tắt hẳn PowerShell và mở lại** để PATH cập nhật. Kiểm tra:

```powershell
ffmpeg -version
cargo -V
node -v
```

---

## 3. Cài đặt và chạy lần đầu

```powershell
git clone https://github.com/TranTheVinh-CDTH17PMC/vcut.git D:\Project\vlogcut
cd D:\Project\vlogcut
npm install
npm --prefix ui install
cargo build -p vlogcut-mcp
npm run dev
```

Lần đầu Rust biên dịch nhiều thư viện, mất khoảng 5–10 phút. Các lần sau chỉ vài giây.

### Cài app để bấm là dùng (nên làm)

Bấm đúp **`cai-dat-app.bat`** trong thư mục project, một lần duy nhất. Script sẽ:

1. Build bản chạy thật (lần đầu khoảng 10 phút, các lần sau 1–3 phút).
2. Cài vào `%LOCALAPPDATA%\Programs\vlogcut` và tạo biểu tượng **vlogcut** trên Desktop và menu Start.
3. Bật **trình tự cập nhật** chạy nền (tự bật mỗi khi mở máy, không có cửa sổ).
4. Mở app.

Từ đó về sau bạn chỉ cần bấm biểu tượng **vlogcut**: không cần Terminal, không cần build.

**Tự cập nhật khi có code mới:** mỗi lần Claude code xong, Claude chép code vào thư mục project và tạo file `.deploy-request`. Trình tự cập nhật thấy file này sẽ tự build và cài bản mới (cũng cập nhật luôn MCP server cho Claude desktop). Nếu app đang mở, góc trên hiện nút **Có bản mới · Mở lại**: bấm là chuyển sang bản mới, project không mất gì.

- Muốn cập nhật tay: bấm đúp `cap-nhat-app.bat`.
- Kết quả lần cập nhật gần nhất: `deploy-status.json`, chi tiết ở `deploy-log.txt`.
- Tắt tự cập nhật: xoá `vlogcut-tu-dong-cap-nhat` trong thư mục Startup (Win+R → `shell:startup`).

### Các file .bat khác

| File | Làm gì |
|---|---|
| `cai-dat-app.bat` | Cài app + biểu tượng + tự cập nhật (xem trên). |
| `cap-nhat-app.bat` | Build và cài bản mới ngay, rồi mở app. |
| `chay-app.bat` | Mở app ở chế độ dev (cho người sửa code, có cửa sổ Terminal). |
| `kiem-tra-build.bat` | Kiểm tra công cụ, cài thư viện, build UI, chạy test lõi, build app + MCP. Kết quả ghi vào `build-log.txt` (xem `TEST_EXIT=0` và `BUILD_EXIT=0` là ổn). |

> Đừng để project trong OneDrive: build sẽ chậm và hay lỗi khoá file.

### Tạo file cài đặt (.exe)

```powershell
npm run build
```

File cài nằm ở `target\release\bundle\nsis\`.

---

## 4. Hướng dẫn sử dụng

Thanh trên cùng có 3 tab: **Dựng thủ công**, **Tự dựng**, **Bắt chước video mẫu** (sắp có). Góc phải có chip **Kết nối Claude**.

### 4.1. Mở project

Lần đầu mở app sẽ hỏi thư mục project. Chọn một thư mục trống (để tạo mới) hoặc thư mục đã có `project.json`. Nên đặt project trên SSD; video gốc ở đâu cũng được.

### 4.2. Dựng thủ công

```
┌────────────┬──────────────────────┬──────────────────┐
│ Thư viện   │      Xem trước       │ Thuộc tính clip  │
│ media      │                      │ Đề xuất Claude   │
│            │                      │ Nhờ Claude       │
├────────────┴──────────────────────┴──────────────────┤
│ Timeline: Chữ / Video / Giọng đọc / Nhạc nền          │
└───────────────────────────────────────────────────────┘
```

1. **Nhập**: kéo file/thư mục từ Explorer vào thư viện (cột trái), hoặc bấm nút nhập. Chờ proxy tạo xong (có tiến độ).
2. **Đưa lên timeline**: kéo video/ảnh xuống track Video, nhạc xuống track Nhạc nền.
3. **Chỉnh**: kéo clip để đổi thứ tự, kéo mép clip để cắt đầu/cuối, đặt playhead rồi bấm **S** để tách.
4. **Hiệu ứng**: chọn clip → cột phải chỉnh filter, sáng/tương phản/bão hoà, tốc độ, motion blur, Ken Burns, chuyển cảnh, âm lượng.
5. **Chữ**: thêm chữ trên track Chữ, sửa nội dung, cỡ, màu, viền, vị trí, hiệu ứng vào.
6. **Nhạc**: chọn clip nhạc → fade vào/ra, âm lượng, bật *tự giảm khi có tiếng nói*.
7. **Xem**: Space để phát. Clip có motion blur → bấm **Render thử clip này** để xem chính xác.
8. **Xuất**: bấm **Xuất MP4** (thanh trên cùng), chọn nơi lưu. App dùng NVENC nếu có.

Clip có keyframe hiện chấm nhỏ trên timeline.

### 4.3. Tự dựng

1. **Thư mục**: nhập cả thư mục video chuyến đi. Phân tích cảnh tự chạy sau khi nhập (xem tiến độ).
2. **Cảnh**: lưới cảnh theo ngày quay, mỗi cảnh có nhãn (Đẹp, Mờ, Tối…). Bấm để **giữ / bỏ**, bấm đúp để xem.
3. **Yêu cầu**: điền độ dài, tỉ lệ khung, mood, chọn nhạc, tuỳ chọn, ghi chú. Tự lưu.
4. **Dựng**, chọn một trong hai:
   - **Dựng nhanh**: app tự dựng bằng thuật toán, không cần Claude.
   - **Nhờ Claude dựng**: copy lệnh và mở Claude desktop (xem mục 5).

Kết quả hiện thành **đề xuất** ở tab Dựng thủ công → **Áp dụng** rồi chỉnh tay tiếp nếu muốn.

---

## 5. Dùng với Claude desktop

vlogcut có một MCP server (`vlogcut-mcp.exe`) để Claude desktop đọc và đề xuất thay đổi cho project đang mở. Không tốn tiền API: dùng gói Claude bạn đang có.

### 5.1. Kết nối (một lần)

1. Trong vlogcut, bấm chip **Kết nối Claude** (góc trên bên phải) → **Kết nối**.
   App chép `vlogcut-mcp.exe` sang `%APPDATA%\vlogcut\bin\` và thêm mục `vlogcut` vào `claude_desktop_config.json` (giữ nguyên các server khác, có lưu bản `.bak`).
   Sau đó Claude desktop hiện hộp thoại cài tiện ích **vlogcut**: bấm **Install**. Không thấy hộp thoại thì kéo file `vlogcut.mcpb` (app chọn sẵn trong Explorer) vào Claude desktop → **Settings → Desktop app → Extensions**.
2. **Thoát hẳn** Claude desktop (chuột phải icon ở khay hệ thống → Quit) rồi mở lại.
3. Chip chuyển sang trạng thái đã kết nối sau lần đầu Claude gọi công cụ.

### 5.2. Dùng hằng ngày

Bạn ở trong vlogcut là chính:

1. Ở panel **Nhờ Claude** (cột phải), bấm một nút:
   - Chung: *Dựng vlog*, *Review & sửa*, *Khớp nhịp nhạc*, *Mở đầu hay hơn*.
   - Theo clip đang chọn: *Ấn tượng hơn*, *Cắt gọn*, *Đổi cảnh khác*.
   - Hoặc gõ yêu cầu tự do vào ô rồi **Gửi**.
2. Claude desktop tự mở **chat mới** với lệnh (bắt đầu bằng `[vlogcut]`) đã điền sẵn trong ô nhập: chỉ cần bấm **Enter**. Lần đầu Claude hỏi quyền dùng công cụ: chọn *Allow always*.
   - Muốn hỏi tiếp trong chat đang mở (để Claude nhớ ngữ cảnh): chọn **Chat đang mở** ở góc panel Nhờ Claude. Khi đó lệnh được copy, bạn dán vào chat cũ bằng **Ctrl+V** rồi **Enter**.
   - Claude desktop không cho tự gửi lệnh từ bên ngoài, nên bước bấm Enter là của bạn.
3. Quay lại vlogcut: app tự chuyển về tab Dựng thủ công. Nhận xét của Claude hiện ở panel Nhờ Claude, đề xuất hiện ở khung phía trên → **Áp dụng** hoặc **Bỏ qua**. Ctrl+Z để hoàn tác.

Mẹo: mỗi lần mở chat mới Claude vẫn đọc lại toàn bộ project qua công cụ, nên không mất dữ liệu. Chỉ dùng **Chat đang mở** khi muốn nối tiếp một trao đổi dài.

### 5.3. Ví dụ yêu cầu

- "Dựng vlog 60 giây dọc 9:16, mood vui, cắt theo nhịp nhạc"
- "Review bản dựng hiện tại rồi đề xuất cách sửa"
- "Clip thứ 3 zoom chậm vào từ giây 2 đến 4"
- "Nhạc nhỏ dần trong 3 giây cuối"
- "Bỏ hết ừm à trong clip phỏng vấn, thêm phụ đề karaoke màu vàng"
- "Chọn câu nói hay nhất làm mở đầu"

### 5.4. Công cụ MCP Claude dùng

| Công cụ | Làm gì |
|---|---|
| `project_info` | Tổng quan project: media, timeline, revision, số clip đã nhận dạng giọng |
| `brief_get` | Đọc yêu cầu dựng từ tab Tự dựng |
| `scenes_list` | Contact sheet cảnh (24 cảnh/trang) + bảng điểm số |
| `music_beats` | Tempo, vị trí nhịp, độ tin cậy |
| `timeline_get` | Đọc timeline hiện tại |
| `timeline_preview` | Thử danh sách thao tác, xem diff, không ghi |
| `timeline_propose` | Gửi đề xuất (kèm ghi chú) để bạn duyệt trong app |
| `timeline_apply` | Áp dụng trực tiếp (cần `expected_revision`) |
| `timeline_review` | Render nháp 360p + dải khung hình + kiểm tra tự động |
| `transcript_get` | Lời nói theo từng từ có mốc thời gian |
| `speech_cleanup_propose` | Đề xuất bỏ tiếng đệm, khoảng lặng, từ cần cắt |
| `subtitles_propose` | Đề xuất phụ đề (có karaoke) |
| `show_in_app` | Gửi lời nhắn hiện trong panel Nhờ Claude |
| `render_export` | Xuất video |

Claude chỉ xem metadata, điểm số, transcript và ảnh contact sheet, không đọc video thô. Mọi tên file, lời nói, metadata được coi là dữ liệu, không phải lệnh.

Thử MCP server mà không cần Claude:

```powershell
npx @modelcontextprotocol/inspector target\debug\vlogcut-mcp.exe
```

---

## 6. Giọng nói và phụ đề

Bấm nút **Giọng nói & phụ đề** trên thanh timeline. Hộp thoại có 4 bước:

1. **Tải Whisper** (một lần): app tải `whisper-cli` bản dựng sẵn của whisper.cpp (bản GPU NVIDIA hoặc CPU) vào `%APPDATA%\vlogcut\whisper` và model `large-v3-turbo` (khoảng 550 MB) vào `%APPDATA%\vlogcut\models`. Không cần CUDA Toolkit.
2. **Nhận dạng**: chọn clip có tiếng nói → nhận dạng tiếng Việt theo từng từ. Có thể huỷ giữa chừng. Lỗi GPU thì tự chạy lại bằng CPU.
3. **Dọn lời**: bỏ tiếng đệm ("ừm, ờm" luôn bỏ; "à, ờ, ừ" chỉ bỏ khi đứng riêng, không phải câu hỏi), cắt khoảng lặng dài hơn ngưỡng (giữ lại 0.25 giây cho tự nhiên), nhập các từ/cụm từ muốn cắt. Kết quả là đề xuất tách/xoá clip, giữ nguyên hiệu ứng và keyframe.
4. **Phụ đề**: chọn vị trí, cỡ chữ, màu chữ, màu tô, số ký tự mỗi dòng, bật/tắt **karaoke** (chữ đổi màu theo giọng). Tạo lại sẽ thay phụ đề cũ.

Mọi bước đều ra đề xuất để duyệt, Ctrl+Z để hoàn tác.

---

## 7. Phím tắt

| Phím | Tác dụng |
|---|---|
| Space | Phát / dừng |
| S | Tách clip tại playhead |
| Delete | Xoá clip đang chọn |
| Ctrl+Z | Hoàn tác |
| Ctrl+Shift+Z | Làm lại |

---

## 8. Thư mục project và dữ liệu

```
<thư mục project>/
  project.json           # nguồn sự thật duy nhất: media, timeline, cài đặt
  proposal.json          # đề xuất chờ duyệt (nếu có)
  brief.json             # yêu cầu dựng từ tab Tự dựng
  claude-message.json    # lời nhắn của Claude hiện trong app
  .vlogcut/
    thumbs/              # thumbnail
    proxies/             # bản xem trước 540p
    contact/             # contact sheet cho Claude
    history/             # lịch sử để hoàn tác
    transcripts/         # lời nói đã nhận dạng
    renders/             # render thử, bản nháp review
    write.lock           # khoá ghi (app và Claude không ghi đè nhau)
```

Trong `%APPDATA%\vlogcut\`:

| File/thư mục | Nội dung |
|---|---|
| `current.json` | Project đang mở, để Claude biết làm việc với project nào |
| `mcp-status.json` | Lần cuối Claude gọi công cụ (app dùng để hiện trạng thái) |
| `bin\vlogcut-mcp.exe` | Bản MCP server Claude desktop chạy |
| `whisper\`, `models\` | Whisper và model nhận dạng giọng nói |

Video gốc **không bị sửa hay chép**. Xoá thư mục `.vlogcut` chỉ mất proxy/thumbnail/lịch sử (app sẽ tạo lại proxy).

---

## 9. Cho người phát triển

### Cấu trúc mã

```
Cargo.toml              workspace Rust
crates/core/            lõi thuần Rust, không phụ thuộc Tauri, test độc lập
  src/project.rs        model: Project, MediaItem, Scene, VideoClip, TextClip, AudioClip
  src/keyframe.rs       Animatable<T>, nội suy + easing
  src/ops.rs            enum Op (thao tác timeline), apply, diff dạng chữ
  src/store.rs          lưu project.json (ghi atomic, khoá file), undo/redo, đề xuất, brief, lời nhắn
  src/media.rs          ffprobe, thumbnail, proxy
  src/analysis.rs       tách cảnh, chấm điểm, dHash lọc trùng
  src/contact.rs        contact sheet
  src/beats.rs          dò nhịp nhạc
  src/autoedit.rs       dựng nhanh không cần Claude
  src/review.rs         render nháp + kiểm tra tự động
  src/speech.rs         Whisper, dọn lời, phụ đề karaoke
  src/render.rs         dựng filtergraph FFmpeg, chạy, báo tiến độ
  src/presets.rs        filter màu
  tests/                test tích hợp chạy FFmpeg thật
  examples/             project mẫu, công cụ dòng lệnh
crates/mcp/             MCP server cho Claude desktop (rmcp, stdio)
src-tauri/              app Tauri 2: lệnh cho giao diện, theo dõi file, Claude, giọng nói
ui/                     React + TypeScript + Vite
```

Nguyên tắc chính: `project.json` là nguồn sự thật; mọi thay đổi đi qua `Op` → `apply` có kiểm tra `revision`; Claude chỉ đề xuất, người dùng duyệt. Chi tiết ở [CLAUDE.md](CLAUDE.md).

Code có comment tiếng Việt giải thích các khái niệm Rust (ownership, `Result`, trait, async) ở chỗ quan trọng, phù hợp để học Rust.

### Build và test

```powershell
cargo test -p vlogcut-core          # unit test + test xuất video thật (~30 giây)
cargo build -p vlogcut-app -p vlogcut-mcp
npm --prefix ui run build
```

### Project mẫu để thử

```powershell
# Project có sẵn video, thư viện và timeline
cargo run -p vlogcut-core --example demo_project -- D:\Project\vlogcut-demo

# Chuyến đi 10 clip qua 2 ngày (có cảnh mờ, tối, rung, trùng), đã phân tích sẵn
cargo run -p vlogcut-core --example demo_trip -- D:\Project\vlogcut-trip
# thêm "build" để dựng nhanh và xuất luôn video
cargo run -p vlogcut-core --example demo_trip -- D:\Project\vlogcut-trip build

# Dò nhịp một file nhạc
cargo run -p vlogcut-core --example beats_cli -- nhac.mp3

# Review bản dựng của một project
cargo run -p vlogcut-core --example review_cli -- D:\Project\vlogcut-trip
```

Mở app, chọn thư mục vừa tạo.

---

## 10. Sự cố thường gặp

| Hiện tượng | Cách xử lý |
|---|---|
| "Không tìm thấy ffmpeg" | `winget install Gyan.FFmpeg`, rồi **tắt hẳn PowerShell và mở lại**. |
| `link.exe not found` khi build | Thiếu Visual Studio Build Tools, cài lại với workload C++. |
| Build chậm hoặc lỗi khoá file | Đưa project ra khỏi OneDrive. Tắt app trước khi build lại. |
| Code mới mà app không cập nhật | Mở `deploy-status.json` xem lỗi. Kiểm tra `deploy-watcher.txt` có giờ mới (trình tự cập nhật đang chạy); nếu không, bấm lại `cai-dat-app.bat`. |
| Video iPhone (HEVC) không xem trước được | Chờ proxy H.264 tạo xong (tiến độ ở cột trái). |
| Xuất video chậm | Kiểm tra `ffmpeg -encoders \| findstr nvenc`. Không có NVENC thì app dùng CPU. Cập nhật driver NVIDIA. |
| Claude không thấy công cụ vlogcut | Bấm lại **Kết nối Claude**, thoát hẳn Claude desktop qua khay hệ thống rồi mở lại. Mở chat **mới** trong Claude desktop. |
| Bấm nút mà Claude desktop không mở chat mới | Cập nhật Claude desktop lên bản mới. App tự chuyển sang cách copy: dán bằng Ctrl+V. |
| Dán lệnh `[vlogcut]` mà Claude không làm gì | Phải dán vào **Claude desktop** trên máy có vlogcut, không phải claude.ai trên web hay app khác. |
| Đề xuất báo lỗi "revision" | Project vừa thay đổi sau khi Claude đọc. Bảo Claude đọc lại timeline rồi đề xuất lại. |
| Tải Whisper thất bại | Kiểm tra mạng (cần truy cập github.com và huggingface.co), bấm tải lại. File tải dở có đuôi `.part` sẽ được ghi đè. |
| Nhận dạng giọng lỗi GPU | App tự chạy lại bằng CPU (chậm hơn). Cập nhật driver NVIDIA. |

Log build: `build-log.txt` (sau khi chạy `kiem-tra-build.bat`).

---

## 11. Lộ trình

| Chặng | Nội dung | Trạng thái |
|---|---|---|
| 0–2 | Workspace, nhập media + proxy, timeline, Op + undo, keyframe, xem trước, xuất NVENC | ✅ Xong |
| 3 | Phân tích cảnh, chấm điểm, lọc trùng, màn hình Tự dựng, dựng nhanh | ✅ Xong |
| 4 | MCP server, đề xuất/duyệt, review bản dựng, panel Nhờ Claude | ✅ Xong |
| 5 | Whisper, bỏ ừm/à, cắt khoảng lặng, cắt theo từ, phụ đề karaoke | ✅ Xong |
| 6 | Khung hình & màu: xuất nhiều tỉ lệ một lần, khung dọc bám người, khớp màu giữa clip, làm mờ mặt | |
| 7 | Âm thanh: nhịp trên timeline + hít nhịp, thu âm, tách giọng/nhạc, giọng đọc AI | |
| 8 | Hiểu chuyến đi (GPS → địa điểm), nhớ phong cách, công thức dựng, bắt chước video mẫu | |
| Sau | Giao diện chỉnh keyframe, chia chương → nhiều short, dịch phụ đề, tăng nét, sticker | |
