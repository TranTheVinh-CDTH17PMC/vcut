//! Lệnh cho gói giọng nói: tải Whisper + model, nhận dạng chạy nền, tạo đề xuất
//! dọn lời nói và phụ đề.

use crate::{blocking, current_dir, err, view, AppState, CmdResult, ProjectView};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use vlogcut_core::{ffmpeg, project::MediaKind, speech, store};

/// Bản build whisper.cpp dựng sẵn cho Windows (b5454). Bản CUDA 11.8 chạy được trên
/// RTX 20/30/40 và nhỏ hơn bản CUDA 12 (272 MB so với 653 MB).
const WHISPER_GPU_URL: &str = "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-win-cuda-11.8.0-x64.zip";
const WHISPER_CPU_URL: &str = "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip";
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin";

#[derive(Serialize)]
pub struct SpeechStatus {
    tools: speech::Tools,
    /// Máy có GPU NVIDIA (dựa vào việc ffmpeg dùng được NVENC).
    nvidia: bool,
    installing: bool,
    transcribing: bool,
}

#[tauri::command]
pub async fn speech_status(app: AppHandle) -> CmdResult<SpeechStatus> {
    let state = app.state::<AppState>();
    let installing = state.speech_installing.load(Ordering::SeqCst);
    let transcribing = state.transcribing.load(Ordering::SeqCst);
    blocking(move || {
        Ok(SpeechStatus { tools: speech::find_tools(), nvidia: ffmpeg::encoder() == ffmpeg::Encoder::Nvenc, installing, transcribing })
    })
    .await
}

// ---------------- Tải về ----------------

#[derive(Serialize, Clone)]
struct InstallProgress {
    /// "whisper" | "model" | "extract"
    phase: &'static str,
    done: u64,
    total: u64,
}

#[derive(Serialize, Clone)]
struct InstallDone {
    ok: bool,
    error: Option<String>,
}

/// Tải một file về `dest` (ghi ra `.part` rồi đổi tên khi xong), báo tiến độ.
fn download(app: &AppHandle, url: &str, dest: &Path, phase: &'static str) -> Result<(), String> {
    let resp = ureq::get(url).call().map_err(|e| format!("Không tải được {url}: {e}"))?;
    let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_body().into_reader();
    let part = dest.with_extension("part");
    let mut file = std::fs::File::create(&part).map_err(err)?;
    let mut buf = vec![0u8; 1 << 16];
    let (mut done, mut last_emit) = (0u64, 0u64);
    let state = app.state::<AppState>();
    loop {
        if state.speech_cancel.load(Ordering::SeqCst) {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err("Đã huỷ".into());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("Mất kết nối khi tải: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(err)?;
        done += n as u64;
        if done - last_emit > 2_000_000 {
            last_emit = done;
            let _ = app.emit("speech-install-progress", InstallProgress { phase, done, total });
        }
    }
    file.flush().map_err(err)?;
    drop(file);
    if total > 0 && done != total {
        let _ = std::fs::remove_file(&part);
        return Err(format!("Tải chưa đủ ({done}/{total} byte), hãy thử lại"));
    }
    std::fs::rename(&part, dest).map_err(err)?;
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(err)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("File zip hỏng: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(err)?;
        // `enclosed_name` chặn đường dẫn kiểu "../../" thoát ra ngoài thư mục đích.
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(err)?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(err)?;
            }
            let mut f = std::fs::File::create(&out).map_err(err)?;
            std::io::copy(&mut entry, &mut f).map_err(err)?;
        }
    }
    Ok(())
}

fn install(app: &AppHandle) -> Result<(), String> {
    let base: PathBuf = speech::install_dir().ok_or("Không xác định được thư mục AppData")?;
    std::fs::create_dir_all(base.join("models")).map_err(err)?;
    let tools = speech::find_tools();

    if tools.cli.is_none() {
        let gpu = ffmpeg::encoder() == ffmpeg::Encoder::Nvenc;
        let zip_path = base.join("whisper-download.zip");
        download(app, if gpu { WHISPER_GPU_URL } else { WHISPER_CPU_URL }, &zip_path, "whisper")?;
        let _ = app.emit("speech-install-progress", InstallProgress { phase: "extract", done: 0, total: 0 });
        let dir = base.join("whisper");
        let _ = std::fs::remove_dir_all(&dir);
        extract_zip(&zip_path, &dir)?;
        let _ = std::fs::remove_file(&zip_path);
        if speech::find_tools().cli.is_none() {
            return Err("Đã giải nén nhưng không thấy whisper-cli.exe".into());
        }
    }
    if tools.model.is_none() {
        download(app, MODEL_URL, &base.join("models").join(speech::DEFAULT_MODEL), "model")?;
    }
    Ok(())
}

