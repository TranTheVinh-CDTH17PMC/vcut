//! Phân tích cảnh, chạy hoàn toàn trên máy, không dùng AI.
//!
//! Với mỗi video, ta cho ffmpeg giải mã thành ảnh xám nhỏ (320x180, 6 khung/giây)
//! và đọc thẳng từ "ống" (pipe) stdout, không ghi file tạm. Từ các khung đó tính:
//!
//! - **Điểm cắt cảnh**: biểu đồ độ sáng (histogram) của hai khung liền nhau khác hẳn nhau.
//! - **Độ nét**: phương sai của bộ lọc Laplacian. Ảnh mờ có ít cạnh sắc => phương sai thấp.
//!   Để không nhầm trời/mây (ít chi tiết nhưng vẫn nét) là ảnh mờ, ta so chi tiết ở
//!   độ phân giải đầy đủ với chi tiết ở ảnh thu nhỏ một nửa: ảnh mờ mất chi tiết nhỏ
//!   nhiều hơn hẳn chi tiết lớn. Cảnh quá ít chi tiết thì không kết luận là mờ.
//! - **Độ sáng**: trung bình mức xám.
//! - **Chuyển động và rung**: ước lượng độ dịch chuyển của cả khung giữa hai khung liền nhau.
//!   Lia máy đều có dịch chuyển đều; rung tay có dịch chuyển giật cục, đổi hướng liên tục.
//! - **dHash**: "dấu vân tay" 64 bit của hình, hai cảnh giống nhau có dấu gần giống nhau.
//!
//! Âm thanh: lọc dải tần giọng nói (300–3400 Hz), đo độ to theo từng đoạn 20 ms.
//! Tiếng nói có nhịp lên xuống theo âm tiết, khác tiếng gió/xe đều đều.
//! Đây chỉ là ước lượng; nhận diện chính xác sẽ dùng Whisper ở chặng 5.

use crate::error::{CoreError, Result};
use crate::ffmpeg::{command, ffmpeg_bin, run, tail};
use crate::project::{new_id, MediaItem, MediaKind, Scene, SceneTag};
use crate::store::data_dir;
use rayon::prelude::*;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Tăng số này khi đổi thuật toán để app biết cần phân tích lại.
pub const ANALYSIS_VERSION: u32 = 3;

const W: usize = 320;
const H: usize = 180;
/// Ảnh thu nhỏ 4 lần (80x45) dùng ước lượng chuyển động.
const SW: usize = W / 4;
const SH: usize = H / 4;
const FPS: f64 = 6.0;
/// Cảnh dài hơn mức này được chia thành nhiều đoạn để dễ chọn.
const MAX_SEGMENT: f64 = 8.0;
const MIN_SCENE: f64 = 1.0;
/// Dưới mức chi tiết này coi là cảnh trơn (trời, tường).
const FLAT_DETAIL: f64 = 20.0;

/// Số liệu của một khung hình.
struct FrameStat {
    brightness: f64,
    sharpness: f64,
    /// Chi tiết ở ảnh thu nhỏ một nửa (Laplacian 160x90).
    detail: f64,
    /// Biểu đồ 32 mức xám, đã chuẩn hoá (tổng = 1).
    hist: [f64; 32],
    /// Ảnh thu nhỏ 80x45 để ước lượng chuyển động.
    small: Vec<u8>,
    /// Tỉ lệ điểm ảnh gần trắng (cháy sáng).
    clipped: f64,
}

fn frame_stat(px: &[u8]) -> FrameStat {
    let n = (W * H) as f64;
    let mut sum = 0u64;
    let mut hist = [0f64; 32];
    let mut clipped = 0usize;
    for &p in px {
        sum += p as u64;
        hist[(p >> 3) as usize] += 1.0;
        if p >= 250 {
            clipped += 1;
        }
    }
    for h in hist.iter_mut() {
        *h /= n;
    }
    let sharpness = lap_var(px, W, H);
    let half = downsample(px, W, H, 2);
    let detail = lap_var(&half, W / 2, H / 2);
    let small = downsample(px, W, H, 4);
    FrameStat { brightness: sum as f64 / n, sharpness, detail, hist, small, clipped: clipped as f64 / n }
}

