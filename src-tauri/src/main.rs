//! App desktop vlogcut (Tauri 2).
//!
//! File này chỉ làm "cầu nối": nhận lệnh từ giao diện (React), gọi vào
//! `vlogcut-core`, rồi trả kết quả hoặc phát sự kiện tiến độ về giao diện.

// Bản release không mở thêm cửa sổ console đen trên Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod claude;
mod speech_cmd;
mod update;

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use vlogcut_core::project::{MediaKind, SceneMark};
use vlogcut_core::{analysis, autoedit, ffmpeg, media, ops::Op, presets::FilterPreset, render, store};

/// Trạng thái dùng chung của app.
///
/// `Mutex` (khoá) cho phép nhiều luồng dùng chung dữ liệu an toàn: muốn đọc/ghi
/// phải `lock()` trước, chỉ một luồng giữ khoá tại một thời điểm.
/// `Arc` là con trỏ đếm tham chiếu, chia sẻ quyền sở hữu giữa nhiều luồng.
#[derive(Default)]
pub(crate) struct AppState {
    dir: Mutex<Option<PathBuf>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// Chỉ một thao tác ghi project tại một thời điểm (trong app).
    write_lock: Mutex<()>,
    export_cancel: Arc<AtomicBool>,
    analysis_cancel: Arc<AtomicBool>,
    importing: AtomicBool,
    exporting: AtomicBool,
    analyzing: AtomicBool,
    speech_installing: AtomicBool,
    transcribing: AtomicBool,
    speech_cancel: Arc<AtomicBool>,
}

/// Dữ liệu gửi lên giao diện mỗi khi project thay đổi.
#[derive(Serialize)]
pub(crate) struct ProjectView {
    dir: String,
    project: vlogcut_core::project::Project,
    can_undo: bool,
    can_redo: bool,
    proposal: Option<store::Proposal>,
    claude_message: Option<store::ClaudeMessage>,
}

/// Lệnh Tauri trả lỗi dạng chuỗi để giao diện hiện thông báo.
pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) fn err<E: ToString>(e: E) -> String {
    e.to_string()
}

pub(crate) fn current_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    let state = app.state::<AppState>();
    let guard = state.dir.lock().map_err(err)?;
    guard.clone().ok_or_else(|| "Chưa mở project nào".to_string())
}

pub(crate) fn view(dir: &Path) -> CmdResult<ProjectView> {
    let project = store::load(dir).map_err(err)?;
    let (u, r) = store::history_counts(dir);
    Ok(ProjectView {
        dir: dir.to_string_lossy().to_string(),
        project,
        can_undo: u > 0,
        can_redo: r > 0,
        proposal: store::load_proposal(dir).ok().flatten(),
        claude_message: store::load_message(dir),
    })
}

/// Chạy việc nặng ở luồng riêng để giao diện không bị đơ.
pub(crate) async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    F: FnOnce() -> CmdResult<T> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

// ---------------- Lệnh: thông tin app ----------------

#[derive(Serialize)]
struct AppInfo {
    ffmpeg: ffmpeg::FfmpegInfo,
    presets: Vec<vlogcut_core::presets::PresetInfo>,
    /// Bảng chuyển cảnh (id, tên, nhóm) cho giao diện.
    transitions: &'static [vlogcut_core::transitions::TransitionInfo],
    transition_groups: &'static [&'static str],
    last_project: Option<String>,
}

#[tauri::command]
async fn app_info() -> CmdResult<AppInfo> {
    blocking(|| {
        Ok(AppInfo {
            ffmpeg: ffmpeg::info(),
            presets: FilterPreset::all_info(),
            transitions: vlogcut_core::transitions::ALL,
            transition_groups: vlogcut_core::transitions::GROUPS,
            last_project: store::get_current().filter(|p| p.join(store::PROJECT_FILE).exists()).map(|p| p.to_string_lossy().to_string()),
        })
    })
    .await
}

// ---------------- Lệnh: project ----------------

