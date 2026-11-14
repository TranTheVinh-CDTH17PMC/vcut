//! Gói giọng nói: nhận dạng lời nói bằng Whisper, rồi từ transcript tạo ra
//! các đề xuất sửa timeline: bỏ "ừm, à", cắt khoảng lặng, cắt theo từ, phụ đề.
//!
//! Whisper chạy bằng chương trình `whisper-cli` của dự án whisper.cpp (bản build sẵn
//! có CUDA cho Windows), gọi như gọi ffmpeg. Cách này tránh phải cài CUDA Toolkit
//! và LLVM để biên dịch, đúng tinh thần "gọn nhẹ" của app.
//!
//! Mọi đề xuất chỉ dùng các Op có sẵn (`split_clip`, `remove_clip`, `add_text`…),
//! nên keyframe, filter, chuyển cảnh… của clip được giữ nguyên.

use crate::error::{invalid, CoreError, Result};
use crate::ffmpeg::{command, ffmpeg_bin, run, tail};
use crate::ops::{self, Op};
use crate::project::{MediaItem, Project, TextAlign, TextStyle, TextWord};
use crate::store::{data_dir, read_json, write_json_atomic};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

pub const DEFAULT_MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";

#[cfg(windows)]
const CLI_NAMES: &[&str] = &["whisper-cli.exe", "main.exe"];
#[cfg(not(windows))]
const CLI_NAMES: &[&str] = &["whisper-cli", "main"];

// ---------------- Dữ liệu ----------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Word {
    /// Giây trong file gốc.
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// Độ tin cậy 0..1 của Whisper.
    #[serde(default)]
    pub prob: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub media_id: String,
    pub language: String,
    pub model: String,
    pub segments: Vec<Segment>,
}

impl Transcript {
    pub fn words(&self) -> impl Iterator<Item = &Word> {
        self.segments.iter().flat_map(|s| s.words.iter())
    }

    /// Lời nói trong đoạn [a, b) của file gốc.
    pub fn text_between(&self, a: f64, b: f64) -> String {
        let v: Vec<&str> = self.words().filter(|w| w.start >= a - 0.05 && w.end <= b + 0.05).map(|w| w.text.as_str()).collect();
        v.join(" ")
    }
}

pub fn transcript_path(dir: &Path, media_id: &str) -> PathBuf {
    data_dir(dir, "transcripts").join(format!("{media_id}.json"))
}

pub fn load_transcript(dir: &Path, media_id: &str) -> Option<Transcript> {
    read_json(&transcript_path(dir, media_id)).ok()
}

// ---------------- Tìm whisper-cli và model ----------------

/// Thư mục cài Whisper của app: `%APPDATA%\vlogcut\whisper` và `...\models`.
pub fn install_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vlogcut"))
}

fn find_file(dir: &Path, names: &[&str], depth: usize) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
            if names.iter().any(|x| x.eq_ignore_ascii_case(n)) {
                return Some(p);
            }
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.into_iter().find_map(|d| find_file(&d, names, depth - 1))
}

#[derive(Debug, Clone, Serialize)]
pub struct Tools {
    pub cli: Option<String>,
    pub model: Option<String>,
    /// Bản whisper-cli có CUDA (chạy trên GPU NVIDIA).
    pub gpu: bool,
}

impl Tools {
    pub fn ready(&self) -> bool {
        self.cli.is_some() && self.model.is_some()
    }
}

pub fn find_tools() -> Tools {
    let cli = std::env::var("VLOGCUT_WHISPER").ok().map(PathBuf::from).filter(|p| p.exists()).or_else(|| {
        install_dir().and_then(|d| find_file(&d.join("whisper"), CLI_NAMES, 3))
    });
    let model = std::env::var("VLOGCUT_WHISPER_MODEL").ok().map(PathBuf::from).filter(|p| p.exists()).or_else(|| {
        let dir = install_dir()?.join("models");
        let preferred = dir.join(DEFAULT_MODEL);
        if preferred.exists() {
            return Some(preferred);
        }
        // Model ggml-*.bin bất kỳ người dùng tự chép vào.
        std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("ggml-") && n.ends_with(".bin")).unwrap_or(false)
        })
    });
    // Bản CUDA có các thư viện cublas/cudart nằm cạnh exe.
    let gpu = cli
        .as_ref()
        .and_then(|c| c.parent())
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().contains("cublas")))
        .unwrap_or(false);
    Tools { cli: cli.map(|p| p.to_string_lossy().to_string()), model: model.map(|p| p.to_string_lossy().to_string()), gpu }
}