/// Phương sai Laplacian (4 lân cận: 4*giữa - trên - dưới - trái - phải).
fn lap_var(px: &[u8], w: usize, h: usize) -> f64 {
    let (mut acc, mut acc2, mut cnt) = (0f64, 0f64, 0f64);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let l = 4.0 * px[i] as f64 - px[i - 1] as f64 - px[i + 1] as f64 - px[i - w] as f64 - px[i + w] as f64;
            acc += l;
            acc2 += l * l;
            cnt += 1.0;
        }
    }
    let mean = acc / cnt;
    acc2 / cnt - mean * mean
}

/// Thu nhỏ `k` lần bằng cách lấy trung bình từng ô k x k.
fn downsample(px: &[u8], w: usize, h: usize, k: usize) -> Vec<u8> {
    let (ow, oh) = (w / k, h / k);
    let mut out = vec![0u8; ow * oh];
    for y in 0..oh {
        for x in 0..ow {
            let mut s = 0u32;
            for dy in 0..k {
                for dx in 0..k {
                    s += px[(y * k + dy) * w + x * k + dx] as u32;
                }
            }
            out[y * ow + x] = (s / (k * k) as u32) as u8;
        }
    }
    out
}

/// Khác biệt biểu đồ giữa hai khung: 0 (giống) .. 2 (khác hoàn toàn).
fn hist_diff(a: &[f64; 32], b: &[f64; 32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum()
}

/// Tìm độ dịch (dx, dy) khiến khung `b` khớp với khung `a` nhất, dò trong ±4 điểm ảnh
/// trên ảnh 80x45 (tức ±16 điểm trên ảnh 320x180). Trả về (dx, dy).
///
/// Nếu dịch đi cũng không khớp hơn hẳn so với đứng yên (cảnh ít chi tiết như trời,
/// tường trơn) thì coi như không có chuyển động, tránh đo nhầm thành rung.
fn global_shift(a: &[u8], b: &[u8]) -> (f64, f64) {
    let (sw, sh) = (SW as i32, SH as i32);
    const R: i32 = 4;
    let mut best = (0i32, 0i32, f64::MAX);
    let mut err0 = 0.0;
    for dy in -R..=R {
        for dx in -R..=R {
            let mut sad = 0u64;
            let mut n = 0u64;
            // Chỉ so phần giữa để khỏi lọt ra ngoài biên.
            for y in R..sh - R {
                let row_a = (y * sw) as usize;
                let row_b = ((y + dy) * sw) as usize;
                for x in R..sw - R {
                    let pa = a[row_a + x as usize] as i32;
                    let pb = b[row_b + (x + dx) as usize] as i32;
                    sad += (pa - pb).unsigned_abs() as u64;
                    n += 1;
                }
            }
            let err = sad as f64 / n as f64;
            if dx == 0 && dy == 0 {
                err0 = err;
            }
            if err < best.2 {
                best = (dx, dy, err);
            }
        }
    }
    // Phải khớp tốt hơn đứng yên ít nhất 15% và 1 mức xám mới tin là có dịch chuyển.
    if err0 - best.2 < (0.15 * err0).max(1.0) {
        return (0.0, 0.0);
    }
    (best.0 as f64, best.1 as f64)
}

/// dHash: thu nhỏ về 9x8, mỗi bit = điểm ảnh bên trái sáng hơn bên phải.
fn dhash(px: &[u8]) -> u64 {
    let mut cells = [0f64; 72];
    for cy in 0..8 {
        for cx in 0..9 {
            let (x0, x1) = (cx * W / 9, (cx + 1) * W / 9);
            let (y0, y1) = (cy * H / 8, (cy + 1) * H / 8);
            let mut s = 0u64;
            for y in y0..y1 {
                for x in x0..x1 {
                    s += px[y * W + x] as u64;
                }
            }
            cells[cy * 9 + cx] = s as f64 / ((x1 - x0) * (y1 - y0)) as f64;
        }
    }
    let mut h = 0u64;
    for cy in 0..8 {
        for cx in 0..8 {
            if cells[cy * 9 + cx] > cells[cy * 9 + cx + 1] {
                h |= 1 << (cy * 8 + cx);
            }
        }
    }
    h
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Kết quả thô của một video trước khi gắn nhãn.
struct Raw {
    frames: Vec<FrameStat>,
    hashes: Vec<u64>,
    /// Dịch chuyển (dx, dy) giữa khung i-1 và i (phần tử 0 = (0,0)).
    shifts: Vec<(f64, f64)>,
}

fn decode_gray(src: &str, cancel: &AtomicBool) -> Result<Raw> {
    let mut c = command(&ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-i", src, "-an", "-vf"]);
    c.arg(format!("fps={FPS},scale={W}:{H}:flags=area,format=gray"));
    c.args(["-f", "rawvideo", "-"]);
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| CoreError::Ffmpeg(format!("không chạy được ffmpeg: {e}")))?;
    let mut stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b);
        b
    });
    let mut out = child.stdout.take().unwrap();
    let mut buf = vec![0u8; W * H];
    let mut raw = Raw { frames: Vec::new(), hashes: Vec::new(), shifts: Vec::new() };
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CoreError::Cancelled);
        }
        match out.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        let st = frame_stat(&buf);
        let shift = match raw.frames.last() {
            // Cảnh trơn (trời, tường) không đủ chi tiết để đo chuyển động đáng tin.
            Some(prev) if prev.detail >= FLAT_DETAIL && st.detail >= FLAT_DETAIL => global_shift(&prev.small, &st.small),
            Some(_) => (0.0, 0.0),
            None => (0.0, 0.0),
        };
        raw.hashes.push(dhash(&buf));
        raw.shifts.push(shift);
        raw.frames.push(st);
    }
    let status = child.wait()?;
    let err = err_thread.join().unwrap_or_default();
    if !status.success() && raw.frames.is_empty() {
        return Err(CoreError::Ffmpeg(tail(&err, 6)));
    }
    Ok(raw)
}

