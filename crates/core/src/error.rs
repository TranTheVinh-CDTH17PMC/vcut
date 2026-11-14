//! Kiểu lỗi chung của crate.
//!
//! Trong Rust, hàm có thể thất bại trả về `Result<T, E>`: hoặc `Ok(giá_trị)`,
//! hoặc `Err(lỗi)`. Không có exception. Dấu `?` sau một lời gọi nghĩa là
//! "nếu lỗi thì trả lỗi đó ra ngoài ngay, nếu không thì lấy giá trị".
//! `thiserror` giúp viết enum lỗi gọn và tự sinh thông báo.

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Lỗi đọc/ghi file: {0}")]
    Io(#[from] std::io::Error),

    #[error("Lỗi dữ liệu JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Thao tác không hợp lệ: {0}")]
    InvalidOp(String),

    #[error("Không tìm thấy: {0}")]
    NotFound(String),

    #[error("Project đã thay đổi (đang ở bản {actual}, yêu cầu dựa trên bản {expected}). Hãy đọc lại project rồi thử lại.")]
    Conflict { expected: u64, actual: u64 },

    #[error("FFmpeg lỗi: {0}")]
    Ffmpeg(String),

    #[error("Đã huỷ")]
    Cancelled,
}

/// Viết tắt: `Result<T>` của crate này luôn dùng `CoreError`.
pub type Result<T> = std::result::Result<T, CoreError>;

/// Cho phép gửi lỗi sang giao diện (Tauri cần lỗi Serialize được).
/// Ta chỉ gửi chuỗi thông báo tiếng Việt.
impl Serialize for CoreError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// Hàm tiện ích để tạo lỗi thao tác không hợp lệ.
pub fn invalid(msg: impl Into<String>) -> CoreError {
    CoreError::InvalidOp(msg.into())
}
