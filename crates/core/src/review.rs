//! Review bản dựng: để Claude "xem" kết quả thật sau khi đã áp filter, chữ, chuyển cảnh, nhạc.
//!
//! 1. Render nhanh một bản nháp nhỏ (cạnh ngắn 360px) giống hệt bản xuất.
//! 2. Cắt ra các khung hình cách đều nhau, ghép thành ảnh lưới có đánh số.
//! 3. Đo độ to âm thanh (LUFS, chuẩn của YouTube/TikTok/Facebook), các điểm cắt có
//!    khớp nhịp nhạc không, cảnh quá ngắn/dài, cảnh bị loại vẫn được dùng, cảnh lặp lại,
//!    chữ chồng nhau…
//!
//! Kết quả gồm một báo cáo chữ và vài ảnh lưới, MCP server gửi cả hai cho Claude.

use crate::beats;
use crate::contact::{self, Cell};
use crate::error::Result;
use crate::ffmpeg::{command, ffmpeg_bin, run};
use crate::project::{MediaKind, Project, SceneTag, Settings};
use crate::render::{self, RenderOptions};
use crate::store::{data_dir, Brief};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub struct Review {
    pub report: String,
    /// Ảnh lưới JPEG, mỗi ảnh tối đa 24 khung.
    pub sheets: Vec<Vec<u8>>,
    pub draft: PathBuf,
}

/// Khung hình nháp: cạnh ngắn 360px, số chẵn.
fn draft_settings(s: Settings) -> (Settings, f64) {
    let f = (360.0 / s.width.min(s.height) as f64).min(1.0);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    (Settings { width: even(s.width as f64 * f), height: even(s.height as f64 * f), fps: s.fps.min(30) }, f)
}

/// Độ to tổng thể (LUFS) và đỉnh (dBFS) của file, đo bằng bộ lọc ebur128 của ffmpeg.
fn loudness(path: &Path) -> Option<(f64, f64)> {
    let mut c = command(&ffmpeg_bin());
    c.args(["-hide_banner", "-nostats", "-i"]).arg(path).args(["-vn", "-af", "ebur128=peak=true:framelog=quiet", "-f", "null", "-"]);
    let out = c.output().ok()?;
    let err = String::from_utf8_lossy(&out.stderr);
    // Phần tổng kết cuối có dạng "I:   -15.2 LUFS" và "Peak:  -1.3 dBFS".
    let summary = err.rsplit("Summary:").next()?;
    let grab = |key: &str| {
        summary.lines().find(|l| l.trim_start().starts_with(key)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<f64>().ok())
    };
    Some((grab("I:")?, grab("Peak:").unwrap_or(0.0)))
}

