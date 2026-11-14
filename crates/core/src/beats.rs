//! Tìm nhịp (beat) của bài nhạc để cắt cảnh khớp nhịp.
//!
//! Cách làm (không dùng AI):
//! 1. Giải mã âm thanh thành mono 11025 Hz.
//! 2. Chia thành các khung ~23 ms, đo năng lượng; "onset" = mức tăng năng lượng
//!    so với khung trước (tiếng trống, tiếng gảy đàn tạo bước nhảy năng lượng).
//! 3. Tìm tempo: tự tương quan (autocorrelation) của đường onset, chọn chu kỳ
//!    trong khoảng 70–180 BPM lặp lại mạnh nhất.
//! 4. Tìm pha: dịch lưới nhịp sao cho trùng nhiều onset nhất.
//! 5. Tinh chỉnh từng nhịp về đỉnh onset gần nhất (±2 khung).

use crate::error::{CoreError, Result};
use crate::ffmpeg::{command, ffmpeg_bin, tail};
use serde::Serialize;
use std::io::Read;
use std::process::Stdio;

const SR: usize = 11025;
const HOP: usize = 256; // ~23 ms
const FRAME_RATE: f64 = SR as f64 / HOP as f64;

#[derive(Debug, Clone, Serialize)]
pub struct Beats {
    pub bpm: f64,
    /// Thời điểm các nhịp (giây).
    pub beats: Vec<f64>,
    /// Nhịp mạnh đầu ô nhịp (giả định nhịp 4/4): mỗi 4 nhịp.
    pub downbeats: Vec<f64>,
    /// Độ tin cậy 0..1 (thấp = nhạc không rõ nhịp, ví dụ nhạc nền ambient).
    pub confidence: f64,
}

