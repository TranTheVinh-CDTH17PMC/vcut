//! Cấu trúc dữ liệu của một project. Đây chính là nội dung `project.json`.
//!
//! `#[derive(Serialize, Deserialize)]` cho phép chuyển struct <-> JSON tự động.
//! `#[serde(default)]` nghĩa là nếu JSON thiếu trường đó thì dùng giá trị mặc
//! định, nhờ vậy project cũ vẫn mở được khi ta thêm trường mới.

use crate::keyframe::{Animatable, Vec2};
use crate::presets::FilterPreset;
use serde::{Deserialize, Serialize};

pub const PROJECT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub name: String,
    /// Tăng mỗi lần project thay đổi. Dùng để phát hiện xung đột giữa app và Claude.
    pub revision: u64,
    pub settings: Settings,
    #[serde(default)]
    pub media: Vec<MediaItem>,
    /// Track video: các clip nối tiếp nhau, không có khoảng trống.
    #[serde(default)]
    pub video: Vec<VideoClip>,
    #[serde(default)]
    pub texts: Vec<TextClip>,
    #[serde(default)]
    pub audio: Vec<AudioClip>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for Settings {
    fn default() -> Self {
        // Mặc định video dọc 9:16 cho TikTok / Reels / Shorts.
        Settings { width: 1080, height: 1920, fps: 30 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Video,
    Image,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: String,
    /// Đường dẫn tuyệt đối tới file gốc (có thể ở ổ ngoài).
    pub path: String,
    pub name: String,
    pub kind: MediaKind,
    /// Giây. Ảnh có duration = 0.
    pub duration: f64,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub has_audio: bool,
    /// Thời điểm quay (ISO 8601) nếu file có lưu.
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub gps: Option<Gps>,
    /// Đường dẫn tuyệt đối tới thumbnail trong `.vlogcut/thumbs`.
    #[serde(default)]
    pub thumb: Option<String>,
    /// Đường dẫn tuyệt đối tới proxy 540p trong `.vlogcut/proxies`.
    #[serde(default)]
    pub proxy: Option<String>,
    /// Các cảnh đã phân tích.
    #[serde(default)]
    pub scenes: Vec<Scene>,
    /// Phiên bản thuật toán phân tích đã chạy (0 = chưa phân tích).
    #[serde(default)]
    pub analysis_version: u32,
    /// Thời điểm sửa file (ISO 8601), dùng khi file không lưu ngày quay.
    #[serde(default)]
    pub modified_at: Option<String>,
    /// Đã nhận dạng giọng nói (có file `.vlogcut/transcripts/<id>.json`).
    #[serde(default)]
    pub transcribed: bool,
}

impl MediaItem {
    /// Ngày giờ dùng để sắp xếp theo chuyến đi: ngày quay, nếu không có thì ngày sửa file.
    pub fn taken_at(&self) -> Option<&str> {
        self.created_at.as_deref().or(self.modified_at.as_deref())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Gps {
    pub lat: f64,
    pub lon: f64,
}

/// Nhãn của một cảnh sau khi phân tích.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SceneTag {
    /// Không có lỗi nào và nét hơn mức trung bình.
    Good,
    Blurry,
    Dark,
    Overexposed,
    Shaky,
    /// Gần giống một cảnh khác đẹp hơn (xem `dup_of`).
    Duplicate,
    /// Có vẻ có người nói (ước lượng từ âm thanh, chưa dùng AI).
    Speech,
    /// Ảnh tĩnh.
    Photo,
}

impl SceneTag {
    pub fn label(self) -> &'static str {
        match self {
            SceneTag::Good => "Đẹp",
            SceneTag::Blurry => "Mờ",
            SceneTag::Dark => "Tối",
            SceneTag::Overexposed => "Cháy sáng",
            SceneTag::Shaky => "Rung",
            SceneTag::Duplicate => "Trùng",
            SceneTag::Speech => "Có người nói",
            SceneTag::Photo => "Ảnh",
        }
    }

    /// Nhãn lỗi: cảnh mang nhãn này mặc định bị loại.
    pub fn is_defect(self) -> bool {
        matches!(self, SceneTag::Blurry | SceneTag::Dark | SceneTag::Overexposed | SceneTag::Shaky | SceneTag::Duplicate)
    }
}

/// Người dùng tự đánh dấu, ghi đè kết quả tự động.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SceneMark {
    Keep,
    Drop,
}

/// Một cảnh (đoạn) trong file video, hoặc cả một ảnh.
///
/// Điểm số: `sharpness` (độ nét, càng cao càng nét), `brightness` 0..255,
/// `motion` (độ chuyển động trung bình), `shake` (độ rung: chuyển động giật,
/// không đều), `audio_db` (độ to tiếng, dBFS). Thang đo chỉ có ý nghĩa khi so
/// sánh các cảnh với nhau.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Scene {
    #[serde(default)]
    pub id: String,
    pub start: f64,
    pub end: f64,
    /// Các đoạn cắt ra từ cùng một cảnh quay dài có chung `shot`.
    #[serde(default)]
    pub shot: u32,
    #[serde(default)]
    pub sharpness: Option<f64>,
    /// Lượng chi tiết lớn trong hình (đo ở ảnh thu nhỏ). Thấp = cảnh trơn như trời, tường.
    #[serde(default)]
    pub detail: Option<f64>,
    #[serde(default)]
    pub brightness: Option<f64>,
    #[serde(default)]
    pub motion: Option<f64>,
    #[serde(default)]
    pub shake: Option<f64>,
    #[serde(default)]
    pub audio_db: Option<f64>,
    /// Mã băm hình ảnh (dHash 64 bit, dạng hex) để tìm cảnh trùng.
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default)]
    pub tags: Vec<SceneTag>,
    /// Điểm tổng 0..100 để xếp hạng.
    #[serde(default)]
    pub score: f64,
    /// Nếu là cảnh trùng: id cảnh được giữ lại.
    #[serde(default)]
    pub dup_of: Option<String>,
    #[serde(default)]
    pub mark: Option<SceneMark>,
}