#[tauri::command]
async fn project_open(app: AppHandle, dir: String) -> CmdResult<ProjectView> {
    let path = PathBuf::from(&dir);
    let p2 = path.clone();
    blocking(move || store::open_or_create(&p2).map(|_| ()).map_err(err)).await?;
    store::set_current(&path).map_err(err)?;
    {
        let state = app.state::<AppState>();
        *state.dir.lock().map_err(err)? = Some(path.clone());
    }
    start_watcher(&app, &path)?;
    view(&path)
}

#[tauri::command]
async fn project_get(app: AppHandle) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    blocking(move || view(&dir)).await
}

#[tauri::command]
async fn project_apply(app: AppHandle, ops: Vec<Op>, expected_revision: Option<u64>) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    let app2 = app.clone();
    blocking(move || {
        let state = app2.state::<AppState>();
        let _lock = state.write_lock.lock().map_err(err)?;
        store::commit(&dir, &ops, expected_revision).map_err(err)?;
        view(&dir)
    })
    .await
}

#[tauri::command]
async fn project_undo(app: AppHandle) -> CmdResult<ProjectView> {
    history_step(app, true).await
}

#[tauri::command]
async fn project_redo(app: AppHandle) -> CmdResult<ProjectView> {
    history_step(app, false).await
}

async fn history_step(app: AppHandle, undo: bool) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    let app2 = app.clone();
    blocking(move || {
        let state = app2.state::<AppState>();
        let _lock = state.write_lock.lock().map_err(err)?;
        if undo { store::undo(&dir) } else { store::redo(&dir) }.map_err(err)?;
        view(&dir)
    })
    .await
}

#[tauri::command]
async fn proposal_accept(app: AppHandle) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    let app2 = app.clone();
    blocking(move || {
        let state = app2.state::<AppState>();
        let _lock = state.write_lock.lock().map_err(err)?;
        store::accept_proposal(&dir).map_err(err)?;
        view(&dir)
    })
    .await
}

#[tauri::command]
async fn proposal_dismiss(app: AppHandle) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    blocking(move || {
        store::dismiss_proposal(&dir).map_err(err)?;
        view(&dir)
    })
    .await
}

// ---------------- Lệnh: nhập media ----------------

#[derive(Serialize, Clone)]
struct ImportProgress {
    /// "scan" = đọc thông tin + thumbnail, "proxy" = tạo bản nhẹ
    phase: &'static str,
    done: usize,
    total: usize,
    name: String,
}

#[derive(Serialize, Clone)]
struct ImportError {
    name: String,
    error: String,
}

/// Nhập file/thư mục. Chạy nền, báo tiến độ qua sự kiện `import-progress`.
#[tauri::command]
fn media_import(app: AppHandle, paths: Vec<String>) -> CmdResult<()> {
    let dir = current_dir(&app)?;
    let state = app.state::<AppState>();
    if state.importing.swap(true, Ordering::SeqCst) {
        return Err("Đang nhập file khác, hãy đợi xong".into());
    }
    // `move`: closure lấy quyền sở hữu `app`, `dir`, `paths` để mang sang luồng mới.
    std::thread::spawn(move || {
        import_job(&app, &dir, paths);
        app.state::<AppState>().importing.store(false, Ordering::SeqCst);
        let _ = app.emit("import-done", ());
        // Nhập xong thì phân tích luôn các file mới.
        let _ = start_analysis(&app, false);
    });
    Ok(())
}

