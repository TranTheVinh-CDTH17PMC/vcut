//! Kết nối với Claude desktop: ghi MCP server của vlogcut vào file cấu hình của Claude.
//!
//! Claude desktop đọc `claude_desktop_config.json`, mục `mcpServers`, rồi tự chạy
//! các chương trình được khai báo ở đó. Ta thêm mục "vlogcut" trỏ tới `vlogcut-mcp.exe`.
//!
//! File exe được chép sang `%APPDATA%\vlogcut\bin\` trước: nếu trỏ thẳng vào thư mục
//! build `target\`, khi Claude đang chạy nó thì Windows khoá file và lần build sau sẽ lỗi.

use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use vlogcut_core::store;

#[cfg(windows)]
const MCP_EXE: &str = "vlogcut-mcp.exe";
#[cfg(not(windows))]
const MCP_EXE: &str = "vlogcut-mcp";

#[derive(Serialize)]
pub struct ClaudeStatus {
    /// Đã có mục "vlogcut" trong cấu hình Claude desktop chưa.
    pub configured: bool,
    /// Đường dẫn file cấu hình đang dùng (hoặc sẽ tạo).
    pub config_path: String,
    /// Có tìm thấy vlogcut-mcp.exe cạnh app không.
    pub mcp_found: bool,
    /// Lần cuối Claude gọi một tool (ISO 8601, UTC).
    pub last_call: Option<String>,
    pub last_tool: Option<String>,
}

/// Các vị trí có thể có của file cấu hình Claude desktop.
/// Bản cài từ Microsoft Store lưu trong thư mục ảo hoá của gói ứng dụng.
fn config_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    // Bản Microsoft Store đứng trước để hộp thoại hiện đúng file Claude thực sự đọc.
    if let Some(local) = dirs::data_local_dir() {
        if let Ok(rd) = std::fs::read_dir(local.join("Packages")) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("Claude_") || name.starts_with("AnthropicPBC.Claude") {
                    v.push(e.path().join("LocalCache").join("Roaming").join("Claude").join("claude_desktop_config.json"));
                }
            }
        }
    }
    if let Some(d) = dirs::config_dir() {
        v.push(d.join("Claude").join("claude_desktop_config.json"));
    }
    v
}

/// File cấu hình đang tồn tại; nếu chưa có thì dùng vị trí chuẩn.
/// Các file cấu hình cần ghi.
///
/// Claude desktop bản Microsoft Store (MSIX) chạy trong "hộp" riêng: nó đọc
/// `%LOCALAPPDATA%\Packages\Claude_xxx\LocalCache\Roaming\Claude\...` chứ không đọc
/// `%APPDATA%\Claude\...`, và thư mục trong hộp có thể chưa tồn tại. Vì vậy với mỗi
/// gói Claude tìm thấy, ta luôn ghi vào hộp (tự tạo thư mục), đồng thời ghi cả bản
/// thường cho chắc — ghi thừa một file không ảnh hưởng gì.
fn config_paths() -> Vec<PathBuf> {
    config_candidates()
}

/// vlogcut-mcp nằm cùng thư mục với file chạy của app.
fn bundled_mcp() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.parent()?.join(MCP_EXE);
    p.exists().then_some(p)
}

fn installed_mcp() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vlogcut").join("bin").join(MCP_EXE))
}

fn read_config(path: &PathBuf) -> Value {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({}))
}

pub fn status() -> ClaudeStatus {
    let paths = config_paths();
    // Phải có ở mọi file: nếu chỉ một file có, có thể đó không phải file Claude đọc.
    let configured = !paths.is_empty() && paths.iter().all(|p| read_config(p)["mcpServers"]["vlogcut"].is_object());
    // Nhịp tim của MCP server có thể ở %APPDATA%\vlogcut hoặc trong project; lấy bản mới nhất.
    let read = |p: Option<PathBuf>| -> Option<Value> { p.and_then(|p| store::read_json(&p).ok()) };
    let a = read(store::current_pointer_path().map(|p| p.with_file_name("mcp-status.json")));
    let b = read(store::get_current().map(|d| d.join(".vlogcut").join("mcp-status.json")));
    let when = |v: &Option<Value>| v.as_ref().and_then(|v| v["last_call"].as_str().map(String::from)).unwrap_or_default();
    // ISO 8601 cùng định dạng nên so sánh chuỗi là so sánh thời gian.
    let hb = if when(&b) > when(&a) { b } else { a };
    ClaudeStatus {
        configured,
        config_path: paths.first().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        mcp_found: bundled_mcp().is_some() || installed_mcp().map(|p| p.exists()).unwrap_or(false),
        last_call: hb.as_ref().and_then(|v| v["last_call"].as_str().map(String::from)),
        last_tool: hb.as_ref().and_then(|v| v["tool"].as_str().map(String::from)),
    }
}

