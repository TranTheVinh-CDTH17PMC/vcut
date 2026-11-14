//! Nhập file media: đọc thông tin bằng ffprobe, tạo thumbnail và proxy.

use crate::error::{CoreError, Result};
use crate::ffmpeg::{self, command, encoder, ffmpeg_bin, ffprobe_bin, run};
use crate::project::{new_id, Gps, MediaItem, MediaKind};
use crate::store::data_dir;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const VIDEO_EXT: &[&str] = &["mp4", "mov", "m4v", "mkv", "avi", "mts", "m2ts", "webm", "3gp"];
pub const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp"];
pub const AUDIO_EXT: &[&str] = &["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"];

/// File có phải media được hỗ trợ không (theo đuôi file).
pub fn is_media_file(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    VIDEO_EXT.contains(&ext.as_str()) || IMAGE_EXT.contains(&ext.as_str()) || AUDIO_EXT.contains(&ext.as_str())
}

/// Gom mọi file media trong danh sách đường dẫn (thư mục thì quét đệ quy).
pub fn collect_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        walk(p, &mut out, 0);
    }
    out.sort();
    out.dedup();
    out
}

fn walk(p: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if p.is_dir() {
        if depth > 8 {
            return;
        }
        // Bỏ qua thư mục dữ liệu của chính app.
        if p.file_name().map(|n| n == crate::store::DATA_DIR).unwrap_or(false) {
            return;
        }
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                walk(&e.path(), out, depth + 1);
            }
        }
    } else if is_media_file(p) {
        out.push(p.to_path_buf());
    }
}

/// Đọc thông tin file bằng ffprobe.
pub fn probe(path: &Path) -> Result<MediaItem> {
    let mut c = command(&ffprobe_bin());
    c.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(path);
    let out = run(c)?;
    let v: Value = serde_json::from_slice(&out.stdout)?;
    let mut item = parse_probe(path, &v)?;
    item.modified_at = std::fs::metadata(path).and_then(|m| m.modified()).ok().map(iso_utc);
    Ok(item)
}

/// Đổi thời điểm hệ thống sang chuỗi ISO 8601 (UTC), không cần thư viện ngày giờ.
pub fn iso_utc(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Thuật toán "days from civil" của Howard Hinnant: số ngày -> năm/tháng/ngày.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem / 60) % 60, rem % 60)
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// "30000/1001" -> 29.97
fn rate(s: &str) -> Option<f64> {
    let (a, b) = s.split_once('/')?;
    let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
    if b == 0.0 { None } else { Some(a / b) }
}

/// Toạ độ dạng ISO 6709: "+10.7769+106.7009/" hoặc "+10.7769+106.7009+005.000/".
fn parse_iso6709(s: &str) -> Option<Gps> {
    let s = s.trim().trim_end_matches('/');
    let mut parts = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if (ch == '+' || ch == '-') && !cur.is_empty() {
            parts.push(cur.clone());
            cur.clear();
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    let lat: f64 = parts.first()?.parse().ok()?;
    let lon: f64 = parts.get(1)?.parse().ok()?;
    Some(Gps { lat, lon })
}

fn parse_probe(path: &Path, v: &Value) -> Result<MediaItem> {
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let format = &v["format"];
    let video = streams.iter().find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1);
    let has_audio = streams.iter().any(|s| s["codec_type"] == "audio");
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();

    let kind = if IMAGE_EXT.contains(&ext.as_str()) {
        MediaKind::Image
    } else if video.is_some() {
        MediaKind::Video
    } else if has_audio {
        MediaKind::Audio
    } else {
        return Err(CoreError::Ffmpeg(format!("'{}' không có hình hay tiếng", path.display())));
    };

    let duration = if kind == MediaKind::Image { 0.0 } else { num(&format["duration"]).unwrap_or(0.0) };
    let (mut width, mut height, mut fps) = (0u32, 0u32, 0.0);
    if let Some(s) = video {
        width = s["width"].as_u64().unwrap_or(0) as u32;
        height = s["height"].as_u64().unwrap_or(0) as u32;
        fps = s["avg_frame_rate"].as_str().and_then(rate).or_else(|| s["r_frame_rate"].as_str().and_then(rate)).unwrap_or(0.0);
        // Video quay dọc trên điện thoại thường lưu xoay 90°: đổi chiều rộng/cao.
        let rot = s["tags"]["rotate"].as_str().and_then(|r| r.parse::<i64>().ok()).or_else(|| {
            s["side_data_list"].as_array().and_then(|l| l.iter().find_map(|d| d["rotation"].as_i64()))
        });
        if matches!(rot.map(|r| r.rem_euclid(180)), Some(90)) {
            std::mem::swap(&mut width, &mut height);
        }
    }
    let tags = &format["tags"];
    let created_at = tags["creation_time"].as_str().map(|s| s.to_string());
    let gps = tags["location"].as_str().or_else(|| tags["com.apple.quicktime.location.ISO6709"].as_str()).and_then(parse_iso6709);

    Ok(MediaItem {
        id: new_id("media"),
        path: path.to_string_lossy().to_string(),
        name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        kind,
        duration,
        width,
        height,
        fps,
        has_audio,
        created_at,
        gps,
        thumb: None,
        proxy: None,
        scenes: vec![],
            analysis_version: 0,
            modified_at: None,
            transcribed: false,
    })
}