fn import_job(app: &AppHandle, dir: &Path, paths: Vec<String>) {
    let state = app.state::<AppState>();
    let existing: Vec<String> = store::load(dir).map(|p| p.media.iter().map(|m| m.path.clone()).collect()).unwrap_or_default();
    let files: Vec<PathBuf> = media::collect_files(&paths.iter().map(PathBuf::from).collect::<Vec<_>>())
        .into_iter()
        .filter(|f| !existing.contains(&f.to_string_lossy().to_string()))
        .collect();
    let total = files.len();
    let mut added = Vec::new();

    // Bước 1: đọc thông tin + thumbnail (nhanh), hiện lên thư viện ngay.
    for (i, f) in files.iter().enumerate() {
        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let _ = app.emit("import-progress", ImportProgress { phase: "scan", done: i, total, name: name.clone() });
        let result = media::probe(f).and_then(|mut m| {
            m.thumb = media::make_thumbnail(dir, &m)?;
            Ok(m)
        });
        match result {
            Ok(m) => {
                let _lock = state.write_lock.lock();
                if store::upsert_media(dir, vec![m.clone()]).is_ok() {
                    added.push(m);
                }
            }
            Err(e) => {
                let _ = app.emit("import-error", ImportError { name, error: e.to_string() });
            }
        }
    }

    // Bước 2: tạo proxy cho video (chậm hơn, chạy lần lượt để không quá tải GPU).
    let videos: Vec<_> = added.into_iter().filter(|m| m.kind == vlogcut_core::project::MediaKind::Video).collect();
    let total = videos.len();
    for (i, m) in videos.iter().enumerate() {
        let _ = app.emit("import-progress", ImportProgress { phase: "proxy", done: i, total, name: m.name.clone() });
        match media::make_proxy(dir, m) {
            Ok(proxy) => {
                let _lock = state.write_lock.lock();
                // Đọc lại bản mới nhất rồi chỉ cập nhật trường proxy.
                if let Ok(p) = store::load(dir) {
                    if let Some(cur) = p.media.iter().find(|x| x.id == m.id) {
                        let mut cur = cur.clone();
                        cur.proxy = proxy;
                        let _ = store::upsert_media(dir, vec![cur]);
                    }
                }
            }
            Err(e) => {
                let _ = app.emit("import-error", ImportError { name: m.name.clone(), error: e.to_string() });
            }
        }
    }
}

#[tauri::command]
async fn media_remove(app: AppHandle, media_id: String) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    blocking(move || {
        store::remove_media(&dir, &media_id).map_err(err)?;
        view(&dir)
    })
    .await
}

// ---------------- Lệnh: xuất video ----------------

#[derive(Serialize, Clone)]
struct ExportDone {
    ok: bool,
    path: String,
    error: Option<String>,
    cancelled: bool,
}

#[tauri::command]
fn export_start(app: AppHandle, out_path: String) -> CmdResult<()> {
    let dir = current_dir(&app)?;
    let state = app.state::<AppState>();
    if state.exporting.swap(true, Ordering::SeqCst) {
        return Err("Đang xuất một video khác".into());
    }
    state.export_cancel.store(false, Ordering::SeqCst);
    let cancel = state.export_cancel.clone();
    std::thread::spawn(move || {
        let result = store::load(&dir).and_then(|p| {
            let opts = render::RenderOptions { out_path: PathBuf::from(&out_path), fast: false, encoder: None };
            let mut last = -1.0;
            render::render(&dir, &p, &opts, &cancel, &mut |pct| {
                // Chỉ gửi khi tăng >= 0.5% để đỡ spam sự kiện.
                if pct - last >= 0.005 || pct >= 1.0 {
                    last = pct;
                    let _ = app.emit("export-progress", pct);
                }
            })
        });
        let done = match result {
            Ok(()) => ExportDone { ok: true, path: out_path, error: None, cancelled: false },
            Err(vlogcut_core::CoreError::Cancelled) => ExportDone { ok: false, path: out_path, error: None, cancelled: true },
            Err(e) => ExportDone { ok: false, path: out_path, error: Some(e.to_string()), cancelled: false },
        };
        app.state::<AppState>().exporting.store(false, Ordering::SeqCst);
        let _ = app.emit("export-done", done);
    });
    Ok(())
}

#[tauri::command]
fn export_cancel(app: AppHandle) {
    app.state::<AppState>().export_cancel.store(true, Ordering::SeqCst);
}