/// Tạo file tiện ích `vlogcut.mcpb` cho Claude desktop (Settings → Extensions).
///
/// `.mcpb` là file zip gồm `manifest.json` + chương trình server. Cài bằng cách mở file
/// (Claude hiện hộp thoại Install) hoặc kéo thả vào trang Extensions. Cách này không phụ
/// thuộc vị trí file cấu hình nên chạy được cả với bản Microsoft Store.
///
/// Lệnh chạy trỏ tới bản ở `%APPDATA%\vlogcut\bin` (không phải bản nằm trong tiện ích),
/// để mỗi lần deploy.ps1 cập nhật thì Claude dùng luôn bản mới, không phải cài lại.
pub fn build_extension(mcp: &std::path::Path) -> Result<PathBuf, String> {
    use std::io::Write;
    let out = mcp
        .parent()
        .and_then(|bin| bin.parent())
        .ok_or("Không xác định được thư mục vlogcut")?
        .join("vlogcut.mcpb");
    // Bỏ đuôi .exe: Windows tự thêm .exe khi chạy, tránh trường hợp Claude thêm lần nữa.
    let command = mcp.with_extension("").to_string_lossy().to_string();
    let manifest = json!({
        "manifest_version": "0.3",
        "name": "vlogcut",
        "display_name": "vlogcut",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Cho Claude dựng video trong app vlogcut: xem cảnh, đọc yêu cầu, đề xuất timeline, review bản dựng.",
        "author": { "name": "vlogcut" },
        "server": {
            "type": "binary",
            "entry_point": "server/vlogcut-mcp.exe",
            "mcp_config": { "command": command, "args": [] }
        },
        "compatibility": { "platforms": ["win32", "darwin", "linux"] }
    });
    let exe = std::fs::read(mcp).map_err(|e| format!("Không đọc được {}: {e}", mcp.display()))?;
    let file = std::fs::File::create(&out).map_err(|e| format!("Không tạo được {}: {e}", out.display()))?;
    // `ZipWriter` ghi từng mục vào file zip; `?` trả lỗi sớm nếu bước nào hỏng.
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| e.to_string();
    zip.start_file("manifest.json", opts).map_err(err)?;
    zip.write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes()).map_err(|e| e.to_string())?;
    zip.start_file("server/vlogcut-mcp.exe", opts).map_err(err)?;
    zip.write_all(&exe).map_err(|e| e.to_string())?;
    zip.finish().map_err(err)?;
    Ok(out)
}

/// Kết quả tạo tiện ích: đường dẫn file và Claude desktop có tự mở hộp thoại cài không.
#[derive(Serialize)]
pub struct ExtensionResult {
    pub path: String,
    pub opened: bool,
}

/// Tạo `vlogcut.mcpb` từ bản MCP đã chép vào AppData rồi mở nó.
/// Nếu không mở được thì chọn sẵn file trong Explorer để người dùng kéo thả.
pub fn install_extension() -> Result<ExtensionResult, String> {
    let mcp = installed_mcp().filter(|p| p.exists()).ok_or("Hãy bấm Kết nối trước")?;
    let path = build_extension(&mcp)?;
    let opened = open_file(&path);
    if !opened {
        reveal(&path);
    }
    Ok(ExtensionResult { path: path.to_string_lossy().to_string(), opened })
}

/// Mở file bằng chương trình mặc định của Windows (với .mcpb là Claude desktop).
/// Trả `false` nếu không có chương trình nào nhận đuôi file đó.
pub fn open_file(path: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", &path.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// Mở Explorer và chọn sẵn file (để người dùng kéo thả).
pub fn reveal(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", path.display())).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
}

/// Chép vlogcut-mcp vào %APPDATA%\vlogcut\bin và thêm vào cấu hình Claude desktop.
/// Giữ nguyên các MCP server khác; lưu bản sao `.bak` trước khi ghi.
pub fn connect() -> Result<ClaudeStatus, String> {
    let src = bundled_mcp().ok_or_else(|| {
        "Không tìm thấy vlogcut-mcp.exe cạnh app. Hãy build nó trước: cargo build -p vlogcut-mcp (hoặc chạy lại kiem-tra-build.bat).".to_string()
    })?;
    let dst = installed_mcp().ok_or("Không xác định được thư mục AppData")?;
    std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::copy(&src, &dst).map_err(|e| {
        format!("Không chép được vlogcut-mcp ({e}). Nếu Claude desktop đang mở, hãy thoát hẳn Claude desktop (chuột phải icon ở khay hệ thống > Quit) rồi bấm lại.")
    })?;

    for path in config_paths() {
        let mut cfg = read_config(&path);
        if path.exists() {
            let _ = std::fs::copy(&path, path.with_extension("json.bak"));
        }
        if !cfg.is_object() {
            cfg = json!({});
        }
        if !cfg["mcpServers"].is_object() {
            cfg["mcpServers"] = json!({});
        }
        cfg["mcpServers"]["vlogcut"] = json!({ "command": dst.to_string_lossy(), "args": [] });
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(&cfg).unwrap()).map_err(|e| format!("Không ghi được {}: {e}", path.display()))?;
    }
    Ok(status())
}

