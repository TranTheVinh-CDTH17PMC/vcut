//! Lưu trữ project trên đĩa.
//!
//! - `project.json`: trạng thái hiện tại.
//! - `.vlogcut/history/`: các bản cũ để undo/redo.
//! - `proposal.json`: thay đổi Claude đề xuất, chờ người dùng duyệt.
//!
//! Ghi file kiểu "atomic": ghi ra file tạm rồi đổi tên đè lên file thật.
//! Nếu máy tắt đột ngột giữa chừng, project.json không bao giờ bị ghi dở.

use crate::error::{CoreError, Result};
use crate::ops::{self, Diff, Op};
use crate::project::{MediaItem, Project, Scene, SceneMark};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "project.json";
pub const PROPOSAL_FILE: &str = "proposal.json";
pub const BRIEF_FILE: &str = "brief.json";
pub const DATA_DIR: &str = ".vlogcut";
const MAX_HISTORY: usize = 100;

/// Các thư mục con trong `.vlogcut`.
pub fn data_dir(dir: &Path, sub: &str) -> PathBuf {
    dir.join(DATA_DIR).join(sub)
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

/// Ghi JSON an toàn: ghi file tạm cạnh file đích rồi đổi tên.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    // Trên Windows, rename sẽ ghi đè file cũ.
    fs::rename(&tmp, path)?;
    Ok(())
}


// ---------- Khoá ghi giữa nhiều chương trình ----------