// ---------------- Nhận dạng ----------------

/// Câu mồi để Whisper ghi lại cả "ừm, à" (mặc định Whisper hay tự bỏ qua chúng).
const VI_PROMPT: &str = "Ừm, à, ờ... thì là, kiểu như là, ừ, ơ, ờm.";

#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    /// "vi", "en"… hoặc "auto".
    pub language: String,
    pub threads: usize,
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        TranscribeOptions { language: "vi".into(), threads: cpus.clamp(2, 8) }
    }
}

/// Tách âm thanh ra WAV 16 kHz mono (định dạng Whisper cần).
fn extract_wav(src: &str, out: &Path) -> Result<()> {
    let mut c = command(&ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y", "-i", src, "-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"]).arg(out);
    run(c).map(|_| ())
}

/// Nhận dạng giọng nói của một file. `progress` nhận 0..1.
pub fn transcribe(dir: &Path, m: &MediaItem, tools: &Tools, opts: &TranscribeOptions, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<Transcript> {
    let cli = tools.cli.as_deref().ok_or_else(|| invalid("chưa cài Whisper"))?;
    let model = tools.model.as_deref().ok_or_else(|| invalid("chưa có model Whisper"))?;
    if !m.has_audio {
        return Err(invalid(format!("'{}' không có âm thanh", m.name)));
    }
    let work = data_dir(dir, "transcripts");
    std::fs::create_dir_all(&work)?;
    let wav = work.join(format!("{}.wav", m.id));
    extract_wav(&m.path, &wav)?;
    let base = work.join(format!("{}.whisper", m.id));

    let run_once = |no_gpu: bool, progress: &mut dyn FnMut(f64)| -> Result<()> {
        let mut c = command(cli);
        // Thư viện CUDA nằm cạnh exe: chạy trong thư mục đó để Windows tìm thấy DLL.
        if let Some(d) = Path::new(cli).parent() {
            c.current_dir(d);
        }
        c.args(["-m", model, "-f"]).arg(&wav);
        c.args(["-l", &opts.language, "-t", &opts.threads.to_string(), "-ojf", "-pp", "-of"]).arg(&base);
        if opts.language == "vi" {
            c.args(["--prompt", VI_PROMPT]);
        }
        if no_gpu {
            c.arg("-ng");
        }
        c.stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = c.spawn().map_err(|e| CoreError::Ffmpeg(format!("không chạy được whisper-cli: {e}")))?;
        let stderr = child.stderr.take().unwrap();
        let mut log = Vec::new();
        // whisper-cli in tiến độ dạng "...progress = 45%" ra stderr.
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CoreError::Cancelled);
            }
            if let Some(p) = line.split("progress =").nth(1).and_then(|x| x.trim().trim_end_matches('%').trim().parse::<f64>().ok()) {
                progress((p / 100.0).clamp(0.0, 1.0));
            }
            log.push(line);
        }
        let status = child.wait()?;
        if !status.success() {
            let joined = log.join("\n");
            return Err(CoreError::Ffmpeg(format!("whisper-cli lỗi:\n{}", tail(joined.as_bytes(), 10))));
        }
        Ok(())
    };

    let result = run_once(false, progress);
    let result = match result {
        // Lỗi GPU (driver cũ, hết VRAM…) thì thử lại bằng CPU.
        Err(CoreError::Ffmpeg(_)) if tools.gpu && !cancel.load(Ordering::Relaxed) => run_once(true, progress),
        other => other,
    };
    let _ = std::fs::remove_file(&wav);
    result?;

    let json_path = base.with_extension("whisper.json");
    let raw = std::fs::read(&json_path)?;
    let _ = std::fs::remove_file(&json_path);
    let mut t = parse_whisper_json(&raw)?;
    t.media_id = m.id.clone();
    t.model = Path::new(model).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if t.language.is_empty() {
        t.language = opts.language.clone();
    }
    write_json_atomic(&transcript_path(dir, &m.id), &t)?;
    progress(1.0);
    Ok(t)
}