/// Tạo thumbnail JPG rộng 320px. Trả về đường dẫn file.
pub fn make_thumbnail(project_dir: &Path, item: &MediaItem) -> Result<Option<String>> {
    if item.kind == MediaKind::Audio {
        return Ok(None);
    }
    let out = data_dir(project_dir, "thumbs").join(format!("{}.jpg", item.id));
    std::fs::create_dir_all(out.parent().unwrap())?;
    let at = if item.kind == MediaKind::Video { (item.duration * 0.3).min(1.5) } else { 0.0 };
    let mut c = command(&ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y"]);
    if at > 0.0 {
        c.args(["-ss", &format!("{at:.3}")]);
    }
    c.arg("-i").arg(&item.path);
    c.args(["-frames:v", "1", "-vf", "scale=320:-2", "-q:v", "4"]).arg(&out);
    run(c)?;
    Ok(Some(out.to_string_lossy().to_string()))
}

/// Tạo proxy H.264 cạnh ngắn 540px để xem trước mượt. Ảnh và âm thanh không cần.
pub fn make_proxy(project_dir: &Path, item: &MediaItem) -> Result<Option<String>> {
    if item.kind != MediaKind::Video {
        return Ok(None);
    }
    let out = data_dir(project_dir, "proxies").join(format!("{}.mp4", item.id));
    std::fs::create_dir_all(out.parent().unwrap())?;
    let mut c = command(&ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(&item.path);
    // Cạnh ngắn = 540 (ngang hay dọc đều đúng). `-g 15`: keyframe dày để tua nhanh.
    c.args(["-vf", "scale='if(gt(iw,ih),-2,540)':'if(gt(iw,ih),540,-2)',format=yuv420p"]);
    c.args(encoder().fast_args());
    c.args(["-g", "15", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart"]).arg(&out);
    match run(c) {
        Ok(_) => Ok(Some(out.to_string_lossy().to_string())),
        Err(e) if encoder() == ffmpeg::Encoder::Nvenc => {
            // NVENC đôi khi lỗi với video lạ: thử lại bằng CPU.
            let mut c = command(&ffmpeg_bin());
            c.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(&item.path);
            c.args(["-vf", "scale='if(gt(iw,ih),-2,540)':'if(gt(iw,ih),540,-2)',format=yuv420p"]);
            c.args(ffmpeg::Encoder::X264.fast_args());
            c.args(["-g", "15", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart"]).arg(&out);
            run(c).map_err(|_| e)?;
            Ok(Some(out.to_string_lossy().to_string()))
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gps() {
        let g = parse_iso6709("+10.7769+106.7009/").unwrap();
        assert_eq!((g.lat, g.lon), (10.7769, 106.7009));
        let g = parse_iso6709("-33.8688+151.2093+012.000/").unwrap();
        assert_eq!(g.lat, -33.8688);
    }

    #[test]
    fn ngay_iso() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_794_230_510);
        assert_eq!(iso_utc(t), "2026-11-09T13:21:50Z");
    }

    #[test]
    fn ty_le_khung() {
        assert!((rate("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert_eq!(rate("0/0"), None);
    }
}
