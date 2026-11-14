//! Tiện ích gọi chương trình `ffmpeg` / `ffprobe` bên ngoài.

use crate::error::{CoreError, Result};
use serde::Serialize;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

/// Đường dẫn ffmpeg: biến môi trường VLOGCUT_FFMPEG, nếu không có thì tìm trong PATH.
pub fn ffmpeg_bin() -> String {
    std::env::var("VLOGCUT_FFMPEG").unwrap_or_else(|_| "ffmpeg".into())
}

pub fn ffprobe_bin() -> String {
    std::env::var("VLOGCUT_FFPROBE").unwrap_or_else(|_| "ffprobe".into())
}

/// Tạo lệnh chạy. Trên Windows, ẩn cửa sổ console đen nhấp nháy.
pub fn command(bin: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(bin);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.stdin(Stdio::null());
    c
}

/// Lấy vài dòng cuối của stderr để báo lỗi dễ đọc.
pub fn tail(bytes: &[u8], lines: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    let v: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    v[v.len().saturating_sub(lines)..].join("\n")
}

/// Chạy lệnh, chờ xong, báo lỗi nếu mã thoát khác 0.
pub fn run(mut cmd: Command) -> Result<Output> {
    let out = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CoreError::Ffmpeg("không tìm thấy ffmpeg/ffprobe. Hãy cài FFmpeg và thêm vào PATH.".into())
        } else {
            CoreError::Io(e)
        }
    })?;
    if !out.status.success() {
        return Err(CoreError::Ffmpeg(tail(&out.stderr, 12)));
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Encoder {
    /// Mã hoá bằng GPU NVIDIA.
    Nvenc,
    /// Mã hoá bằng CPU.
    X264,
}

impl Encoder {
    /// Tham số mã hoá cho bản xuất chính thức.
    pub fn final_args(self) -> Vec<String> {
        let v: &[&str] = match self {
            Encoder::Nvenc => &["-c:v", "h264_nvenc", "-preset", "p5", "-rc", "vbr", "-cq", "21", "-b:v", "0"],
            Encoder::X264 => &["-c:v", "libx264", "-preset", "medium", "-crf", "20"],
        };
        v.iter().map(|s| s.to_string()).collect()
    }

    /// Tham số mã hoá nhanh cho proxy / bản xem thử.
    pub fn fast_args(self) -> Vec<String> {
        let v: &[&str] = match self {
            Encoder::Nvenc => &["-c:v", "h264_nvenc", "-preset", "p2", "-rc", "vbr", "-cq", "28", "-b:v", "0"],
            Encoder::X264 => &["-c:v", "libx264", "-preset", "veryfast", "-crf", "26"],
        };
        v.iter().map(|s| s.to_string()).collect()
    }
}

static ENCODER: OnceLock<Encoder> = OnceLock::new();

/// Phát hiện encoder một lần (thử mã hoá 0.1 giây bằng NVENC).
/// `OnceLock` đảm bảo chỉ chạy một lần dù nhiều luồng cùng gọi.
pub fn encoder() -> Encoder {
    *ENCODER.get_or_init(|| {
        let mut c = command(&ffmpeg_bin());
        c.args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=black:s=256x256:d=0.1", "-c:v", "h264_nvenc", "-f", "null", "-"]);
        match c.output() {
            Ok(o) if o.status.success() => Encoder::Nvenc,
            _ => Encoder::X264,
        }
    })
}

/// Phiên bản chính của ffmpeg (vd. 7). Bản build git không có số thì coi là mới.
pub fn version_major() -> u32 {
    static V: OnceLock<u32> = OnceLock::new();
    *V.get_or_init(|| {
        let mut c = command(&ffmpeg_bin());
        c.arg("-version");
        let Ok(o) = c.output() else { return 0 };
        let s = String::from_utf8_lossy(&o.stdout);
        let first = s.lines().next().unwrap_or("");
        // "ffmpeg version 7.1-full_build..." hoặc "ffmpeg version N-1234-..."
        let ver = first.split_whitespace().nth(2).unwrap_or("");
        ver.split(['.', '-']).next().and_then(|x| x.parse().ok()).unwrap_or(99)
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct FfmpegInfo {
    pub found: bool,
    pub version: String,
    pub encoder: Encoder,
}

pub fn info() -> FfmpegInfo {
    let mut c = command(&ffmpeg_bin());
    c.arg("-version");
    match c.output() {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let version = s.lines().next().unwrap_or("").to_string();
            FfmpegInfo { found: true, version, encoder: encoder() }
        }
        _ => FfmpegInfo { found: false, version: String::new(), encoder: Encoder::X264 },
    }
}