/// Render thử một clip; trả về đường dẫn file mp4 để giao diện phát.
#[tauri::command]
async fn preview_clip(app: AppHandle, clip_id: String) -> CmdResult<String> {
    let dir = current_dir(&app)?;
    blocking(move || {
        let p = store::load(&dir).map_err(err)?;
        let cancel = AtomicBool::new(false);
        let out = render::render_clip_preview(&dir, &p, &clip_id, &cancel, &mut |pct| {
            let _ = app.emit("preview-progress", pct);
        })
        .map_err(err)?;
        Ok(out.to_string_lossy().to_string())
    })
    .await
}


// ---------------- Lệnh: phân tích cảnh ----------------

#[derive(Serialize, Clone)]
struct AnalysisProgress {
    done: usize,
    total: usize,
    name: String,
}

/// Bắt đầu phân tích (chạy nền). `force` = phân tích lại cả file đã phân tích.
#[tauri::command]
fn analysis_start(app: AppHandle, force: bool) -> CmdResult<()> {
    start_analysis(&app, force)
}

fn start_analysis(app: &AppHandle, force: bool) -> CmdResult<()> {
    let dir = current_dir(app)?;
    let state = app.state::<AppState>();
    if state.importing.load(Ordering::SeqCst) {
        return Err("Đang nhập file, phân tích sẽ tự chạy khi nhập xong".into());
    }
    if state.analyzing.swap(true, Ordering::SeqCst) {
        return Err("Đang phân tích rồi".into());
    }
    state.analysis_cancel.store(false, Ordering::SeqCst);
    let cancel = state.analysis_cancel.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        let todo: Vec<_> = store::load(&dir)
            .map(|p| {
                p.media
                    .into_iter()
                    .filter(|m| m.kind != MediaKind::Audio && (force || m.analysis_version < analysis::ANALYSIS_VERSION))
                    .collect()
            })
            .unwrap_or_default();
        let total = todo.len();
        let _ = app.emit("analysis-progress", AnalysisProgress { done: 0, total, name: String::new() });
        analysis::analyze_all(&dir, &todo, analysis::default_jobs(), &cancel, &|prog, m, result| {
            match result {
                Ok(scenes) => {
                    let state = app.state::<AppState>();
                    let _lock = state.write_lock.lock();
                    let _ = store::set_scenes(&dir, &m.id, scenes);
                }
                Err(vlogcut_core::CoreError::Cancelled) => {}
                Err(e) => {
                    let _ = app.emit("import-error", ImportError { name: m.name.clone(), error: format!("Phân tích lỗi: {e}") });
                }
            }
            let _ = app.emit("analysis-progress", AnalysisProgress { done: prog.done, total: prog.total, name: prog.name });
        });
        app.state::<AppState>().analyzing.store(false, Ordering::SeqCst);
        let _ = app.emit("analysis-done", ());
    });
    Ok(())
}

#[tauri::command]
fn analysis_cancel(app: AppHandle) {
    app.state::<AppState>().analysis_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn scene_mark(app: AppHandle, scene_id: String, mark: Option<SceneMark>) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    let app2 = app.clone();
    blocking(move || {
        let state = app2.state::<AppState>();
        let _lock = state.write_lock.lock().map_err(err)?;
        store::set_scene_mark(&dir, &scene_id, mark).map_err(err)?;
        view(&dir)
    })
    .await
}

// ---------------- Lệnh: yêu cầu dựng ----------------

#[tauri::command]
async fn brief_get(app: AppHandle) -> CmdResult<store::Brief> {
    let dir = current_dir(&app)?;
    Ok(store::load_brief(&dir))
}

#[tauri::command]
async fn brief_save(app: AppHandle, brief: store::Brief) -> CmdResult<()> {
    let dir = current_dir(&app)?;
    store::save_brief(&dir, &brief).map_err(err)
}

/// Dựng nhanh không cần Claude: lưu yêu cầu, lập kế hoạch, ghi thành đề xuất để duyệt.
#[tauri::command]
async fn autoedit_quick(app: AppHandle, brief: store::Brief) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    blocking(move || {
        store::save_brief(&dir, &brief).map_err(err)?;
        let p = store::load(&dir).map_err(err)?;
        let plan = autoedit::quick_plan(&p, &brief)?;
        store::propose(&dir, &plan.description, plan.ops).map_err(err)?;
        view(&dir)
    })
    .await
}