/// Đưa Claude desktop lên trước. Thử theo thứ tự:
/// 1. Mở đường dẫn `claude://` (Claude desktop đăng ký giao thức này khi cài).
/// 2. Chạy thẳng file claude.exe ở vị trí cài mặc định (nếu đang chạy, Claude tự
///    chuyển cửa sổ đang mở lên trước thay vì mở cửa sổ mới).
/// Trả về `true` nếu đã gửi được lệnh mở.
/// Mở một chat mới trong Claude desktop với lời nhắn điền sẵn, qua link chính thức
/// `claude://claude.ai/new?q=...`. Claude desktop chỉ điền vào ô nhập, người dùng
/// tự bấm Enter để gửi (Claude không tự gửi lệnh từ link — đó là cơ chế an toàn).
///
/// `prompt` là văn bản thường; hàm tự mã hoá URL. Trả `false` nếu không mở được
/// (vd. Claude desktop chưa cài) để giao diện chuyển sang cách copy.
pub fn open_chat(prompt: &str) -> bool {
    // Claude cắt `q` ở khoảng 14.000 ký tự; giữ an toàn ở 6.000 ký tự gốc.
    let text: String = prompt.chars().take(6000).collect();
    let url = format!("claude://claude.ai/new?q={}", encode_query(&text));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // Dùng rundll32 + FileProtocolHandler thay vì `cmd /C start`: cmd sẽ hiểu sai
        // các ký tự `%` và `&` trong URL, còn rundll32 chuyển nguyên chuỗi cho Windows.
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&url).status().map(|s| s.success()).unwrap_or(false)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = url;
        false
    }
}

/// Mã hoá phần trăm (percent-encoding) cho tham số URL: chữ/số ASCII và `-_.~`
/// giữ nguyên, mọi byte UTF-8 khác (kể cả chữ có dấu tiếng Việt) thành `%XX`.
/// Ví dụ "Dựng vlog" → "D%E1%BB%B1ng%20vlog".
fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    // `bytes()` duyệt từng byte UTF-8 — một chữ "ự" là 3 byte nên thành 3 nhóm %XX.
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::encode_query;

    #[test]
    fn builds_mcpb_with_manifest_and_server() {
        let root = std::env::temp_dir().join(format!("vlogcut-mcpb-{}", std::process::id()));
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("vlogcut-mcp.exe");
        std::fs::write(&exe, b"fake").unwrap();
        let out = super::build_extension(&exe).unwrap();
        assert_eq!(out, root.join("vlogcut.mcpb"));
        let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut m = String::new();
        std::io::Read::read_to_string(&mut z.by_name("manifest.json").unwrap(), &mut m).unwrap();
        let v: serde_json::Value = serde_json::from_str(&m).unwrap();
        assert_eq!(v["server"]["type"], "binary");
        assert_eq!(v["server"]["entry_point"], "server/vlogcut-mcp.exe");
        assert!(v["server"]["mcp_config"]["command"].as_str().unwrap().ends_with("vlogcut-mcp"));
        assert_eq!(z.by_name("server/vlogcut-mcp.exe").unwrap().size(), 4);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn encodes_vietnamese_and_reserved_chars() {
        assert_eq!(encode_query("Dựng vlog"), "D%E1%BB%B1ng%20vlog");
        assert_eq!(encode_query("a&b=c?%"), "a%26b%3Dc%3F%25");
        assert_eq!(encode_query("[vlogcut] ok"), "%5Bvlogcut%5D%20ok");
    }
}

pub fn open_app() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Some(local) = dirs::data_local_dir() {
            for exe in [local.join("AnthropicClaude").join("claude.exe"), local.join("Programs").join("Claude").join("Claude.exe")] {
                if exe.exists() && std::process::Command::new(&exe).spawn().is_ok() {
                    return true;
                }
            }
        }
        std::process::Command::new("cmd")
            .args(["/C", "start", "", "claude://"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").args(["-a", "Claude"]).status().map(|s| s.success()).unwrap_or(false)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        false
    }
}
