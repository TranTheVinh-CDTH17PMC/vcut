//! Cập nhật tại chỗ cho bản cài trên máy.
//!
//! `deploy.ps1` build xong sẽ chép `vlogcut.exe` mới vào thư mục cài
//! (`%LOCALAPPDATA%\Programs\vlogcut`). Windows không cho ghi đè file .exe đang chạy,
//! nhưng **cho đổi tên** nó, nên script đổi tên bản đang chạy thành `vlogcut.old-<số>.exe`
//! rồi chép bản mới vào đúng tên cũ. App đang mở vẫn chạy bình thường; module này
//! phát hiện có bản mới để giao diện hiện nút "Mở lại".

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

#[cfg(windows)]
const APP_EXE: &str = "vlogcut.exe";
#[cfg(not(windows))]
const APP_EXE: &str = "vlogcut";

/// Thời điểm app khởi động. `OnceLock` là ô nhớ toàn cục chỉ gán được một lần,
/// an toàn khi nhiều luồng cùng đọc (khác với `static mut` phải dùng `unsafe`).
static STARTED: OnceLock<SystemTime> = OnceLock::new();

/// Gọi một lần khi app mở: ghi lại giờ khởi động và dọn các bản cũ đã bị đổi tên.
pub fn init() {
    STARTED.get_or_init(SystemTime::now);
    if let Some(dir) = install_dir() {
        // `flatten()` bỏ qua các mục đọc lỗi; xoá lỗi (file còn đang chạy) thì thôi.
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.contains(".old-") && name.ends_with(".exe") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
}

/// Thư mục chứa file chạy, chỉ với bản cài (bản dev `npm run dev` không kiểm tra).
fn install_dir() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        return None;
    }
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

/// Có bản mới hơn bản đang chạy không.
pub fn available() -> bool {
    let Some(dir) = install_dir() else { return false };
    let target = dir.join(APP_EXE);
    let Ok(meta) = std::fs::metadata(&target) else { return false };
    let Ok(modified) = meta.modified() else { return false };
    let started = *STARTED.get_or_init(SystemTime::now);
    // File vlogcut.exe được ghi sau lúc app mở (chừa 5 giây sai số) → có bản mới.
    modified > started + Duration::from_secs(5)
}

/// Mở bản mới rồi thoát bản đang chạy. Project luôn được lưu ngay sau mỗi thao tác
/// nên khởi động lại không mất gì.
pub fn restart(app: &tauri::AppHandle) -> Result<(), String> {
    let dir = install_dir().ok_or("Chỉ dùng được với bản cài")?;
    std::process::Command::new(dir.join(APP_EXE))
        .current_dir(&dir)
        .spawn()
        .map_err(|e| format!("Không mở được bản mới: {e}"))?;
    app.exit(0);
    Ok(())
}