/// Độ to tiếng (dBFS) và "giống tiếng nói" cho từng đoạn 20 ms.
fn decode_audio_energy(src: &str, cancel: &AtomicBool) -> Result<Vec<f64>> {
    const SR: usize = 8000;
    const WIN: usize = SR / 50; // 20 ms
    let mut c = command(&ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-i", src, "-vn", "-af", "highpass=f=300,lowpass=f=3400", "-ac", "1", "-ar"]);
    c.arg(SR.to_string());
    c.args(["-f", "s16le", "-"]);
    c.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = c.spawn().map_err(|e| CoreError::Ffmpeg(format!("không chạy được ffmpeg: {e}")))?;
    let mut out = child.stdout.take().unwrap();
    let mut buf = vec![0u8; WIN * 2];
    let mut energies = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CoreError::Cancelled);
        }
        match out.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        let mut s = 0f64;
        for ch in buf.chunks_exact(2) {
            let v = i16::from_le_bytes([ch[0], ch[1]]) as f64 / 32768.0;
            s += v * v;
        }
        let rms = (s / WIN as f64).sqrt();
        energies.push(20.0 * rms.max(1e-6).log10());
    }
    let _ = child.wait();
    Ok(energies)
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
}

fn percentile(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s[((s.len() - 1) as f64 * p).round() as usize]
}

/// Đánh giá tiếng nói trong đoạn [a, b) giây. Trả về (độ to dBFS, có giống tiếng nói không).
fn speech_in(energy: &[f64], a: f64, b: f64) -> (f64, bool) {
    let (i0, i1) = (((a * 50.0) as usize).min(energy.len()), ((b * 50.0) as usize).min(energy.len()));
    let seg = &energy[i0..i1];
    if seg.len() < 25 {
        return (-90.0, false);
    }
    let loud = percentile(seg, 0.9);
    let floor = percentile(seg, 0.2);
    // Tỉ lệ khung "có tiếng" (to hơn nền ít nhất 10 dB).
    let voiced = seg.iter().filter(|e| **e > floor + 10.0).count() as f64 / seg.len() as f64;
    // Độ biến thiên giữa các khung liền nhau: tiếng nói lên xuống liên tục.
    let flux = mean(&seg.windows(2).map(|w| (w[1] - w[0]).abs()).collect::<Vec<_>>());
    let speech = loud > -40.0 && loud - floor > 15.0 && (0.25..=0.92).contains(&voiced) && flux > 2.5;
    (loud, speech)
}