/// Tải Whisper (bản GPU nếu có card NVIDIA) và model large-v3-turbo. Chạy nền.
#[tauri::command]
pub fn speech_install(app: AppHandle) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if state.speech_installing.swap(true, Ordering::SeqCst) {
        return Err("Đang tải rồi".into());
    }
    state.speech_cancel.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let r = install(&app);
        app.state::<AppState>().speech_installing.store(false, Ordering::SeqCst);
        let _ = app.emit("speech-install-done", InstallDone { ok: r.is_ok(), error: r.err() });
    });
    Ok(())
}

#[tauri::command]
pub fn speech_cancel(app: AppHandle) {
    app.state::<AppState>().speech_cancel.store(true, Ordering::SeqCst);
}

// ---------------- Nhận dạng ----------------

#[derive(Serialize, Clone)]
struct TranscribeProgress {
    done: usize,
    total: usize,
    name: String,
    /// Tiến độ file hiện tại 0..1.
    pct: f64,
}

#[derive(Serialize, Clone)]
struct TranscribeDone {
    ok: usize,
    errors: Vec<String>,
}

/// Nhận dạng giọng nói. `media_ids` rỗng = mọi file có tiếng đang dùng trên timeline
/// mà chưa nhận dạng. `force` = làm lại cả file đã nhận dạng.
#[tauri::command]
pub fn speech_transcribe(app: AppHandle, media_ids: Option<Vec<String>>, language: Option<String>, force: Option<bool>) -> CmdResult<()> {
    let dir = current_dir(&app)?;
    let tools = speech::find_tools();
    if !tools.ready() {
        return Err("Chưa cài Whisper. Bấm \"Tải Whisper\" trước.".into());
    }
    let state = app.state::<AppState>();
    if state.transcribing.swap(true, Ordering::SeqCst) {
        return Err("Đang nhận dạng rồi".into());
    }
    state.speech_cancel.store(false, Ordering::SeqCst);
    let force = force.unwrap_or(false);
    std::thread::spawn(move || {
        let p = store::load(&dir).ok();
        let todo: Vec<_> = p
            .map(|p| {
                let used: Vec<String> = p.video.iter().map(|c| c.media_id.clone()).chain(p.audio.iter().filter(|a| a.role == vlogcut_core::project::AudioRole::Voice).map(|a| a.media_id.clone())).collect();
                p.media
                    .into_iter()
                    .filter(|m| m.has_audio && m.kind != MediaKind::Image)
                    .filter(|m| match &media_ids {
                        Some(ids) if !ids.is_empty() => ids.contains(&m.id),
                        _ => used.contains(&m.id) && m.kind == MediaKind::Video,
                    })
                    .filter(|m| force || !m.transcribed)
                    .collect()
            })
            .unwrap_or_default();
        let opts = speech::TranscribeOptions { language: language.unwrap_or_else(|| "vi".into()), ..Default::default() };
        let total = todo.len();
        let (mut ok, mut errors) = (0, Vec::new());
        let cancel = app.state::<AppState>().speech_cancel.clone();
        for (i, m) in todo.iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let _ = app.emit("speech-progress", TranscribeProgress { done: i, total, name: m.name.clone(), pct: 0.0 });
            let mut last = 0.0;
            let r = speech::transcribe(&dir, m, &tools, &opts, &cancel, &mut |pct| {
                if pct - last >= 0.02 {
                    last = pct;
                    let _ = app.emit("speech-progress", TranscribeProgress { done: i, total, name: m.name.clone(), pct });
                }
            });
            match r {
                Ok(_) => {
                    ok += 1;
                    let _ = store::set_transcribed(&dir, &m.id, true);
                }
                Err(vlogcut_core::CoreError::Cancelled) => break,
                Err(e) => errors.push(format!("{}: {e}", m.name)),
            }
        }
        app.state::<AppState>().transcribing.store(false, Ordering::SeqCst);
        let _ = app.emit("speech-done", TranscribeDone { ok, errors });
    });
    Ok(())
}

// ---------------- Đề xuất ----------------

#[tauri::command]
pub async fn speech_cleanup(app: AppHandle, opts: speech::CleanupOptions) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    blocking(move || {
        let p = store::load(&dir).map_err(err)?;
        let plan = speech::cleanup_plan(&dir, &p, &opts).map_err(err)?;
        if plan.ops.is_empty() {
            return Err("Không tìm thấy tiếng đệm, khoảng lặng hay từ nào cần cắt.".into());
        }
        store::propose(&dir, &plan.description, plan.ops).map_err(err)?;
        view(&dir)
    })
    .await
}

#[tauri::command]
pub async fn speech_subtitles(app: AppHandle, opts: speech::SubtitleOptions) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    blocking(move || {
        let p = store::load(&dir).map_err(err)?;
        let (ops, desc) = speech::subtitle_plan(&dir, &p, &opts).map_err(err)?;
        store::propose(&dir, &desc, ops).map_err(err)?;
        view(&dir)
    })
    .await
}