/// App và MCP server (Claude) là hai chương trình riêng cùng ghi project.json.
/// Trước khi "đọc - sửa - ghi", ta tạo file khoá `.vlogcut/write.lock`.
/// `create_new` chỉ thành công nếu file chưa tồn tại, nên chỉ một bên giữ được khoá.
/// Khi biến `WriteLock` ra khỏi phạm vi, `Drop` tự xoá file khoá (kể cả khi lỗi).
pub struct WriteLock {
    path: PathBuf,
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn lock(dir: &Path) -> Result<WriteLock> {
    let path = dir.join(DATA_DIR).join("write.lock");
    fs::create_dir_all(path.parent().unwrap())?;
    let start = std::time::Instant::now();
    loop {
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(WriteLock { path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Khoá bỏ quên (chương trình kia bị tắt ngang) quá 30 giây thì gỡ.
                let stale = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .map(|t| t.elapsed().map(|d| d.as_secs() > 30).unwrap_or(false))
                    .unwrap_or(false);
                if stale {
                    let _ = fs::remove_file(&path);
                    continue;
                }
                if start.elapsed().as_secs() >= 10 {
                    return Err(CoreError::InvalidOp("project đang bận (bên kia đang ghi), thử lại sau".into()));
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Tạo project mới trong thư mục (nếu đã có thì báo lỗi).
pub fn create(dir: &Path, name: &str) -> Result<Project> {
    if dir.join(PROJECT_FILE).exists() {
        return Err(CoreError::InvalidOp("thư mục này đã có project".into()));
    }
    for sub in ["thumbs", "proxies", "contact", "history", "renders"] {
        fs::create_dir_all(data_dir(dir, sub))?;
    }
    let p = Project::new(name);
    write_json_atomic(&dir.join(PROJECT_FILE), &p)?;
    Ok(p)
}

/// Mở project; nếu thư mục chưa có thì tạo mới với tên thư mục.
pub fn open_or_create(dir: &Path) -> Result<Project> {
    if dir.join(PROJECT_FILE).exists() {
        for sub in ["thumbs", "proxies", "contact", "history", "renders"] {
            fs::create_dir_all(data_dir(dir, sub))?;
        }
        load(dir)
    } else {
        let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Project".into());
        create(dir, &name)
    }
}

pub fn load(dir: &Path) -> Result<Project> {
    read_json(&dir.join(PROJECT_FILE))
}

// ---------- Lịch sử undo/redo ----------

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryIndex {
    next: u64,
    undo: Vec<String>,
    redo: Vec<String>,
}

fn history_index_path(dir: &Path) -> PathBuf {
    data_dir(dir, "history").join("index.json")
}

fn load_history(dir: &Path) -> HistoryIndex {
    read_json(&history_index_path(dir)).unwrap_or_default()
}

fn save_snapshot(dir: &Path, h: &mut HistoryIndex, p: &Project) -> Result<String> {
    h.next += 1;
    let name = format!("snap-{:06}.json", h.next);
    write_json_atomic(&data_dir(dir, "history").join(&name), p)?;
    Ok(name)
}

fn drop_snapshot(dir: &Path, name: &str) {
    // Xoá file cũ; lỗi cũng bỏ qua vì không ảnh hưởng project.
    let _ = fs::remove_file(data_dir(dir, "history").join(name));
}

/// Kết quả một lần ghi thay đổi.
#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub project: Project,
    pub diff: Diff,
}

/// Áp dụng thao tác và lưu. Bản cũ được đẩy vào lịch sử undo.
///
/// `expected_revision`: nếu có, chỉ ghi khi project trên đĩa đúng bản đó.
/// Đây là cách chống ghi đè khi bạn và Claude cùng sửa.
pub fn commit(dir: &Path, ops_list: &[Op], expected_revision: Option<u64>) -> Result<Commit> {
    let _lock = lock(dir)?;
    let current = load(dir)?;
    if let Some(exp) = expected_revision {
        if exp != current.revision {
            return Err(CoreError::Conflict { expected: exp, actual: current.revision });
        }
    }
    let mut next = ops::apply(&current, ops_list)?;
    let diff = ops::diff(&current, &next);
    next.revision = current.revision + 1;

    let mut h = load_history(dir);
    let snap = save_snapshot(dir, &mut h, &current)?;
    h.undo.push(snap);
    for r in h.redo.drain(..) {
        drop_snapshot(dir, &r);
    }
    while h.undo.len() > MAX_HISTORY {
        let old = h.undo.remove(0);
        drop_snapshot(dir, &old);
    }
    write_json_atomic(&dir.join(PROJECT_FILE), &next)?;
    write_json_atomic(&history_index_path(dir), &h)?;
    Ok(Commit { project: next, diff })
}

/// Thử áp dụng mà không ghi gì (để xem trước diff).
pub fn preview(dir: &Path, ops_list: &[Op]) -> Result<Diff> {
    let current = load(dir)?;
    let next = ops::apply(&current, ops_list)?;
    Ok(ops::diff(&current, &next))
}

/// Hoàn tác. Trả về project sau khi hoàn tác, hoặc `None` nếu không còn gì.
pub fn undo(dir: &Path) -> Result<Option<Project>> {
    step_history(dir, true)
}

pub fn redo(dir: &Path) -> Result<Option<Project>> {
    step_history(dir, false)
}

fn step_history(dir: &Path, is_undo: bool) -> Result<Option<Project>> {
    let _lock = lock(dir)?;
    let mut h = load_history(dir);
    let name = if is_undo { h.undo.pop() } else { h.redo.pop() };
    let Some(name) = name else { return Ok(None) };
    let current = load(dir)?;
    let mut restored: Project = read_json(&data_dir(dir, "history").join(&name))?;
    drop_snapshot(dir, &name);
    let snap = save_snapshot(dir, &mut h, &current)?;
    if is_undo { h.redo.push(snap) } else { h.undo.push(snap) }
    // Thư viện media không nằm trong undo: luôn giữ danh sách media mới nhất.
    restored.media = current.media.clone();
    // Revision luôn tăng, để mọi bên đều biết project vừa đổi.
    restored.revision = current.revision + 1;
    write_json_atomic(&dir.join(PROJECT_FILE), &restored)?;
    write_json_atomic(&history_index_path(dir), &h)?;
    Ok(Some(restored))
}

/// Số bước có thể undo/redo (cho giao diện bật/tắt nút).
pub fn history_counts(dir: &Path) -> (usize, usize) {
    let h = load_history(dir);
    (h.undo.len(), h.redo.len())
}

/// Thêm hoặc cập nhật media trong thư viện. Không đi vào lịch sử undo.
pub fn upsert_media(dir: &Path, items: Vec<MediaItem>) -> Result<Project> {
    let _lock = lock(dir)?;
    let mut p = load(dir)?;
    for item in items {
        match p.media.iter_mut().find(|m| m.id == item.id || m.path == item.path) {
            Some(existing) => *existing = MediaItem { id: existing.id.clone(), ..item },
            None => p.media.push(item),
        }
    }
    p.revision += 1;
    write_json_atomic(&dir.join(PROJECT_FILE), &p)?;
    Ok(p)
}

/// Lưu kết quả phân tích của một file, rồi tính lại nhãn Mờ/Trùng/Đẹp cho cả project
/// (vì các nhãn đó phụ thuộc vào việc so sánh giữa các file).
pub fn set_scenes(dir: &Path, media_id: &str, scenes: Vec<Scene>) -> Result<Project> {
    let _lock = lock(dir)?;
    let mut p = load(dir)?;
    let m = p.media.iter_mut().find(|m| m.id == media_id).ok_or_else(|| CoreError::NotFound(format!("media {media_id}")))?;
    m.scenes = scenes;
    m.analysis_version = crate::analysis::ANALYSIS_VERSION;
    crate::analysis::finalize(&mut p.media);
    p.revision += 1;
    write_json_atomic(&dir.join(PROJECT_FILE), &p)?;
    Ok(p)
}

/// Đánh dấu file đã được nhận dạng giọng nói.
pub fn set_transcribed(dir: &Path, media_id: &str, value: bool) -> Result<Project> {
    let _lock = lock(dir)?;
    let mut p = load(dir)?;
    if let Some(m) = p.media.iter_mut().find(|m| m.id == media_id) {
        m.transcribed = value;
    }
    p.revision += 1;
    write_json_atomic(&dir.join(PROJECT_FILE), &p)?;
    Ok(p)
}

/// Người dùng đánh dấu giữ / bỏ một cảnh (`None` = theo tự động).
pub fn set_scene_mark(dir: &Path, scene_id: &str, mark: Option<SceneMark>) -> Result<Project> {
    let _lock = lock(dir)?;
    let mut p = load(dir)?;
    let s = p
        .media
        .iter_mut()
        .flat_map(|m| m.scenes.iter_mut())
        .find(|s| s.id == scene_id)
        .ok_or_else(|| CoreError::NotFound(format!("cảnh {scene_id}")))?;
    s.mark = mark;
    p.revision += 1;
    write_json_atomic(&dir.join(PROJECT_FILE), &p)?;
    Ok(p)
}

// ---------- Yêu cầu dựng (brief) ----------

/// Yêu cầu dựng từ màn hình Tự dựng. Claude đọc file này qua MCP (chặng 4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Brief {
    /// Độ dài mong muốn (giây). `None` = tuỳ ý.
    #[serde(default)]
    pub target_secs: Option<f64>,
    #[serde(default = "default_ratio")]
    pub width: u32,
    #[serde(default = "default_ratio_h")]
    pub height: u32,
    /// "chill" | "hype" | "emotional" | "fun"
    #[serde(default = "default_mood")]
    pub mood: String,
    /// Media id của bài nhạc nền; `None` = để Claude chọn trong thư viện.
    #[serde(default)]
    pub music_media_id: Option<String>,
    #[serde(default = "yes")]
    pub chronological: bool,
    #[serde(default = "yes")]
    pub place_titles: bool,
    #[serde(default = "yes")]
    pub keep_speech: bool,
    #[serde(default = "yes")]
    pub beat_sync: bool,
    #[serde(default)]
    pub notes: String,
}

fn default_ratio() -> u32 {
    1080
}
fn default_ratio_h() -> u32 {
    1920
}
fn default_mood() -> String {
    "chill".into()
}
fn yes() -> bool {
    true
}

impl Default for Brief {
    fn default() -> Self {
        serde_json::from_str("{}").expect("brief mặc định")
    }
}

pub fn load_brief(dir: &Path) -> Brief {
    read_json(&dir.join(BRIEF_FILE)).unwrap_or_default()
}

pub fn save_brief(dir: &Path, brief: &Brief) -> Result<()> {
    write_json_atomic(&dir.join(BRIEF_FILE), brief)
}

/// Xoá media khỏi thư viện (chỉ khi không clip nào dùng).
pub fn remove_media(dir: &Path, media_id: &str) -> Result<Project> {
    let _lock = lock(dir)?;
    let mut p = load(dir)?;
    if p.video.iter().any(|c| c.media_id == media_id) || p.audio.iter().any(|a| a.media_id == media_id) {
        return Err(CoreError::InvalidOp("media đang được dùng trên timeline".into()));
    }
    p.media.retain(|m| m.id != media_id);
    p.revision += 1;
    write_json_atomic(&dir.join(PROJECT_FILE), &p)?;
    Ok(p)
}

// ---------- Đề xuất của Claude ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub description: String,
    /// Giải thích thêm của Claude (vì sao sửa như vậy), hiện trong app.
    #[serde(default)]
    pub notes: Option<String>,
    pub base_revision: u64,
    pub ops: Vec<Op>,
    pub diff: Diff,
}

pub fn propose(dir: &Path, description: &str, ops_list: Vec<Op>) -> Result<Proposal> {
    propose_with_notes(dir, description, None, ops_list)
}

pub fn propose_with_notes(dir: &Path, description: &str, notes: Option<String>, ops_list: Vec<Op>) -> Result<Proposal> {
    let current = load(dir)?;
    let next = ops::apply(&current, &ops_list)?;
    let proposal = Proposal {
        id: crate::project::new_id("prop"),
        description: description.to_string(),
        notes,
        base_revision: current.revision,
        diff: ops::diff(&current, &next),
        ops: ops_list,
    };
    write_json_atomic(&dir.join(PROPOSAL_FILE), &proposal)?;
    Ok(proposal)
}

pub fn load_proposal(dir: &Path) -> Result<Option<Proposal>> {
    let path = dir.join(PROPOSAL_FILE);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_json(&path)?))
}

pub fn dismiss_proposal(dir: &Path) -> Result<()> {
    let path = dir.join(PROPOSAL_FILE);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Áp dụng đề xuất. Nếu project đã đổi kể từ lúc Claude đề xuất, vẫn thử áp dụng
/// lên bản mới; chỉ báo lỗi khi thao tác không còn hợp lệ.
pub fn accept_proposal(dir: &Path) -> Result<Commit> {
    let proposal = load_proposal(dir)?.ok_or_else(|| CoreError::NotFound("không có đề xuất nào".into()))?;
    let c = commit(dir, &proposal.ops, None)?;
    dismiss_proposal(dir)?;
    Ok(c)
}

// ---------- Lời nhắn của Claude hiện trong app ----------

pub const MESSAGE_FILE: &str = "claude-message.json";

/// Lời nhắn Claude gửi vào app (nhận xét review, giải thích, câu hỏi…),
/// để người dùng đọc ngay trong vlogcut thay vì chuyển sang Claude desktop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMessage {
    pub id: String,
    pub text: String,
    pub at: String,
}

pub fn set_message(dir: &Path, text: &str) -> Result<ClaudeMessage> {
    let m = ClaudeMessage {
        id: crate::project::new_id("msg"),
        text: text.to_string(),
        at: crate::media::iso_utc(std::time::SystemTime::now()),
    };
    write_json_atomic(&dir.join(MESSAGE_FILE), &m)?;
    Ok(m)
}

pub fn load_message(dir: &Path) -> Option<ClaudeMessage> {
    read_json(&dir.join(MESSAGE_FILE)).ok()
}

pub fn dismiss_message(dir: &Path) -> Result<()> {
    let p = dir.join(MESSAGE_FILE);
    if p.exists() {
        fs::remove_file(p)?;
    }
    Ok(())
}

// ---------- Project đang mở (để MCP server biết) ----------

#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentPointer {
    pub project_dir: String,
}