/// Tìm điểm cắt cảnh. Trả về các đoạn (bắt đầu, kết thúc, số thứ tự cảnh quay).
/// Cảnh quay dài được chia thành nhiều đoạn bằng nhau, các đoạn đó chung số cảnh quay.
fn segments(raw: &Raw, duration: f64) -> Vec<(f64, f64, u32)> {
    let mut cuts = vec![0.0];
    for i in 1..raw.frames.len() {
        let d = hist_diff(&raw.frames[i - 1].hist, &raw.frames[i].hist);
        let t = i as f64 / FPS;
        // Cắt khi biểu đồ đổi mạnh, hoặc đổi vừa và hình cũng khác hẳn.
        let big_jump = d > 0.7 || (d > 0.45 && hamming(raw.hashes[i - 1], raw.hashes[i]) > 22);
        if big_jump && t - cuts.last().unwrap() >= MIN_SCENE && duration - t >= MIN_SCENE {
            cuts.push(t);
        }
    }
    cuts.push(duration);

    let mut out = Vec::new();
    for (shot, w) in cuts.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let n = ((b - a) / MAX_SEGMENT).ceil().max(1.0) as usize;
        for k in 0..n {
            out.push((a + (b - a) * k as f64 / n as f64, a + (b - a) * (k + 1) as f64 / n as f64, shot as u32));
        }
    }
    out
}

/// Phân tích một file. Không gắn nhãn "Trùng" (cần so với các file khác, làm ở `finalize`).
pub fn analyze_media(project_dir: &Path, m: &MediaItem, cancel: &AtomicBool) -> Result<Vec<Scene>> {
    let src = m.proxy.clone().unwrap_or_else(|| m.path.clone());
    if m.kind == MediaKind::Audio {
        return Ok(vec![]);
    }
    let raw = decode_gray(&src, cancel)?;
    if raw.frames.is_empty() {
        return Err(CoreError::Ffmpeg(format!("không đọc được khung hình nào từ '{}'", m.name)));
    }

    if m.kind == MediaKind::Image {
        let f = &raw.frames[0];
        let mut s = Scene {
            id: new_id("scene"),
            start: 0.0,
            end: 0.0,
            sharpness: Some(f.sharpness),
            detail: Some(f.detail),
            brightness: Some(f.brightness),
            hash: Some(format!("{:016x}", raw.hashes[0])),
            thumb: m.thumb.clone(),
            tags: vec![SceneTag::Photo],
            ..Default::default()
        };
        if f.brightness < 40.0 {
            s.tags.push(SceneTag::Dark);
        }
        if f.brightness > 215.0 || f.clipped > 0.35 {
            s.tags.push(SceneTag::Overexposed);
        }
        return Ok(vec![s]);
    }

    let duration = if m.duration > 0.0 { m.duration } else { raw.frames.len() as f64 / FPS };
    let energy = if m.has_audio { decode_audio_energy(&src, cancel).unwrap_or_default() } else { vec![] };
    let mut scenes = Vec::new();
    for (a, b, shot) in segments(&raw, duration) {
        let (i0, i1) = (((a * FPS) as usize).min(raw.frames.len() - 1), ((b * FPS) as usize).clamp(1, raw.frames.len()));
        let i1 = i1.max(i0 + 1);
        let fr = &raw.frames[i0..i1];
        // Bỏ 1/6 đầu và cuối đoạn khi tính điểm (tránh khung chuyển cảnh).
        let skip = fr.len() / 6;
        let core = if fr.len() > 4 { &fr[skip..fr.len() - skip] } else { fr };
        let sharp = percentile(&core.iter().map(|f| f.sharpness).collect::<Vec<_>>(), 0.6);
        let detail = percentile(&core.iter().map(|f| f.detail).collect::<Vec<_>>(), 0.6);
        let bright = mean(&core.iter().map(|f| f.brightness).collect::<Vec<_>>());
        let clipped = mean(&core.iter().map(|f| f.clipped).collect::<Vec<_>>());
        let sh = &raw.shifts[(i0 + 1).min(i1 - 1)..i1];
        let motion = mean(&sh.iter().map(|(x, y)| (x * x + y * y).sqrt()).collect::<Vec<_>>());
        // Rung = độ thay đổi vận tốc giữa các khung (gia tốc), và hay đổi chiều.
        let accel: Vec<f64> = sh.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).collect();
        let shake = mean(&accel);
        let mid = ((i0 + i1) / 2).min(raw.hashes.len() - 1);
        let (audio_db, speech) = if energy.is_empty() { (None, false) } else {
            let (db, sp) = speech_in(&energy, a, b);
            (Some(db), sp)
        };

        let mut tags = Vec::new();
        if bright < 45.0 {
            tags.push(SceneTag::Dark);
        }
        if bright > 215.0 || clipped > 0.35 {
            tags.push(SceneTag::Overexposed);
        }
        if shake > 1.6 && motion > 0.8 && detail >= FLAT_DETAIL {
            tags.push(SceneTag::Shaky);
        }
        if speech {
            tags.push(SceneTag::Speech);
        }
        scenes.push(Scene {
            id: new_id("scene"),
            start: a,
            end: b,
            shot,
            sharpness: Some(sharp),
            detail: Some(detail),
            brightness: Some(bright),
            motion: Some(motion),
            shake: Some(shake),
            audio_db,
            hash: Some(format!("{:016x}", raw.hashes[mid])),
            tags,
            ..Default::default()
        });
    }

    make_scene_thumbs(project_dir, m, &src, &mut scenes)?;
    Ok(scenes)
}