impl Scene {
    pub fn duration(&self) -> f64 {
        self.end - self.start
    }

    /// Cảnh có được dùng không: theo đánh dấu của người dùng, nếu không thì theo nhãn lỗi.
    pub fn is_kept(&self) -> bool {
        match self.mark {
            Some(SceneMark::Keep) => true,
            Some(SceneMark::Drop) => false,
            None => !self.tags.iter().any(|t| t.is_defect()),
        }
    }
}

// Kiểu chuyển cảnh: xem bảng trong `transitions.rs`.
pub use crate::transitions::TransitionKind;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Transition {
    pub kind: TransitionKind,
    /// Giây. Phần chồng lên clip trước.
    pub duration: f64,
}

/// Màu của clip. brightness: -1..1 (0 = gốc), contrast: 0..2 (1 = gốc),
/// saturation: 0..3 (1 = gốc). Cùng ý nghĩa với filter `eq` của FFmpeg.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColorSettings {
    #[serde(default)]
    pub preset: FilterPreset,
    #[serde(default = "zero")]
    pub brightness: Animatable<f64>,
    #[serde(default = "one")]
    pub contrast: Animatable<f64>,
    #[serde(default = "one")]
    pub saturation: Animatable<f64>,
}

impl Default for ColorSettings {
    fn default() -> Self {
        ColorSettings { preset: FilterPreset::None, brightness: zero(), contrast: one(), saturation: one() }
    }
}

