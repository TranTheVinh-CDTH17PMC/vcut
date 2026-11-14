//! Lõi của vlogcut.
//!
//! Crate này KHÔNG phụ thuộc Tauri hay giao diện: app desktop và MCP server
//! đều gọi vào đây. Nhờ vậy phần logic quan trọng nhất test được độc lập
//! bằng `cargo test -p vlogcut-core`.
//!
//! Các module:
//! - [`project`]  : cấu trúc dữ liệu của một project (media, clip, chữ, nhạc)
//! - [`keyframe`] : giá trị chuyển động theo thời gian + nội suy
//! - [`ops`]      : các thao tác sửa timeline, áp dụng và so sánh (diff)
//! - [`store`]    : đọc/ghi project.json, lịch sử undo/redo, đề xuất của Claude
//! - [`media`]    : gọi ffprobe/ffmpeg để đọc thông tin, tạo thumbnail, proxy
//! - [`presets`]  : các filter màu dựng sẵn
//! - [`render`]   : dựng lệnh FFmpeg từ timeline và xuất video

pub mod analysis;
pub mod autoedit;
pub mod beats;
pub mod contact;
pub mod error;
pub mod ffmpeg;
pub mod keyframe;
pub mod media;
pub mod ops;
pub mod presets;
pub mod project;
pub mod render;
pub mod review;
pub mod speech;
pub mod store;
pub mod transitions;

pub use error::{CoreError, Result};