/// Thumbnail giữa mỗi cảnh, rộng 320px. Một lần chạy ffmpeg cho cả file.
fn make_scene_thumbs(project_dir: &Path, m: &MediaItem, src: &str, scenes: &mut [Scene]) -> Result<()> {
    let dir = data_dir(project_dir, "thumbs").join(&m.id);
    std::fs::create_dir_all(&dir)?;
    for (i, s) in scenes.iter_mut().enumerate() {
        let out = dir.join(format!("s{i:03}.jpg"));
        let at = s.start + s.duration() / 2.0;
        let mut c = command(&ffmpeg_bin());
        c.args(["-hide_banner", "-loglevel", "error", "-y", "-ss", &format!("{at:.3}"), "-i", src]);
        c.args(["-frames:v", "1", "-vf", "scale=320:-2", "-q:v", "5"]).arg(&out);
        if run(c).is_ok() {
            s.thumb = Some(out.to_string_lossy().to_string());
        }
    }
    Ok(())
}

/// Bước cuối sau khi đã phân tích mọi file: so sánh giữa các cảnh để
/// gắn nhãn Mờ (so với mức chung), Trùng, Đẹp và tính điểm.
pub fn finalize(media: &mut [MediaItem]) {
    // Độ nét: dùng ngưỡng tương đối, vì máy quay/độ phân giải khác nhau thì thang đo khác.
    let all_sharp: Vec<f64> = media.iter().flat_map(|m| m.scenes.iter().filter_map(|s| s.sharpness)).collect();
    let med = percentile(&all_sharp, 0.5).max(1.0);

    for m in media.iter_mut() {
        for s in m.scenes.iter_mut() {
            s.tags.retain(|t| !matches!(t, SceneTag::Blurry | SceneTag::Duplicate | SceneTag::Good));
            s.dup_of = None;
            let sharp = s.sharpness.unwrap_or(med);
            let detail = s.detail.unwrap_or(sharp);
            // Cảnh trơn (trời, tường, sương): quá ít chi tiết để kết luận là mờ.
            let flat = detail < FLAT_DETAIL;
            // Mờ: mất chi tiết nhỏ so với chi tiết lớn, hoặc kém nét hẳn so với cả project.
            let ratio = sharp / detail.max(0.01);
            if !flat && (ratio < 0.22 || sharp < med * 0.12) {
                s.tags.push(SceneTag::Blurry);
            }
            // Điểm: độ nét (40), độ sáng vừa phải (25), không rung (20), có tiếng nói (15).
            let sharp_score = if flat { 28.0 } else if s.tags.contains(&SceneTag::Blurry) { 5.0 } else { (sharp / (med * 1.5)).min(1.0) * 40.0 };
            let b = s.brightness.unwrap_or(128.0);
            let bright_score = (1.0 - ((b - 125.0).abs() / 125.0)).max(0.0) * 25.0;
            let shake_score = (1.0 - s.shake.unwrap_or(0.0) / 3.0).clamp(0.0, 1.0) * 20.0;
            let speech_score = if s.tags.contains(&SceneTag::Speech) { 15.0 } else { 5.0 };
            s.score = ((sharp_score + bright_score + shake_score + speech_score) * 10.0).round() / 10.0;
        }
    }

    // Trùng: dHash gần giống (<= 8 bit khác). Giữ cảnh điểm cao hơn.
    // Chỉ so các cảnh thuộc cảnh quay khác nhau (các đoạn liền nhau của một cảnh quay dài
    // vốn giống nhau, không tính là trùng).
    struct Item {
        mi: usize,
        si: usize,
        hash: u64,
        score: f64,
        shot: (usize, u32),
    }
    let mut items: Vec<Item> = Vec::new();
    for (mi, m) in media.iter().enumerate() {
        for (si, s) in m.scenes.iter().enumerate() {
            if let Some(h) = s.hash.as_deref().and_then(|h| u64::from_str_radix(h, 16).ok()) {
                items.push(Item { mi, si, hash: h, score: s.score, shot: (mi, s.shot) });
            }
        }
    }
    items.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<usize> = Vec::new();
    for i in 0..items.len() {
        let dup = kept.iter().copied().find(|&k| items[k].shot != items[i].shot && hamming(items[k].hash, items[i].hash) <= 8);
        match dup {
            Some(k) => {
                let keep_id = media[items[k].mi].scenes[items[k].si].id.clone();
                let s = &mut media[items[i].mi].scenes[items[i].si];
                s.tags.push(SceneTag::Duplicate);
                s.dup_of = Some(keep_id);
            }
            None => kept.push(i),
        }
    }

    for m in media.iter_mut() {
        for s in m.scenes.iter_mut() {
            let flat = s.detail.unwrap_or(0.0) < FLAT_DETAIL;
            if !s.tags.iter().any(|t| t.is_defect()) && (s.sharpness.unwrap_or(0.0) >= med * 0.5 || flat) {
                s.tags.insert(0, SceneTag::Good);
            }
        }
    }
}