pub fn current_pointer_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vlogcut").join("current.json"))
}

pub fn set_current(dir: &Path) -> Result<()> {
    if let Some(path) = current_pointer_path() {
        write_json_atomic(&path, &CurrentPointer { project_dir: dir.to_string_lossy().to_string() })?;
    }
    Ok(())
}

pub fn get_current() -> Option<PathBuf> {
    let path = current_pointer_path()?;
    let c: CurrentPointer = read_json(&path).ok()?;
    Some(PathBuf::from(c.project_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{MediaItem, MediaKind};

    fn media() -> MediaItem {
        MediaItem {
            id: "m1".into(),
            path: "/x/a.mp4".into(),
            name: "a.mp4".into(),
            kind: MediaKind::Video,
            duration: 10.0,
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_audio: true,
            created_at: None,
            gps: None,
            thumb: None,
            proxy: None,
            scenes: vec![],
            analysis_version: 0,
            modified_at: None,
            transcribed: false,
        }
    }

    #[test]
    fn ghi_undo_redo_xung_dot() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        create(dir, "demo").unwrap();
        upsert_media(dir, vec![media()]).unwrap();
        let ops: Vec<Op> = serde_json::from_str(r#"[{"op":"add_clip","media_id":"m1","id":"c1"}]"#).unwrap();
        let rev = load(dir).unwrap().revision;
        let c = commit(dir, &ops, Some(rev)).unwrap();
        assert_eq!(c.project.video.len(), 1);
        // Gửi lại với revision cũ => xung đột
        assert!(matches!(commit(dir, &ops, Some(rev)), Err(CoreError::Conflict { .. })));
        let u = undo(dir).unwrap().unwrap();
        assert!(u.video.is_empty());
        assert_eq!(u.media.len(), 1);
        let r = redo(dir).unwrap().unwrap();
        assert_eq!(r.video.len(), 1);
        assert_eq!(history_counts(dir), (1, 0));
    }

    #[test]
    fn de_xuat() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        create(dir, "demo").unwrap();
        upsert_media(dir, vec![media()]).unwrap();
        let ops: Vec<Op> = serde_json::from_str(r#"[{"op":"add_clip","media_id":"m1"}]"#).unwrap();
        let p = propose(dir, "thêm clip", ops).unwrap();
        assert_eq!(p.diff.lines.len(), 1);
        accept_proposal(dir).unwrap();
        assert!(load_proposal(dir).unwrap().is_none());
        assert_eq!(load(dir).unwrap().video.len(), 1);
    }
}