/// Biến đổi hình của clip.
/// position: độ lệch tâm khung, tính theo tỉ lệ khung hình (0,0 = giữa;
/// x = 0.1 là lệch phải 10% chiều rộng). scale: 1 = vừa khung.
/// rotation: độ. opacity: 0..1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transform {
    #[serde(default)]
    pub position: Animatable<Vec2>,
    #[serde(default = "one")]
    pub scale: Animatable<f64>,
    #[serde(default = "zero")]
    pub rotation: Animatable<f64>,
    #[serde(default = "one")]
    pub opacity: Animatable<f64>,
}

impl Default for Transform {
    fn default() -> Self {
        Transform { position: Animatable::Static(Vec2::default()), scale: one(), rotation: zero(), opacity: one() }
    }
}

fn zero() -> Animatable<f64> {
    Animatable::Static(0.0)
}
fn one() -> Animatable<f64> {
    Animatable::Static(1.0)
}
fn default_speed() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoClip {
    pub id: String,
    pub media_id: String,
    /// Đoạn lấy từ file gốc (giây). Với ảnh: src_in = 0, src_out = thời lượng hiển thị.
    pub src_in: f64,
    pub src_out: f64,
    #[serde(default = "default_speed")]
    pub speed: f64,
    #[serde(default)]
    pub color: ColorSettings,
    #[serde(default)]
    pub transform: Transform,
    /// Số khung trộn để tạo motion blur. 0 = tắt.
    #[serde(default)]
    pub motion_blur: u32,
    #[serde(default)]
    pub transition_in: Option<Transition>,
    #[serde(default = "one")]
    pub volume: Animatable<f64>,
}

impl VideoClip {
    /// Thời lượng trên timeline (đã tính tốc độ).
    pub fn duration(&self) -> f64 {
        (self.src_out - self.src_in) / self.speed
    }