/// Đọc file JSON của whisper-cli (`-ojf`): mỗi đoạn có danh sách token kèm mốc thời gian (ms).
/// Token là mảnh chữ; token bắt đầu bằng dấu cách là đầu một từ mới.
pub fn parse_whisper_json(raw: &[u8]) -> Result<Transcript> {
    // Whisper đôi khi cắt giữa ký tự UTF-8 nhiều byte; đọc "lossy" để không lỗi cả file.
    let text = String::from_utf8_lossy(raw);
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let language = v["result"]["language"].as_str().unwrap_or("").to_string();
    let mut segments = Vec::new();
    for seg in v["transcription"].as_array().cloned().unwrap_or_default() {
        let s0 = seg["offsets"]["from"].as_f64().unwrap_or(0.0) / 1000.0;
        let s1 = seg["offsets"]["to"].as_f64().unwrap_or(0.0) / 1000.0;
        let mut words: Vec<Word> = Vec::new();
        for tok in seg["tokens"].as_array().cloned().unwrap_or_default() {
            let t = tok["text"].as_str().unwrap_or("");
            if t.starts_with("[_") || t.trim().is_empty() && !t.is_empty() && words.is_empty() {
                continue; // token đặc biệt như [_BEG_], [_TT_120]
            }
            if t.is_empty() || t.contains('\u{FFFD}') && t.trim_matches('\u{FFFD}').is_empty() {
                continue;
            }
            let a = tok["offsets"]["from"].as_f64().unwrap_or(0.0) / 1000.0;
            let b = tok["offsets"]["to"].as_f64().unwrap_or(0.0) / 1000.0;
            let p = tok["p"].as_f64().unwrap_or(0.0) as f32;
            let starts_word = t.starts_with(' ');
            let is_punct = t.trim().chars().all(|c| c.is_ascii_punctuation() || "…“”".contains(c));
            match words.last_mut() {
                Some(w) if !starts_word || is_punct => {
                    // Mảnh tiếp theo của cùng một từ, hoặc dấu câu dính vào từ trước.
                    w.text.push_str(t.trim_start());
                    if !is_punct {
                        w.end = w.end.max(b);
                        w.prob = w.prob.min(p);
                    }
                }
                _ => words.push(Word { start: a, end: b.max(a), text: t.trim().to_string(), prob: p }),
            }
        }
        words.retain(|w| !w.text.trim().is_empty());
        let text = seg["text"].as_str().unwrap_or("").trim().to_string();
        if !text.is_empty() {
            segments.push(Segment { start: s0, end: s1, text, words });
        }
    }
    Ok(Transcript { media_id: String::new(), language, model: String::new(), segments })
}

// ---------------- Dọn lời nói ----------------