// ---------------- Lệnh: kết nối Claude desktop ----------------

#[tauri::command]
async fn claude_status() -> CmdResult<claude::ClaudeStatus> {
    blocking(|| Ok(claude::status())).await
}

#[tauri::command]
async fn claude_connect() -> CmdResult<claude::ClaudeStatus> {
    blocking(claude::connect).await
}

/// Bật Claude desktop lên trước (mở nếu chưa chạy).
#[tauri::command]
async fn claude_open() -> CmdResult<bool> {
    blocking(|| Ok(claude::open_app())).await
}

/// Tạo và mở tiện ích vlogcut.mcpb cho Claude desktop.
#[tauri::command]
async fn claude_install_extension() -> CmdResult<claude::ExtensionResult> {
    blocking(claude::install_extension).await
}

/// Chọn sẵn file trong Explorer.
#[tauri::command]
fn claude_reveal(path: String) {
    claude::reveal(std::path::Path::new(&path));
}

/// Có bản cài mới hơn bản đang chạy không (sau khi deploy.ps1 cập nhật).
#[tauri::command]
fn update_check() -> bool {
    update::available()
}

/// Mở bản mới và thoát bản đang chạy.
#[tauri::command]
fn update_restart(app: AppHandle) -> CmdResult<()> {
    update::restart(&app)
}

/// Mở chat mới trong Claude desktop với lời nhắn điền sẵn (người dùng chỉ bấm Enter).
#[tauri::command]
async fn claude_open_chat(prompt: String) -> CmdResult<bool> {
    blocking(move || Ok(claude::open_chat(&prompt))).await
}

#[tauri::command]
async fn claude_message_dismiss(app: AppHandle) -> CmdResult<ProjectView> {
    let dir = current_dir(&app)?;
    blocking(move || {
        store::dismiss_message(&dir).map_err(err)?;
        view(&dir)
    })
    .await
}

// ---------------- Theo dõi file ----------------

/// Theo dõi thư mục project: khi Claude (qua MCP) sửa project.json hoặc
/// proposal.json, báo giao diện tải lại.
fn start_watcher(app: &AppHandle, dir: &Path) -> CmdResult<()> {
    let app2 = app.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        for path in &event.paths {
            match path.file_name().and_then(|n| n.to_str()) {
                Some(store::PROJECT_FILE) => {
                    let _ = app2.emit("project-changed", ());
                }
                Some(store::PROPOSAL_FILE) | Some(store::MESSAGE_FILE) => {
                    let _ = app2.emit("project-changed", ());
                }
                _ => {}
            }
        }
    })
    .map_err(err)?;
    watcher.watch(dir, RecursiveMode::NonRecursive).map_err(err)?;
    let state = app.state::<AppState>();
    *state.watcher.lock().map_err(err)? = Some(watcher);
    Ok(())
}

fn main() {
    update::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_info,
            project_open,
            project_get,
            project_apply,
            project_undo,
            project_redo,
            proposal_accept,
            proposal_dismiss,
            media_import,
            media_remove,
            export_start,
            export_cancel,
            preview_clip,
            analysis_start,
            analysis_cancel,
            scene_mark,
            brief_get,
            brief_save,
            autoedit_quick,
            claude_status,
            claude_connect,
            claude_open,
            claude_open_chat,
            claude_install_extension,
            claude_reveal,
            update_check,
            update_restart,
            claude_message_dismiss,
            speech_cmd::speech_status,
            speech_cmd::speech_install,
            speech_cmd::speech_cancel,
            speech_cmd::speech_transcribe,
            speech_cmd::speech_cleanup,
            speech_cmd::speech_subtitles,
        ])
        .run(tauri::generate_context!())
        .expect("không khởi động được vlogcut");
}