/// Đọc âm thanh mono 11025 Hz dạng số thực.
fn decode(path: &str, max_secs: f64) -> Result<Vec<f32>> {
    let mut c = command(&ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-i", path, "-vn", "-ac", "1", "-ar", &SR.to_string()]);
    c.args(["-t", &format!("{max_secs}"), "-f", "f32le", "-"]);
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    let out = c.output().map_err(|e| CoreError::Ffmpeg(format!("không chạy được ffmpeg: {e}")))?;
    if !out.status.success() {
        return Err(CoreError::Ffmpeg(tail(&out.stderr, 6)));
    }
    let mut buf = out.stdout.as_slice();
    let mut v = Vec::with_capacity(buf.len() / 4);
    let mut b = [0u8; 4];
    while buf.read_exact(&mut b).is_ok() {
        v.push(f32::from_le_bytes(b));
    }
    Ok(v)
}

/// Đường onset: mức tăng năng lượng (log) giữa các khung, đã trừ trung bình cục bộ.
fn onset_envelope(samples: &[f32]) -> Vec<f64> {
    let n = samples.len() / HOP;
    let mut energy = Vec::with_capacity(n);
    for i in 0..n {
        let s: f64 = samples[i * HOP..(i + 1) * HOP].iter().map(|x| (*x as f64) * (*x as f64)).sum();
        energy.push((s / HOP as f64 + 1e-10).ln());
    }
    let mut onset: Vec<f64> = (0..n).map(|i| if i == 0 { 0.0 } else { (energy[i] - energy[i - 1]).max(0.0) }).collect();
    // Trừ trung bình trượt ~0.5 giây để bỏ độ to chung.
    let w = (FRAME_RATE * 0.25) as usize;
    let copy = onset.clone();
    for i in 0..n {
        let (a, b) = (i.saturating_sub(w), (i + w + 1).min(n));
        let m = copy[a..b].iter().sum::<f64>() / (b - a) as f64;
        onset[i] = (copy[i] - m).max(0.0);
    }
    onset
}

/// Phân tích nhịp từ dữ liệu mẫu (tách riêng để test không cần ffmpeg).
pub fn detect_from_samples(samples: &[f32]) -> Beats {
    let onset = onset_envelope(samples);
    let n = onset.len();
    let duration = samples.len() as f64 / SR as f64;
    if n < (FRAME_RATE * 4.0) as usize {
        return Beats { bpm: 0.0, beats: vec![], downbeats: vec![], confidence: 0.0 };
    }
    // Tự tương quan cho các chu kỳ tương ứng 70..180 BPM.
    let lag_min = (FRAME_RATE * 60.0 / 180.0).floor() as usize;
    let lag_max = (FRAME_RATE * 60.0 / 70.0).ceil() as usize;
    // Làm mượt nhẹ đường onset để nhịp không rơi đúng khung vẫn tương quan tốt.
    let smooth: Vec<f64> = (0..n).map(|i| {
        let a = if i > 0 { onset[i - 1] } else { 0.0 };
        let c = if i + 1 < n { onset[i + 1] } else { 0.0 };
        (a + 2.0 * onset[i] + c) / 4.0
    }).collect();
    let energy0: f64 = smooth.iter().map(|x| x * x).sum::<f64>().max(1e-9);
    let mut best = (lag_min, f64::MIN);
    let mut scores = Vec::new();
    let mut raw = Vec::new();
    for lag in lag_min..=lag_max.min(n - 1) {
        let s: f64 = (lag..n).map(|i| smooth[i] * smooth[i - lag]).sum::<f64>() / (n - lag) as f64;
        raw.push(s);
        // Ưu tiên nhẹ tempo gần 120 BPM (phổ biến) để tránh chọn nhầm nửa/gấp đôi.
        let bpm = 60.0 * FRAME_RATE / lag as f64;
        let w = (-((bpm / 120.0).log2()).powi(2) / (2.0 * 0.9f64.powi(2))).exp();
        let s = s * (0.6 + 0.4 * w);
        scores.push(s);
        if s > best.1 {
            best = (lag, s);
        }
    }
    // Lỗi "nửa tempo": nếu chu kỳ bằng một nửa cũng lặp lại gần mạnh như vậy
    // thì nhạc thực ra nhanh gấp đôi (vd. 140 BPM bị đo thành 70).
    let h0 = best.0 / 2;
    if h0 > lag_min {
        let cand = (h0 - 1..=h0 + 1).filter(|l| *l >= lag_min && l - lag_min < raw.len()).max_by(|a, b| raw[a - lag_min].total_cmp(&raw[b - lag_min]));
        if let Some(h) = cand {
            if raw[h - lag_min] >= 0.5 * raw[best.0 - lag_min] {
                best = (h, scores[h - lag_min]);
            }
        }
    }
    // Nội suy parabol để có tempo không bị làm tròn theo khung.
    let li = best.0 - lag_min;
    let mut lag = best.0 as f64;
    if li > 0 && li + 1 < scores.len() {
        let (a, b, c) = (scores[li - 1], scores[li], scores[li + 1]);
        let d = a - 2.0 * b + c;
        if d.abs() > 1e-12 {
            lag += 0.5 * (a - c) / d;
        }
    }
    let period = lag; // tính bằng khung
    let bpm = 60.0 * FRAME_RATE / period;
    let mean_s = scores.iter().sum::<f64>() / scores.len() as f64;
    let confidence = ((best.1 - mean_s) / (energy0 / n as f64)).clamp(0.0, 1.0);

    // Pha: thử mọi độ lệch trong một chu kỳ.
    let mut best_phase = (0.0, f64::MIN);
    let steps = period.ceil() as usize;
    for k in 0..steps {
        let ph = k as f64;
        let mut s = 0.0;
        let mut t = ph;
        while (t as usize) < n {
            s += onset[t.round() as usize % n];
            t += period;
        }
        if s > best_phase.1 {
            best_phase = (ph, s);
        }
    }
    let mut beats = Vec::new();
    let mut t = best_phase.0;
    while t < n as f64 {
        // Tinh chỉnh về đỉnh onset gần nhất (±2 khung). Lấy giữa khung làm thời điểm.
        let i = (t.round() as usize).min(n - 1);
        let j = (i.saturating_sub(2)..=(i + 2).min(n - 1)).max_by(|a, b| onset[*a].total_cmp(&onset[*b])).unwrap_or(i);
        let sec = (j as f64 + 0.5) * HOP as f64 / SR as f64 - 0.01;
        if sec <= duration {
            beats.push((sec * 1000.0).round() / 1000.0);
        }
        t += period;
    }
    // Nhịp mạnh: chọn pha (0..3) có tổng onset lớn nhất.
    let mut best_db = (0usize, f64::MIN);
    for off in 0..4 {
        let s: f64 = beats.iter().skip(off).step_by(4).map(|b| onset[((b * FRAME_RATE).round() as usize).min(n - 1)]).sum();
        if s > best_db.1 {
            best_db = (off, s);
        }
    }
    let downbeats = beats.iter().skip(best_db.0).step_by(4).copied().collect();
    // Độ tin cậy: onset tại các nhịp mạnh hơn mức trung bình bao nhiêu lần.
    // Nhạc rõ nhịp: gấp nhiều lần. Âm đều/ambient: xấp xỉ 1.
    let at_beats: Vec<f64> = beats.iter().map(|b| onset[((b * FRAME_RATE).round() as usize).min(n - 1)]).collect();
    let avg = onset.iter().sum::<f64>() / n as f64;
    let ratio = if avg > 1e-9 { at_beats.iter().sum::<f64>() / at_beats.len().max(1) as f64 / avg } else { 0.0 };
    let beat_mean = at_beats.iter().sum::<f64>() / at_beats.len().max(1) as f64;
    // Thêm điều kiện độ mạnh tuyệt đối: âm đều có thể có nhịp "giả" rất yếu.
    let strength = ((beat_mean - 0.3) / 1.2).clamp(0.0, 1.0);
    let confidence = ((ratio - 1.5) / 6.0).clamp(0.0, 1.0).min(confidence * 3.0).min(strength);
    Beats { bpm: (bpm * 10.0).round() / 10.0, beats, downbeats, confidence: (confidence * 100.0).round() / 100.0 }
}

/// Tìm nhịp của một file (chỉ đọc tối đa `max_secs` giây đầu).
pub fn detect(path: &str, max_secs: f64) -> Result<Beats> {
    let samples = decode(path, max_secs)?;
    Ok(detect_from_samples(&samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tạo tiếng trống giả: tiếng "tách" ngắn mỗi nhịp, trên nền nhiễu nhẹ.
    fn click_track(bpm: f64, secs: f64, offset: f64) -> Vec<f32> {
        let n = (secs * SR as f64) as usize;
        let mut v = vec![0f32; n];
        let mut seed = 12345u32;
        for x in v.iter_mut() {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            *x = ((seed >> 16) as f32 / 65536.0 - 0.5) * 0.02;
        }
        let period = 60.0 / bpm;
        let mut t = offset;
        while t < secs {
            let i0 = (t * SR as f64) as usize;
            for k in 0..(SR / 50).min(n.saturating_sub(i0)) {
                let decay = (-(k as f32) / 60.0).exp();
                v[i0 + k] += 0.8 * decay * ((k as f32) * 0.9).sin();
            }
            t += period;
        }
        v
    }

    #[test]
    fn tim_tempo_va_nhip() {
        for (bpm, off) in [(120.0, 0.25), (96.0, 0.1), (140.0, 0.4), (75.0, 0.3), (172.0, 0.05)] {
            let b = detect_from_samples(&click_track(bpm, 20.0, off));
            assert!((b.bpm - bpm).abs() < 2.0, "tempo {} thay vì {bpm}", b.bpm);
            // Nhịp đầu tiên gần offset (sai số < 40 ms).
            assert!((b.beats[0] - off).abs() < 0.04, "nhịp đầu {} thay vì {off}", b.beats[0]);
            assert!(b.confidence > 0.2, "độ tin cậy {}", b.confidence);
        }
    }
}