/// Chuẩn hoá để so sánh: chữ thường, bỏ dấu câu ở hai đầu.
pub fn norm(s: &str) -> String {
    s.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Luôn là tiếng đệm.
const FILLERS: &[&str] = &["ừm", "ờm", "ưm", "hừm", "ừa", "ơ", "um", "umm", "uh", "uhm", "hmm", "mm", "ah", "eh", "er", "erm"];
/// Chỉ là tiếng đệm khi đứng riêng (có ngắt nghỉ trước/sau hoặc dấu phẩy), vì
/// "à", "ờ", "ừ" còn là từ có nghĩa ("thế à?", "ừ, được").
const SOFT_FILLERS: &[&str] = &["à", "ờ", "ừ", "a"];

pub fn is_filler(words: &[Word], i: usize) -> bool {
    let w = &words[i];
    let n = norm(&w.text);
    if FILLERS.contains(&n.as_str()) {
        return true;
    }
    if !SOFT_FILLERS.contains(&n.as_str()) {
        return false;
    }
    let gap_before = if i == 0 { 1.0 } else { w.start - words[i - 1].end };
    let gap_after = words.get(i + 1).map(|x| x.start - w.end).unwrap_or(1.0);
    let comma = w.text.ends_with(',') || w.text.ends_with("...") || w.text.ends_with('…');
    // "thế à?" có dấu hỏi: là câu hỏi, giữ lại.
    let question = w.text.ends_with('?');
    !question && (comma || (gap_before >= 0.15 && gap_after >= 0.15))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupOptions {
    #[serde(default = "yes")]
    pub remove_fillers: bool,
    /// Cắt khoảng lặng giữa các câu dài hơn số giây này. `None` = không cắt.
    #[serde(default)]
    pub max_pause: Option<f64>,
    /// Các từ/cụm từ cần cắt bỏ (vd. từ tục). Không phân biệt hoa thường.
    #[serde(default)]
    pub remove_words: Vec<String>,
    /// Chỉ xử lý các clip này; rỗng = mọi clip có transcript.
    #[serde(default)]
    pub clip_ids: Vec<String>,
}

fn yes() -> bool {
    true
}

impl Default for CleanupOptions {
    fn default() -> Self {
        CleanupOptions { remove_fillers: true, max_pause: Some(0.7), remove_words: vec![], clip_ids: vec![] }
    }
}

/// Đoạn cần bỏ (giây trong file gốc), kèm lý do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cut {
    pub start: f64,
    pub end: f64,
    pub kind: CutKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutKind {
    Filler,
    Pause,
    Word,
}

/// Khoảng đệm giữ lại quanh mỗi vết cắt để lời nói không bị cụt.
const PAD: f64 = 0.06;
/// Khoảng lặng được giữ lại sau khi cắt (để vẫn nghe tự nhiên).
const KEEP_PAUSE: f64 = 0.25;

pub fn find_cuts(t: &Transcript, opts: &CleanupOptions) -> Vec<Cut> {
    let words: Vec<Word> = t.words().cloned().collect();
    let mut cuts = Vec::new();
    if opts.remove_fillers {
        for i in 0..words.len() {
            if is_filler(&words, i) {
                cuts.push(Cut { start: words[i].start - PAD, end: words[i].end + PAD, kind: CutKind::Filler });
            }
        }
    }
    // Cắt theo từ / cụm từ: so khớp chuỗi các từ liên tiếp.
    for phrase in &opts.remove_words {
        let parts: Vec<String> = phrase.split_whitespace().map(norm).filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        for i in 0..words.len().saturating_sub(parts.len() - 1) {
            if (0..parts.len()).all(|k| norm(&words[i + k].text) == parts[k]) {
                cuts.push(Cut { start: words[i].start - PAD, end: words[i + parts.len() - 1].end + PAD, kind: CutKind::Word });
            }
        }
    }
    if let Some(max) = opts.max_pause {
        for w in words.windows(2) {
            let gap = w[1].start - w[0].end;
            if gap > max.max(KEEP_PAUSE + 0.1) {
                let half = KEEP_PAUSE / 2.0;
                cuts.push(Cut { start: w[0].end + half, end: w[1].start - half, kind: CutKind::Pause });
            }
        }
    }
    cuts.sort_by(|a, b| a.start.total_cmp(&b.start));
    // Gộp các vết cắt chồng nhau hoặc sát nhau (< 0.1 s).
    let mut merged: Vec<Cut> = Vec::new();
    for c in cuts {
        match merged.last_mut() {
            Some(m) if c.start <= m.end + 0.1 => m.end = m.end.max(c.end),
            _ => merged.push(c),
        }
    }
    merged.retain(|c| c.end - c.start >= 0.08);
    merged
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupPlan {
    pub ops: Vec<Op>,
    pub fillers: usize,
    pub pauses: usize,
    pub words: usize,
    /// Tổng số giây bị cắt.
    pub removed_secs: f64,
    pub description: String,
}

/// Tạo đề xuất dọn lời nói cho các clip có transcript.
///
/// Với mỗi đoạn cần bỏ trong một clip: tách clip ở cuối đoạn, tách ở đầu đoạn,
/// rồi xoá mẩu ở giữa. Làm từ cuối clip về đầu để vị trí các vết cắt trước không đổi.
pub fn cleanup_plan(dir: &Path, p: &Project, opts: &CleanupOptions) -> Result<CleanupPlan> {
    let mut ops: Vec<Op> = Vec::new();
    let (mut fillers, mut pauses, mut words, mut removed) = (0, 0, 0, 0.0);
    let mut staged = p.clone();
    let mut any_transcript = false;
    for clip in p.video.iter() {
        if !opts.clip_ids.is_empty() && !opts.clip_ids.contains(&clip.id) {
            continue;
        }
        let Some(t) = load_transcript(dir, &clip.media_id) else { continue };
        any_transcript = true;
        let mut cuts: Vec<Cut> = find_cuts(&t, opts)
            .into_iter()
            .filter_map(|c| {
                let (a, b) = (c.start.max(clip.src_in), c.end.min(clip.src_out));
                (b - a >= 0.08).then_some(Cut { start: a, end: b, ..c })
            })
            .collect();
        cuts.sort_by(|a, b| b.start.total_cmp(&a.start)); // từ cuối về đầu
        let mut n = 0;
        for c in cuts {
            // Vị trí tính từ đầu clip trên timeline (đã chia tốc độ).
            let at_end = (c.end - clip.src_in) / clip.speed;
            let at_start = (c.start - clip.src_in) / clip.speed;
            let len = (clip.src_out - clip.src_in) / clip.speed;
            let mut step: Vec<Op> = Vec::new();
            let mid_id = format!("{}_x{n}", clip.id);
            if at_end < len - 0.1 {
                step.push(Op::SplitClip { clip_id: clip.id.clone(), at: at_end, new_id: Some(format!("{}_k{n}", clip.id)) });
            }
            if at_start > 0.1 {
                step.push(Op::SplitClip { clip_id: clip.id.clone(), at: at_start, new_id: Some(mid_id.clone()) });
                step.push(Op::RemoveClip { clip_id: mid_id });
            } else {
                // Đoạn cần bỏ nằm ngay đầu clip: cắt bớt đầu clip thay vì tách.
                step.push(Op::TrimClip { clip_id: clip.id.clone(), src_in: Some(c.end), src_out: None });
            }
            // Bỏ qua vết cắt nào không hợp lệ (vd. tạo ra mẩu quá ngắn).
            match ops::apply(&staged, &step) {
                Ok(next) => {
                    staged = next;
                    ops.extend(step);
                    removed += c.end - c.start;
                    match c.kind {
                        CutKind::Filler => fillers += 1,
                        CutKind::Pause => pauses += 1,
                        CutKind::Word => words += 1,
                    }
                    n += 1;
                }
                Err(_) => continue,
            }
        }
    }
    if !any_transcript {
        return Err(invalid("chưa có clip nào được nhận dạng giọng nói. Hãy chạy Nhận dạng trong hộp thoại Giọng nói."));
    }
    let mut parts = Vec::new();
    if fillers > 0 {
        parts.push(format!("bỏ {fillers} tiếng đệm"));
    }
    if pauses > 0 {
        parts.push(format!("cắt {pauses} khoảng lặng"));
    }
    if words > 0 {
        parts.push(format!("cắt {words} chỗ có từ cần bỏ"));
    }
    let description = if parts.is_empty() {
        "Không tìm thấy gì cần dọn".to_string()
    } else {
        format!("Dọn lời nói: {} (ngắn đi {:.1}s)", parts.join(", "), removed)
    };
    Ok(CleanupPlan { ops, fillers, pauses, words, removed_secs: removed, description })
}

// ---------------- Phụ đề ----------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleOptions {
    #[serde(default = "yes")]
    pub karaoke: bool,
    #[serde(default)]
    pub align: TextAlign,
    /// Cỡ chữ (pixel khung xuất). `None` = tự tính theo khung hình.
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(default = "default_hl")]
    pub highlight_color: String,
    #[serde(default = "white")]
    pub color: String,
    /// Số ký tự tối đa mỗi dòng phụ đề. `None` = tự tính theo khung hình.
    #[serde(default)]
    pub max_chars: Option<usize>,
    /// Bỏ "ừm, à" khỏi phụ đề.
    #[serde(default = "yes")]
    pub hide_fillers: bool,
}

fn default_hl() -> String {
    "#FFD60A".into()
}
fn white() -> String {
    "#FFFFFF".into()
}

impl Default for SubtitleOptions {
    fn default() -> Self {
        serde_json::from_str("{}").expect("mặc định")
    }
}

pub const SUB_PREFIX: &str = "sub_";

/// Tạo phụ đề cho mọi clip có transcript (và đang có tiếng). Xoá phụ đề cũ do app tạo.
pub fn subtitle_plan(dir: &Path, p: &Project, opts: &SubtitleOptions) -> Result<(Vec<Op>, String)> {
    let s = p.settings;
    let portrait = s.height > s.width;
    let size = opts.size.unwrap_or((s.height.min(s.width) as f64 * if portrait { 0.075 } else { 0.06 }).round());
    let max_chars = opts.max_chars.unwrap_or(if portrait { 26 } else { 40 });
    let style = TextStyle {
        size,
        color: opts.color.clone(),
        outline: (size / 16.0).round().max(2.0),
        align: opts.align,
        anim_in: crate::project::TextAnim::None,
        karaoke: opts.karaoke,
        highlight_color: opts.highlight_color.clone(),
        ..Default::default()
    };

    let mut ops: Vec<Op> = p.texts.iter().filter(|t| t.id.starts_with(SUB_PREFIX)).map(|t| Op::RemoveText { text_id: t.id.clone() }).collect();
    let layout = p.video_layout();
    let mut n = 0;
    let mut any = false;
    for (ci, clip) in p.video.iter().enumerate() {
        let Some(t) = load_transcript(dir, &clip.media_id) else { continue };
        any = true;
        // Clip bị tắt tiếng (vd. chỉ làm hình nền cho nhạc) thì không cần phụ đề.
        if let crate::keyframe::Animatable::Static(v) = clip.volume {
            if v < 0.3 {
                continue;
            }
        }
        let all: Vec<Word> = t.words().cloned().collect();
        let span = layout[ci];
        // Thời gian trên timeline của một mốc trong file gốc.
        let tl = |src: f64| span.start + (src - clip.src_in) / clip.speed;
        let words: Vec<(usize, &Word)> = all.iter().enumerate().filter(|(_, w)| w.start >= clip.src_in - 0.05 && w.end <= clip.src_out + 0.05).collect();
        let mut chunk: Vec<&Word> = Vec::new();
        let flush = |chunk: &mut Vec<&Word>, next_start: Option<f64>, ops: &mut Vec<Op>, n: &mut usize| {
            if chunk.is_empty() {
                return;
            }
            let start = tl(chunk[0].start).max(span.start);
            let mut end = tl(chunk.last().unwrap().end) + 0.2;
            if let Some(ns) = next_start {
                end = end.min(tl(ns) - 0.02);
            }
            end = end.min(span.start + span.duration).max(start + 0.3);
            let text = chunk.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
            let words: Vec<TextWord> = chunk
                .iter()
                .map(|w| TextWord { text: w.text.clone(), start: (tl(w.start) - start).max(0.0), end: (tl(w.end) - start).max(0.0) })
                .collect();
            ops.push(Op::AddText { text, start, end, style: Some(style.clone()), id: Some(format!("{SUB_PREFIX}{n:04}")), words: Some(words) });
            *n += 1;
            chunk.clear();
        };
        for (wi, w) in words.iter() {
            if opts.hide_fillers && is_filler(&all, *wi) {
                continue;
            }
            if let Some(last) = chunk.last() {
                let len: usize = chunk.iter().map(|x| x.text.chars().count() + 1).sum::<usize>() + w.text.chars().count();
                let gap = w.start - last.end;
                let dur = w.end - chunk[0].start;
                let sentence_end = last.text.ends_with(['.', '?', '!', '…']) && chunk.len() >= 2;
                if len > max_chars || gap > 0.6 || dur > 3.2 || sentence_end {
                    flush(&mut chunk, Some(w.start), &mut ops, &mut n);
                }
            }
            chunk.push(w);
        }
        flush(&mut chunk, None, &mut ops, &mut n);
    }
    if !any {
        return Err(invalid("chưa có clip nào được nhận dạng giọng nói. Hãy chạy Nhận dạng trong hộp thoại Giọng nói."));
    }
    // Kiểm tra trước để báo lỗi rõ ràng.
    ops::apply(p, &ops)?;
    let desc = if n == 0 { "Không có lời nói nào để làm phụ đề".to_string() } else { format!("Phụ đề tự động: {n} dòng{}", if opts.karaoke { ", chữ chạy theo giọng" } else { "" }) };
    Ok((ops, desc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(start: f64, end: f64, text: &str) -> Word {
        Word { start, end, text: text.into(), prob: 0.9 }
    }

    #[test]
    fn doc_json_whisper() {
        let raw = r#"{"result":{"language":"vi"},"transcription":[
          {"offsets":{"from":0,"to":2400},"text":" Xin chào, ừm, các bạn.","tokens":[
            {"text":"[_BEG_]","offsets":{"from":0,"to":0},"p":0.9},
            {"text":" X","offsets":{"from":0,"to":200},"p":0.9},
            {"text":"in","offsets":{"from":200,"to":400},"p":0.8},
            {"text":" chào","offsets":{"from":400,"to":800},"p":0.95},
            {"text":",","offsets":{"from":800,"to":810},"p":0.99},
            {"text":" ừm","offsets":{"from":1000,"to":1300},"p":0.6},
            {"text":",","offsets":{"from":1300,"to":1310},"p":0.99},
            {"text":" các","offsets":{"from":1600,"to":1900},"p":0.9},
            {"text":" bạn","offsets":{"from":1900,"to":2300},"p":0.9},
            {"text":".","offsets":{"from":2300,"to":2400},"p":0.9},
            {"text":"[_TT_120]","offsets":{"from":2400,"to":2400},"p":0.9}
          ]}]}"#;
        let t = parse_whisper_json(raw.as_bytes()).unwrap();
        let ws: Vec<&str> = t.words().map(|w| w.text.as_str()).collect();
        assert_eq!(ws, vec!["Xin", "chào,", "ừm,", "các", "bạn."]);
        let first = t.words().next().unwrap();
        assert_eq!((first.start, first.end), (0.0, 0.4));
        assert_eq!(t.language, "vi");
    }

    #[test]
    fn tieng_dem() {
        let ws = vec![w(0.0, 0.3, "Thế"), w(0.3, 0.5, "à?"), w(1.0, 1.2, "à,"), w(1.3, 1.6, "mình"), w(2.0, 2.2, "ừm"), w(2.3, 2.5, "ừ"), w(2.5, 2.8, "được")];
        assert!(!is_filler(&ws, 1), "'thế à?' là câu hỏi");
        assert!(is_filler(&ws, 2), "'à,' có dấu phẩy là tiếng đệm");
        assert!(is_filler(&ws, 4), "'ừm' luôn là tiếng đệm");
        assert!(!is_filler(&ws, 5), "'ừ được' liền nhau là câu trả lời");
    }

    #[test]
    fn tim_vet_cat() {
        let t = Transcript {
            media_id: "m".into(),
            language: "vi".into(),
            model: String::new(),
            segments: vec![Segment {
                start: 0.0,
                end: 8.0,
                text: String::new(),
                words: vec![w(0.0, 0.4, "Hôm"), w(0.4, 0.8, "nay"), w(1.2, 1.5, "ừm,"), w(1.8, 2.0, "đi"), w(4.0, 4.3, "chơi"), w(4.3, 4.6, "vãi"), w(4.6, 5.0, "luôn")],
            }],
        };
        let opts = CleanupOptions { remove_words: vec!["vãi".into()], ..Default::default() };
        let cuts = find_cuts(&t, &opts);
        assert!(cuts.iter().any(|c| c.kind == CutKind::Filler && c.start < 1.2 && c.end > 1.5));
        assert!(cuts.iter().any(|c| c.kind == CutKind::Pause && (c.start - 2.125).abs() < 0.01 && (c.end - 3.875).abs() < 0.01));
        assert!(cuts.iter().any(|c| c.kind == CutKind::Word && c.start < 4.3 && c.end > 4.6));
    }
}