    /// Mọi mốc keyframe của clip (để hiện chấm trên timeline).
    pub fn keyframe_times(&self) -> Vec<f64> {
        let mut v = Vec::new();
        v.extend(self.transform.position.times());
        v.extend(self.transform.scale.times());
        v.extend(self.transform.rotation.times());
        v.extend(self.transform.opacity.times());
        v.extend(self.color.brightness.times());
        v.extend(self.color.contrast.times());
        v.extend(self.color.saturation.times());
        v.extend(self.volume.times());
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        v
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Top,
    Center,
    #[default]
    Bottom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextAnim {
    None,
    #[default]
    Fade,
    Pop,
    SlideUp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextStyle {
    #[serde(default = "default_font")]
    pub font: String,
    /// Cỡ chữ theo pixel của khung xuất (vd. khung 1080x1920).
    #[serde(default = "default_size")]
    pub size: f64,
    /// Màu dạng "#RRGGBB".
    #[serde(default = "white")]
    pub color: String,
    #[serde(default = "black")]
    pub outline_color: String,
    #[serde(default = "default_outline")]
    pub outline: f64,
    #[serde(default = "yes")]
    pub bold: bool,
    #[serde(default)]
    pub align: TextAlign,
    #[serde(default)]
    pub anim_in: TextAnim,
    /// Chữ chạy theo giọng: từng từ đổi sang `highlight_color` đúng lúc được nói.
    /// Cần `TextClip::words`.
    #[serde(default)]
    pub karaoke: bool,
    #[serde(default = "yellow")]
    pub highlight_color: String,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            font: default_font(),
            size: default_size(),
            color: white(),
            outline_color: black(),
            outline: default_outline(),
            bold: true,
            align: TextAlign::Bottom,
            anim_in: TextAnim::Fade,
            karaoke: false,
            highlight_color: yellow(),
        }
    }
}

fn default_font() -> String {
    "Arial".into()
}
fn default_size() -> f64 {
    72.0
}
fn white() -> String {
    "#FFFFFF".into()
}
fn black() -> String {
    "#000000".into()
}
fn yellow() -> String {
    "#FFD60A".into()
}
fn default_outline() -> f64 {
    4.0
}
fn yes() -> bool {
    true
}

/// Biến đổi của chữ. position: tâm chữ theo tỉ lệ khung (0..1); `None` = theo `align`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TextTransform {
    #[serde(default)]
    pub position: Option<Animatable<Vec2>>,
    #[serde(default)]
    pub scale: Option<Animatable<f64>>,
    #[serde(default)]
    pub opacity: Option<Animatable<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextClip {
    pub id: String,
    pub text: String,
    /// Giây trên timeline.
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub style: TextStyle,
    #[serde(default)]
    pub transform: TextTransform,
    /// Thời điểm từng từ (giây, tính từ `start` của chữ). Có khi chữ được tạo từ lời nói.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<TextWord>,
}

/// Một từ trong phụ đề và lúc nó được nói (giây, tính từ đầu đoạn chữ).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

impl TextClip {
    /// Vị trí mặc định theo kiểu căn.
    pub fn base_position(&self) -> Vec2 {
        let y = match self.style.align {
            TextAlign::Top => 0.12,
            TextAlign::Center => 0.5,
            TextAlign::Bottom => 0.82,
        };
        Vec2 { x: 0.5, y }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioRole {
    #[default]
    Music,
    Voice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioClip {
    pub id: String,
    pub media_id: String,
    #[serde(default)]
    pub role: AudioRole,
    /// Giây trên timeline.
    pub start: f64,
    pub src_in: f64,
    pub src_out: f64,
    #[serde(default = "one")]
    pub volume: Animatable<f64>,
    #[serde(default)]
    pub fade_in: f64,
    #[serde(default)]
    pub fade_out: f64,
    /// Tự giảm âm lượng khi clip video có tiếng nói.
    #[serde(default)]
    pub duck: bool,
}

impl AudioClip {
    pub fn duration(&self) -> f64 {
        self.src_out - self.src_in
    }
}

/// Vị trí một clip video trên timeline sau khi tính chuyển cảnh.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct ClipSpan {
    pub start: f64,
    pub duration: f64,
    /// Thời lượng chuyển cảnh thực tế (đã giới hạn cho vừa).
    pub transition: f64,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Project {
            version: PROJECT_VERSION,
            name: name.into(),
            revision: 1,
            settings: Settings::default(),
            media: vec![],
            video: vec![],
            texts: vec![],
            audio: vec![],
        }
    }

    pub fn media_by_id(&self, id: &str) -> Option<&MediaItem> {
        self.media.iter().find(|m| m.id == id)
    }

    /// Thời lượng chuyển cảnh thật của clip `i`: không vượt quá nửa clip ngắn hơn.
    pub fn effective_transition(&self, i: usize) -> f64 {
        if i == 0 {
            return 0.0;
        }
        match self.video[i].transition_in {
            Some(t) if t.duration > 0.0 => {
                let max = self.video[i - 1].duration().min(self.video[i].duration()) / 2.0;
                t.duration.min(max).max(0.0)
            }
            _ => 0.0,
        }
    }

    /// Tính vị trí từng clip video. Chuyển cảnh làm clip sau bắt đầu sớm hơn.
    pub fn video_layout(&self) -> Vec<ClipSpan> {
        let mut out = Vec::with_capacity(self.video.len());
        let mut cursor = 0.0;
        for (i, clip) in self.video.iter().enumerate() {
            let tr = self.effective_transition(i);
            let start = (cursor - tr).max(0.0);
            let duration = clip.duration();
            out.push(ClipSpan { start, duration, transition: tr });
            cursor = start + duration;
        }
        out
    }

    /// Tổng thời lượng của video (theo track video).
    pub fn duration(&self) -> f64 {
        self.video_layout().last().map(|s| s.start + s.duration).unwrap_or(0.0)
    }
}

/// Sinh id ngắn, vd. "clip_3f9a1c2b".
pub fn new_id(prefix: &str) -> String {
    let u = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}_{}", &u[..8])
}