/// Tiến độ phân tích.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub name: String,
}

/// Phân tích nhiều file song song. `jobs` = số file chạy cùng lúc.
///
/// `rayon` chia việc cho nhiều luồng CPU. `on_done` được gọi mỗi khi xong một file
/// (từ luồng bất kỳ, nên phải là `Fn + Sync`: gọi được an toàn từ nhiều luồng).
pub fn analyze_all(
    project_dir: &Path,
    media: &[MediaItem],
    jobs: usize,
    cancel: &AtomicBool,
    on_done: &(dyn Fn(Progress, &MediaItem, Result<Vec<Scene>>) + Sync),
) {
    let total = media.len();
    let done = AtomicUsize::new(0);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(jobs.max(1)).build().expect("tạo thread pool");
    pool.install(|| {
        media.par_iter().for_each(|m| {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let r = analyze_media(project_dir, m, cancel);
            let n = done.fetch_add(1, Ordering::SeqCst) + 1;
            on_done(Progress { done: n, total, name: m.name.clone() }, m, r);
        });
    });
}

/// Số file nên phân tích song song: mỗi file chiếm 1 tiến trình giải mã + 1 luồng tính.
pub fn default_jobs() -> usize {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    (cpus / 4).clamp(1, 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(f: impl Fn(usize, usize) -> u8) -> Vec<u8> {
        let mut v = vec![0u8; W * H];
        for y in 0..H {
            for x in 0..W {
                v[y * W + x] = f(x, y);
            }
        }
        v
    }

    #[test]
    fn do_net_va_mo() {
        let sharp = frame(|x, y| if (x / 4 + y / 4) % 2 == 0 { 230 } else { 20 });
        let flat = frame(|x, _| (x as f64 / W as f64 * 200.0) as u8);
        assert!(frame_stat(&sharp).sharpness > 100.0 * frame_stat(&flat).sharpness.max(0.1));
    }

    #[test]
    fn uoc_luong_dich_chuyen() {
        let pat = |x: i32, y: i32| (((x * 37 + y * 91) % 255) as u8) ^ ((x * y) as u8);
        let a = frame(|x, y| pat(x as i32, y as i32));
        let b = frame(|x, y| pat(x as i32 + 12, y as i32 - 8)); // dịch (-12,+8) điểm ảnh 320x180
        let (sa, sb) = (frame_stat(&a), frame_stat(&b));
        let (dx, dy) = global_shift(&sa.small, &sb.small);
        assert_eq!((dx, dy), (-3.0, 2.0));
    }

    #[test]
    fn dhash_giong_va_khac() {
        let a = frame(|x, y| ((x + y) % 256) as u8);
        let b = frame(|x, y| ((x + y) % 256).saturating_add(3) as u8);
        let c = frame(|x, y| ((W - x) * 2 + y) as u8);
        assert!(hamming(dhash(&a), dhash(&b)) <= 2);
        assert!(hamming(dhash(&a), dhash(&c)) > 20);
    }
}