/// Render bản nháp, cắt khung, phân tích và viết báo cáo.
/// `max_frames`: số khung tối đa (mặc định 48 = 2 ảnh lưới).
pub fn review(dir: &Path, p: &Project, brief: Option<&Brief>, max_frames: usize) -> Result<Review> {
    // ---- 1. Bản nháp ----
    let mut draft_p = p.clone();
    let (ds, f) = draft_settings(p.settings);
    draft_p.settings = ds;
    for t in draft_p.texts.iter_mut() {
        t.style.size *= f;
        t.style.outline *= f;
    }
    let draft = data_dir(dir, "renders").join("review-draft.mp4");
    render::render(dir, &draft_p, &RenderOptions { out_path: draft.clone(), fast: true, encoder: None }, &AtomicBool::new(false), &mut |_| {})?;

    // ---- 2. Khung hình ----
    let duration = p.duration();
    let max_frames = max_frames.clamp(6, 96);
    let interval = ((duration / max_frames as f64) * 2.0).ceil().max(1.0) / 2.0; // bội của 0.5 giây
    let frames_dir = data_dir(dir, "renders").join("review-frames");
    let _ = std::fs::remove_dir_all(&frames_dir);
    std::fs::create_dir_all(&frames_dir)?;
    let portrait = ds.height > ds.width;
    let (cw, ch, cols) = if portrait { (144, 256, 8) } else { (256, 144, 6) };
    let mut c = command(&ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(&draft);
    // Lấy khung ở giữa mỗi khoảng: dời nửa khoảng rồi lấy mỗi `interval` giây một khung.
    c.args(["-vf", &format!("trim=start={},setpts=PTS-STARTPTS,fps=1/{interval},scale={cw}:{ch}:force_original_aspect_ratio=decrease", interval / 2.0)]);
    c.args(["-q:v", "4"]).arg(frames_dir.join("f%03d.jpg"));
    run(c)?;
    let mut frame_files: Vec<PathBuf> = std::fs::read_dir(&frames_dir)?.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e == "jpg").unwrap_or(false)).collect();
    frame_files.sort();
    frame_files.truncate(max_frames);
    let times: Vec<f64> = (0..frame_files.len()).map(|i| interval / 2.0 + i as f64 * interval).collect();
    let paths: Vec<String> = frame_files.iter().map(|p| p.to_string_lossy().to_string()).collect();
    let mut sheets = Vec::new();
    for (k, chunk) in paths.chunks(contact::PER_SHEET).enumerate() {
        let cells: Vec<Cell> = chunk.iter().enumerate().map(|(i, t)| Cell { number: (k * contact::PER_SHEET + i + 1) as u32, thumb: Some(t.as_str()), dropped: false }).collect();
        sheets.push(contact::contact_sheet_with(&cells, cw, ch, cols)?);
    }

    // ---- 3. Phân tích ----
    let layout = p.video_layout();
    let name = |id: &str| p.media_by_id(id).map(|m| m.name.clone()).unwrap_or_else(|| id.to_string());
    let clip_at = |t: f64| layout.iter().rposition(|s| t >= s.start && t < s.start + s.duration);
    let mut r = String::new();
    let mut issues: Vec<String> = Vec::new();

    writeln!(r, "REVIEW of the current timeline (revision {})", p.revision).unwrap();
    writeln!(r, "Output {}x{} @{}fps, duration {:.1}s, {} clips, {} texts, {} audio tracks.", p.settings.width, p.settings.height, p.settings.fps, duration, p.video.len(), p.texts.len(), p.audio.len()).unwrap();
    if let Some(b) = brief {
        if let Some(target) = b.target_secs {
            let diff = duration - target;
            writeln!(r, "Brief target {:.0}s ({:+.1}s), mood {}, size {}x{}.", target, diff, b.mood, b.width, b.height).unwrap();
            if diff.abs() > target * 0.1 {
                issues.push(format!("Duration {:.1}s is {:.0}% off the requested {:.0}s.", duration, diff / target * 100.0, target));
            }
        }
        if (b.width, b.height) != (p.settings.width, p.settings.height) {
            issues.push(format!("Frame size {}x{} differs from the brief {}x{}.", p.settings.width, p.settings.height, b.width, b.height));
        }
    }

    // Nhịp dựng
    let durs: Vec<f64> = layout.iter().map(|s| s.duration).collect();
    if !durs.is_empty() {
        let avg = durs.iter().sum::<f64>() / durs.len() as f64;
        let min = durs.iter().cloned().fold(f64::MAX, f64::min);
        let max = durs.iter().cloned().fold(0.0, f64::max);
        writeln!(r, "Clip length: avg {avg:.2}s, min {min:.2}s, max {max:.2}s.").unwrap();
    }
    for (i, c) in p.video.iter().enumerate() {
        let d = layout[i].duration;
        let media = p.media_by_id(&c.media_id);
        let speech = media.map(|m| m.scenes.iter().any(|s| s.tags.contains(&SceneTag::Speech) && s.start < c.src_out && s.end > c.src_in)).unwrap_or(false);
        if d < 0.5 {
            issues.push(format!("Clip {} ({}) at {:.1}s is only {:.2}s: likely a flash cut.", c.id, name(&c.media_id), layout[i].start, d));
        }
        if d > 8.0 && !speech && media.map(|m| m.kind != MediaKind::Image).unwrap_or(true) {
            issues.push(format!("Clip {} ({}) at {:.1}s runs {:.1}s without speech: may feel slow.", c.id, name(&c.media_id), layout[i].start, d));
        }
        // Cảnh bị loại (mờ/tối/rung/trùng) nhưng vẫn dùng
        if let Some(m) = media {
            for s in m.scenes.iter().filter(|s| !s.is_kept() && s.start < c.src_out && s.end > c.src_in && m.kind == MediaKind::Video) {
                let overlap = s.end.min(c.src_out) - s.start.max(c.src_in);
                if overlap > 0.4 {
                    let why: Vec<&str> = s.tags.iter().filter(|t| t.is_defect()).map(|t| t.label()).collect();
                    issues.push(format!("Clip {} uses a dropped scene of {} ({:.1}-{:.1}s: {}).", c.id, m.name, s.start, s.end, if why.is_empty() { "dropped by user".to_string() } else { why.join(", ") }));
                }
            }
        }
        // Lặp lại cùng một đoạn nguồn
        for (j, o) in p.video.iter().enumerate().skip(i + 1) {
            if o.media_id == c.media_id {
                let overlap = c.src_out.min(o.src_out) - c.src_in.max(o.src_in);
                if overlap > 0.5 {
                    issues.push(format!("Clips {} and {} reuse the same footage ({}, {:.1}s overlap) at {:.1}s and {:.1}s.", c.id, o.id, name(&c.media_id), overlap, layout[i].start, layout[j].start));
                }
            }
        }
    }
    // Các chuyển cảnh giống hệt nhau liên tục
    let trans: Vec<String> = p.video.iter().skip(1).map(|c| c.transition_in.map(|t| format!("{:?}", t.kind)).unwrap_or_else(|| "cut".into())).collect();
    if trans.len() >= 6 && trans.iter().all(|t| t == &trans[0]) && trans[0] != "cut" {
        issues.push(format!("All {} transitions are {}: consider mixing hard cuts for rhythm.", trans.len(), trans[0]));
    }

    // Khớp nhịp nhạc
    if let Some(music) = p.audio.iter().find(|a| a.role == crate::project::AudioRole::Music) {
        if let Some(m) = p.media_by_id(&music.media_id) {
            match beats::detect(&m.path, (music.src_out + 1.0).min(1800.0)) {
                Ok(b) if b.confidence >= 0.3 => {
                    let beat_tl: Vec<f64> = b.beats.iter().filter(|x| **x >= music.src_in && **x <= music.src_out).map(|x| music.start + x - music.src_in).collect();
                    let cuts: Vec<(usize, f64)> = layout.iter().enumerate().skip(1).map(|(i, s)| (i, s.start + s.transition / 2.0)).collect();
                    let mut offs: Vec<(usize, f64)> = cuts
                        .iter()
                        .filter_map(|(i, t)| beat_tl.iter().map(|bt| bt - t).min_by(|a, b| a.abs().total_cmp(&b.abs())).map(|o| (*i, o)))
                        .collect();
                    if !offs.is_empty() {
                        let on = offs.iter().filter(|(_, o)| o.abs() <= 0.08).count();
                        writeln!(r, "Music {} : {:.0} BPM (confidence {:.2}). {}/{} cuts land within 80 ms of a beat.", m.name, b.bpm, b.confidence, on, offs.len()).unwrap();
                        offs.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
                        let off_list: Vec<String> = offs.iter().take(6).filter(|(_, o)| o.abs() > 0.08).map(|(i, o)| format!("cut before {} at {:.2}s is {:+.2}s from the nearest beat", p.video[*i].id, layout[*i].start, o)).collect();
                        if !off_list.is_empty() {
                            issues.push(format!("Off-beat cuts: {}. Positive = the beat comes later: lengthen the clip before the cut by that amount; negative = shorten it.", off_list.join("; ")));
                        }
                    }
                }
                Ok(b) => {
                    writeln!(r, "Music {}: no clear beat (confidence {:.2}), beat sync not checked.", m.name, b.confidence).unwrap();
                }
                Err(e) => {
                    writeln!(r, "Could not analyze music beats: {e}").unwrap();
                }
            }
            if music.start + music.duration() < duration - 0.5 {
                issues.push(format!("Music ends at {:.1}s but the video runs {:.1}s: silent ending.", music.start + music.duration(), duration));
            }
            if music.fade_out < 0.5 {
                issues.push("Music has no fade-out: the ending may cut off abruptly.".into());
            }
        }
    } else {
        writeln!(r, "No music track.").unwrap();
    }

    // Chữ
    for (i, t) in p.texts.iter().enumerate() {
        let len = t.end - t.start;
        let chars = t.text.chars().count();
        if len < 0.12 * chars as f64 + 0.6 && len < 2.0 {
            issues.push(format!("Text \"{}\" shows only {:.1}s: too short to read.", t.text, len));
        }
        if t.end > duration + 0.1 {
            issues.push(format!("Text \"{}\" ends at {:.1}s, after the video ends ({:.1}s).", t.text, t.end, duration));
        }
        for o in p.texts.iter().skip(i + 1) {
            if o.style.align == t.style.align && o.start < t.end && o.end > t.start && t.transform.position.is_none() && o.transform.position.is_none() {
                issues.push(format!("Texts \"{}\" and \"{}\" overlap in time at the same position.", t.text, o.text));
            }
        }
    }

    // Âm lượng
    match loudness(&draft) {
        Some((lufs, peak)) => {
            writeln!(r, "Loudness {lufs:.1} LUFS, peak {peak:.1} dBFS (social media target about -14 LUFS).").unwrap();
            if lufs < -20.0 {
                issues.push(format!("Overall audio is quiet ({lufs:.1} LUFS): raise music/clip volume."));
            } else if lufs > -9.0 {
                issues.push(format!("Overall audio is very loud ({lufs:.1} LUFS): lower volumes."));
            }
        }
        None => writeln!(r, "Loudness could not be measured.").unwrap(),
    }

    writeln!(r, "\nAUTOMATIC CHECKS ({} found):", issues.len()).unwrap();
    if issues.is_empty() {
        writeln!(r, "- none").unwrap();
    }
    for i in &issues {
        writeln!(r, "- {i}").unwrap();
    }

    writeln!(r, "\nFRAMES (number on the image | timeline time | clip | source file | texts on screen):").unwrap();
    for (i, t) in times.iter().enumerate() {
        let clip = clip_at(*t).map(|k| (p.video[k].id.clone(), name(&p.video[k].media_id))).unwrap_or_default();
        let on: Vec<&str> = p.texts.iter().filter(|x| *t >= x.start && *t < x.end).map(|x| x.text.as_str()).collect();
        writeln!(r, "{}|{:.1}s|{}|{}|{}", i + 1, t, clip.0, clip.1, on.join(" / ")).unwrap();
    }
    writeln!(r, "\nLook at the frames for: weak opening shot, story order, color consistency between clips, readable text, awkward framing. Propose concrete fixes with timeline_propose.").unwrap();

    let _ = std::fs::remove_dir_all(&frames_dir);
    Ok(Review { report: r, sheets, draft })
}
