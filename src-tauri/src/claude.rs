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
    if let Some(d) = dirs::config_dir() {
        v.push(d.join("Claude").join("claude_desktop_config.json"));
    }
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
    v
}

/// File cấu hình đang tồn tại; nếu chưa có thì dùng vị trí chuẩn.
fn config_paths() -> Vec<PathBuf> {
    let all = config_candidates();
    let existing: Vec<PathBuf> = all.iter().filter(|p| p.exists() || p.parent().map(|d| d.exists()).unwrap_or(false)).cloned().collect();
    if existing.is_empty() { all.into_iter().take(1).collect() } else { existing }
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
    let configured = paths.iter().any(|p| read_config(p)["mcpServers"]["vlogcut"].is_object());
    let hb: Option<Value> = store::current_pointer_path().and_then(|p| store::read_json(&p.with_file_name("mcp-status.json")).ok());
    ClaudeStatus {
        configured,
        config_path: paths.first().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        mcp_found: bundled_mcp().is_some() || installed_mcp().map(|p| p.exists()).unwrap_or(false),
        last_call: hb.as_ref().and_then(|v| v["last_call"].as_str().map(String::from)),
        last_tool: hb.as_ref().and_then(|v| v["tool"].as_str().map(String::from)),
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
