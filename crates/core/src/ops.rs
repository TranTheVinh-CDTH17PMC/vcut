//! Các thao tác sửa timeline.
//!
//! Mọi thay đổi (từ giao diện hay từ Claude) đều là một danh sách `Op`.
//! `apply` nhận project cũ, trả project mới; không sửa trực tiếp bản cũ.
//! Nhờ vậy ta luôn có cả "trước" và "sau" để tạo diff và lưu lịch sử undo.

use crate::error::{invalid, CoreError, Result};
use crate::keyframe::{normalize, Animatable, Keyframe, Vec2};
use crate::presets::FilterPreset;
use crate::project::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `#[serde(tag = "op")]`: JSON có dạng `{"op": "add_clip", "media_id": ...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    AddClip {
        media_id: String,
        #[serde(default)]
        src_in: Option<f64>,
        #[serde(default)]
        src_out: Option<f64>,
        /// Vị trí chèn; bỏ trống = thêm vào cuối.
        #[serde(default)]
        index: Option<usize>,
        #[serde(default)]
        id: Option<String>,
    },
    RemoveClip {
        clip_id: String,
    },
    MoveClip {
        clip_id: String,
        to_index: usize,
    },
    TrimClip {
        clip_id: String,
        #[serde(default)]
        src_in: Option<f64>,
        #[serde(default)]
        src_out: Option<f64>,
    },
    /// Tách clip tại `at` giây tính từ đầu clip trên timeline.
    SplitClip {
        clip_id: String,
        at: f64,
        #[serde(default)]
        new_id: Option<String>,
    },
    SetClipProps {
        clip_id: String,
        #[serde(flatten)]
        props: ClipProps,
    },
    AddText {
        text: String,
        start: f64,
        end: f64,
        #[serde(default)]
        style: Option<TextStyle>,
        #[serde(default)]
        id: Option<String>,
        /// Thời điểm từng từ (giây từ đầu đoạn chữ), dùng cho chữ chạy theo giọng.
        #[serde(default)]
        words: Option<Vec<TextWord>>,
    },
    UpdateText {
        text_id: String,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        start: Option<f64>,
        #[serde(default)]
        end: Option<f64>,
        #[serde(default)]
        style: Option<TextStyle>,
    },
    RemoveText {
        text_id: String,
    },
    AddAudio {
        media_id: String,
        start: f64,
        #[serde(default)]
        src_in: Option<f64>,
        #[serde(default)]
        src_out: Option<f64>,
        #[serde(default)]
        role: Option<AudioRole>,
        #[serde(default)]
        volume: Option<f64>,
        #[serde(default)]
        fade_in: Option<f64>,
        #[serde(default)]
        fade_out: Option<f64>,
        #[serde(default)]
        duck: Option<bool>,
        #[serde(default)]
        id: Option<String>,
    },
    SetAudioProps {
        audio_id: String,
        #[serde(default)]
        start: Option<f64>,
        #[serde(default)]
        src_in: Option<f64>,
        #[serde(default)]
        src_out: Option<f64>,
        #[serde(default)]
        role: Option<AudioRole>,
        #[serde(default)]
        volume: Option<f64>,
        #[serde(default)]
        fade_in: Option<f64>,
        #[serde(default)]
        fade_out: Option<f64>,
        #[serde(default)]
        duck: Option<bool>,
    },
    RemoveAudio {
        audio_id: String,
    },
    /// Thay toàn bộ keyframe của một thuộc tính. `t` tính từ đầu clip.
    SetKeyframes {
        target: Target,
        property: String,
        keyframes: Vec<Keyframe<Value>>,
    },
    /// Bỏ keyframe, giữ giá trị tại đầu clip làm giá trị cố định.
    RemoveKeyframes {
        target: Target,
        property: String,
    },
    ClearTimeline,
    SetSettings {
        #[serde(default)]
        width: Option<u32>,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        fps: Option<u32>,
    },
}

/// Đối tượng của thao tác keyframe.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Target {
    Clip(String),
    Text(String),
    Audio(String),
}

impl Target {
    fn id(&self) -> &str {
        match self {
            Target::Clip(i) | Target::Text(i) | Target::Audio(i) => i,
        }
    }
}

/// Các thuộc tính đặt giá trị cố định cho clip video. Trường nào `None` thì giữ nguyên.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClipProps {
    #[serde(default)]
    pub speed: Option<f64>,
    #[serde(default)]
    pub preset: Option<FilterPreset>,
    #[serde(default)]
    pub brightness: Option<f64>,
    #[serde(default)]
    pub contrast: Option<f64>,
    #[serde(default)]
    pub saturation: Option<f64>,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub rotation: Option<f64>,
    #[serde(default)]
    pub opacity: Option<f64>,
    #[serde(default)]
    pub position: Option<Vec2>,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub motion_blur: Option<u32>,
    /// Đặt chuyển cảnh vào clip. Dùng `remove_transition: true` để bỏ.
    #[serde(default)]
    pub transition_in: Option<Transition>,
    #[serde(default)]
    pub remove_transition: Option<bool>,
}

/// Thời lượng mặc định cho ảnh khi thêm vào timeline.
pub const IMAGE_DEFAULT_SECS: f64 = 3.0;
const MIN_CLIP: f64 = 0.1;

/// Áp dụng danh sách thao tác. Lỗi ở bất kỳ thao tác nào => không thay đổi gì.
///
/// `&Project` là "mượn" (borrow) project để đọc; ta `clone()` thành bản mới
/// mà ta sở hữu (own) và được phép sửa. Bản gốc của người gọi không bị đụng.
pub fn apply(project: &Project, ops: &[Op]) -> Result<Project> {
    let mut p = project.clone();
    for (i, op) in ops.iter().enumerate() {
        apply_one(&mut p, op).map_err(|e| match e {
            CoreError::InvalidOp(m) => CoreError::InvalidOp(format!("thao tác #{} ({}): {m}", i + 1, op_name(op))),
            other => other,
        })?;
    }
    Ok(p)
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::AddClip { .. } => "add_clip",
        Op::RemoveClip { .. } => "remove_clip",
        Op::MoveClip { .. } => "move_clip",
        Op::TrimClip { .. } => "trim_clip",
        Op::SplitClip { .. } => "split_clip",
        Op::SetClipProps { .. } => "set_clip_props",
        Op::AddText { .. } => "add_text",
        Op::UpdateText { .. } => "update_text",
        Op::RemoveText { .. } => "remove_text",
        Op::AddAudio { .. } => "add_audio",
        Op::SetAudioProps { .. } => "set_audio_props",
        Op::RemoveAudio { .. } => "remove_audio",
        Op::SetKeyframes { .. } => "set_keyframes",
        Op::RemoveKeyframes { .. } => "remove_keyframes",
        Op::ClearTimeline => "clear_timeline",
        Op::SetSettings { .. } => "set_settings",
    }
}

fn clip_index(p: &Project, id: &str) -> Result<usize> {
    p.video.iter().position(|c| c.id == id).ok_or_else(|| invalid(format!("không có clip '{id}'")))
}

fn finite(v: f64, what: &str) -> Result<f64> {
    if v.is_finite() { Ok(v) } else { Err(invalid(format!("{what} không hợp lệ"))) }
}

/// Kiểm tra đoạn nguồn [src_in, src_out] nằm trong file.
fn check_range(media: &MediaItem, src_in: f64, src_out: f64) -> Result<()> {
    finite(src_in, "src_in")?;
    finite(src_out, "src_out")?;
    if src_in < 0.0 || src_out - src_in < MIN_CLIP {
        return Err(invalid(format!("đoạn {src_in:.2}–{src_out:.2}s quá ngắn hoặc âm")));
    }
    if media.kind != MediaKind::Image && src_out > media.duration + 0.05 {
        return Err(invalid(format!(
            "src_out {src_out:.2}s vượt quá độ dài file '{}' ({:.2}s)",
            media.name, media.duration
        )));
    }
    Ok(())
}

fn apply_one(p: &mut Project, op: &Op) -> Result<()> {
    match op {
        Op::AddClip { media_id, src_in, src_out, index, id } => {
            let media = p.media_by_id(media_id).ok_or_else(|| invalid(format!("không có media '{media_id}'")))?;
            if media.kind == MediaKind::Audio {
                return Err(invalid("file âm thanh phải thêm bằng add_audio"));
            }
            let default_out = if media.kind == MediaKind::Image { IMAGE_DEFAULT_SECS } else { media.duration };
            let (a, b) = (src_in.unwrap_or(0.0), src_out.unwrap_or(default_out));
            check_range(media, a, b)?;
            let clip = VideoClip {
                id: id.clone().unwrap_or_else(|| new_id("clip")),
                media_id: media_id.clone(),
                src_in: a,
                src_out: b,
                speed: 1.0,
                color: ColorSettings::default(),
                transform: Transform::default(),
                motion_blur: 0,
                transition_in: None,
                volume: Animatable::Static(1.0),
            };
            if p.video.iter().any(|c| c.id == clip.id) {
                return Err(invalid(format!("id '{}' đã tồn tại", clip.id)));
            }
            let at = index.unwrap_or(p.video.len()).min(p.video.len());
            p.video.insert(at, clip);
        }
        Op::RemoveClip { clip_id } => {
            let i = clip_index(p, clip_id)?;
            p.video.remove(i);
            if let Some(first) = p.video.first_mut() {
                first.transition_in = None; // clip đầu không có chuyển cảnh vào
            }
        }
        Op::MoveClip { clip_id, to_index } => {
            let i = clip_index(p, clip_id)?;
            let clip = p.video.remove(i);
            let to = (*to_index).min(p.video.len());
            p.video.insert(to, clip);
            if let Some(first) = p.video.first_mut() {
                first.transition_in = None;
            }
        }
        Op::TrimClip { clip_id, src_in, src_out } => {
            let i = clip_index(p, clip_id)?;
            let media = p.media_by_id(&p.video[i].media_id).cloned().ok_or_else(|| invalid("clip trỏ tới media không tồn tại"))?;
            let clip = &mut p.video[i];
            let a = src_in.unwrap_or(clip.src_in);
            let b = src_out.unwrap_or(clip.src_out);
            check_range(&media, a, b)?;
            // Cắt đầu clip thì nội dung dời về trước => keyframe dời theo.
            let shift = -(a - clip.src_in) / clip.speed;
            if shift != 0.0 {
                shift_clip_keyframes(clip, shift);
            }
            clip.src_in = a;
            clip.src_out = b;
        }
        Op::SplitClip { clip_id, at, new_id: nid } => {
            let i = clip_index(p, clip_id)?;
            let clip = p.video[i].clone();
            let at = finite(*at, "at")?;
            if at < MIN_CLIP || at > clip.duration() - MIN_CLIP {
                return Err(invalid(format!("điểm tách {at:.2}s phải nằm trong clip (dài {:.2}s)", clip.duration())));
            }
            let src_mid = clip.src_in + at * clip.speed;
            let mut left = clip.clone();
            let mut right = clip.clone();
            left.src_out = src_mid;
            right.src_in = src_mid;
            right.id = nid.clone().unwrap_or_else(|| new_id("clip"));
            right.transition_in = None;
            split_clip_keyframes(&clip, at, &mut left, &mut right);
            p.video[i] = left;
            p.video.insert(i + 1, right);
        }
        Op::SetClipProps { clip_id, props } => {
            let i = clip_index(p, clip_id)?;
            let first = i == 0;
            let c = &mut p.video[i];
            if let Some(s) = props.speed {
                if !(0.25..=4.0).contains(&s) {
                    return Err(invalid("tốc độ phải từ 0.25 đến 4"));
                }
                c.speed = s;
            }
            if let Some(v) = props.preset {
                c.color.preset = v;
            }
            set_static(&mut c.color.brightness, props.brightness, -1.0, 1.0, "brightness")?;
            set_static(&mut c.color.contrast, props.contrast, 0.0, 2.0, "contrast")?;
            set_static(&mut c.color.saturation, props.saturation, 0.0, 3.0, "saturation")?;
            set_static(&mut c.transform.scale, props.scale, 0.1, 5.0, "scale")?;
            set_static(&mut c.transform.rotation, props.rotation, -360.0, 360.0, "rotation")?;
            set_static(&mut c.transform.opacity, props.opacity, 0.0, 1.0, "opacity")?;
            set_static(&mut c.volume, props.volume, 0.0, 4.0, "volume")?;
            if let Some(pos) = props.position {
                c.transform.position = Animatable::Static(pos);
            }
            if let Some(mb) = props.motion_blur {
                if mb > 16 {
                    return Err(invalid("motion_blur tối đa 16 khung"));
                }
                c.motion_blur = mb;
            }
            if props.remove_transition == Some(true) {
                c.transition_in = None;
            } else if let Some(t) = props.transition_in {
                if first {
                    return Err(invalid("clip đầu tiên không có chuyển cảnh vào"));
                }
                if !(0.05..=3.0).contains(&t.duration) {
                    return Err(invalid("thời lượng chuyển cảnh từ 0.05 đến 3 giây"));
                }
                c.transition_in = Some(t);
            }
        }
        Op::AddText { text, start, end, style, id, words } => {
            check_span(*start, *end)?;
            let clip = TextClip {
                id: id.clone().unwrap_or_else(|| new_id("text")),
                text: text.clone(),
                start: *start,
                end: *end,
                style: style.clone().unwrap_or_default(),
                transform: TextTransform::default(),
                words: words.clone().unwrap_or_default(),
            };
            p.texts.push(clip);
            p.texts.sort_by(|a, b| a.start.total_cmp(&b.start));
        }
        Op::UpdateText { text_id, text, start, end, style } => {
            let t = p.texts.iter_mut().find(|t| &t.id == text_id).ok_or_else(|| invalid(format!("không có chữ '{text_id}'")))?;
            if let Some(s) = text {
                t.text = s.clone();
            }
            let (s, e) = (start.unwrap_or(t.start), end.unwrap_or(t.end));
            check_span(s, e)?;
            t.start = s;
            t.end = e;
            if let Some(st) = style {
                t.style = st.clone();
            }
            p.texts.sort_by(|a, b| a.start.total_cmp(&b.start));
        }
        Op::RemoveText { text_id } => {
            let before = p.texts.len();
            p.texts.retain(|t| &t.id != text_id);
            if p.texts.len() == before {
                return Err(invalid(format!("không có chữ '{text_id}'")));
            }
        }
        Op::AddAudio { media_id, start, src_in, src_out, role, volume, fade_in, fade_out, duck, id } => {
            let media = p.media_by_id(media_id).ok_or_else(|| invalid(format!("không có media '{media_id}'")))?;
            if media.kind == MediaKind::Image {
                return Err(invalid("ảnh không có âm thanh"));
            }
            if media.kind == MediaKind::Video && !media.has_audio {
                return Err(invalid(format!("video '{}' không có âm thanh", media.name)));
            }
            let (a, b) = (src_in.unwrap_or(0.0), src_out.unwrap_or(media.duration));
            check_range(media, a, b)?;
            if finite(*start, "start")? < 0.0 {
                return Err(invalid("start phải >= 0"));
            }
            p.audio.push(AudioClip {
                id: id.clone().unwrap_or_else(|| new_id("audio")),
                media_id: media_id.clone(),
                role: role.unwrap_or_default(),
                start: *start,
                src_in: a,
                src_out: b,
                volume: Animatable::Static(volume.unwrap_or(1.0)),
                fade_in: fade_in.unwrap_or(0.0).max(0.0),
                fade_out: fade_out.unwrap_or(0.0).max(0.0),
                duck: duck.unwrap_or(false),
            });
        }
        Op::SetAudioProps { audio_id, start, src_in, src_out, role, volume, fade_in, fade_out, duck } => {
            let idx = p.audio.iter().position(|a| &a.id == audio_id).ok_or_else(|| invalid(format!("không có âm thanh '{audio_id}'")))?;
            let media = p.media_by_id(&p.audio[idx].media_id).cloned().ok_or_else(|| invalid("âm thanh trỏ tới media không tồn tại"))?;
            let a = &mut p.audio[idx];
            let (si, so) = (src_in.unwrap_or(a.src_in), src_out.unwrap_or(a.src_out));
            check_range(&media, si, so)?;
            if src_in.is_some() {
                let shift = -(si - a.src_in);
                a.volume.shift(shift);
            }
            a.src_in = si;
            a.src_out = so;
            if let Some(s) = start {
                if finite(*s, "start")? < 0.0 {
                    return Err(invalid("start phải >= 0"));
                }
                a.start = *s;
            }
            if let Some(r) = role {
                a.role = *r;
            }
            set_static(&mut a.volume, *volume, 0.0, 4.0, "volume")?;
            if let Some(v) = fade_in {
                a.fade_in = v.max(0.0);
            }
            if let Some(v) = fade_out {
                a.fade_out = v.max(0.0);
            }
            if let Some(v) = duck {
                a.duck = *v;
            }
        }
        Op::RemoveAudio { audio_id } => {
            let before = p.audio.len();
            p.audio.retain(|a| &a.id != audio_id);
            if p.audio.len() == before {
                return Err(invalid(format!("không có âm thanh '{audio_id}'")));
            }
        }
        Op::SetKeyframes { target, property, keyframes } => set_keyframes(p, target, property, keyframes)?,
        Op::RemoveKeyframes { target, property } => remove_keyframes(p, target, property)?,
        Op::ClearTimeline => {
            p.video.clear();
            p.texts.clear();
            p.audio.clear();
        }
        Op::SetSettings { width, height, fps } => {
            let w = width.unwrap_or(p.settings.width);
            let h = height.unwrap_or(p.settings.height);
            let f = fps.unwrap_or(p.settings.fps);
            if !(144..=4096).contains(&w) || !(144..=4096).contains(&h) || w % 2 != 0 || h % 2 != 0 {
                return Err(invalid("kích thước phải là số chẵn từ 144 đến 4096"));
            }
            if !(10..=60).contains(&f) {
                return Err(invalid("fps từ 10 đến 60"));
            }
            p.settings = Settings { width: w, height: h, fps: f };
        }
    }
    Ok(())
}

fn check_span(start: f64, end: f64) -> Result<()> {
    finite(start, "start")?;
    finite(end, "end")?;
    if start < 0.0 || end - start < MIN_CLIP {
        return Err(invalid("start phải >= 0 và end phải lớn hơn start"));
    }
    Ok(())
}

/// Đặt giá trị cố định (xoá keyframe nếu có) khi `v` có giá trị.
fn set_static(slot: &mut Animatable<f64>, v: Option<f64>, min: f64, max: f64, name: &str) -> Result<()> {
    if let Some(v) = v {
        if !v.is_finite() || v < min || v > max {
            return Err(invalid(format!("{name} phải từ {min} đến {max}")));
        }
        *slot = Animatable::Static(v);
    }
    Ok(())
}

fn shift_clip_keyframes(c: &mut VideoClip, dt: f64) {
    c.transform.position.shift(dt);
    c.transform.scale.shift(dt);
    c.transform.rotation.shift(dt);
    c.transform.opacity.shift(dt);
    c.color.brightness.shift(dt);
    c.color.contrast.shift(dt);
    c.color.saturation.shift(dt);
    c.volume.shift(dt);
}

fn split_clip_keyframes(orig: &VideoClip, at: f64, left: &mut VideoClip, right: &mut VideoClip) {
    // Macro nhỏ để khỏi lặp 8 lần cùng một dòng.
    macro_rules! split {
        ($($field:ident).+) => {{
            let (l, r) = orig.$($field).+.split(at);
            left.$($field).+ = l;
            right.$($field).+ = r;
        }};
    }
    split!(transform.position);
    split!(transform.scale);
    split!(transform.rotation);
    split!(transform.opacity);
    split!(color.brightness);
    split!(color.contrast);
    split!(color.saturation);
    split!(volume);
}

/// Chuyển keyframe dạng JSON sang kiểu cụ thể (f64 hoặc Vec2), có kiểm tra khoảng.
fn typed<T: serde::de::DeserializeOwned>(kfs: &[Keyframe<Value>]) -> Result<Vec<Keyframe<T>>> {
    let mut out = Vec::with_capacity(kfs.len());
    for k in kfs {
        let value: T = serde_json::from_value(k.value.clone())
            .map_err(|e| invalid(format!("giá trị keyframe tại t={} sai kiểu: {e}", k.t)))?;
        out.push(Keyframe { t: k.t, value, easing: k.easing });
    }
    normalize(&mut out).map_err(invalid)?;
    Ok(out)
}

fn ranged(kfs: &[Keyframe<Value>], min: f64, max: f64, name: &str) -> Result<Animatable<f64>> {
    let v: Vec<Keyframe<f64>> = typed(kfs)?;
    if v.iter().any(|k| !k.value.is_finite() || k.value < min || k.value > max) {
        return Err(invalid(format!("{name} phải từ {min} đến {max}")));
    }
    Ok(Animatable::Keyframes(v))
}

fn set_keyframes(p: &mut Project, target: &Target, prop: &str, kfs: &[Keyframe<Value>]) -> Result<()> {
    let id = target.id().to_string();
    match target {
        Target::Clip(_) => {
            let i = clip_index(p, &id)?;
            let c = &mut p.video[i];
            match prop {
                "position" => c.transform.position = Animatable::Keyframes(typed::<Vec2>(kfs)?),
                "scale" => c.transform.scale = ranged(kfs, 0.1, 5.0, "scale")?,
                "rotation" => c.transform.rotation = ranged(kfs, -360.0, 360.0, "rotation")?,
                "opacity" => c.transform.opacity = ranged(kfs, 0.0, 1.0, "opacity")?,
                "brightness" => c.color.brightness = ranged(kfs, -1.0, 1.0, "brightness")?,
                "contrast" => c.color.contrast = ranged(kfs, 0.0, 2.0, "contrast")?,
                "saturation" => c.color.saturation = ranged(kfs, 0.0, 3.0, "saturation")?,
                "volume" => c.volume = ranged(kfs, 0.0, 4.0, "volume")?,
                other => return Err(invalid(format!("clip không có thuộc tính keyframe '{other}'"))),
            }
        }
        Target::Text(_) => {
            let t = p.texts.iter_mut().find(|t| t.id == id).ok_or_else(|| invalid(format!("không có chữ '{id}'")))?;
            match prop {
                "position" => t.transform.position = Some(Animatable::Keyframes(typed::<Vec2>(kfs)?)),
                "scale" => t.transform.scale = Some(ranged(kfs, 0.1, 5.0, "scale")?),
                "opacity" => t.transform.opacity = Some(ranged(kfs, 0.0, 1.0, "opacity")?),
                other => return Err(invalid(format!("chữ không có thuộc tính keyframe '{other}'"))),
            }
        }
        Target::Audio(_) => {
            let a = p.audio.iter_mut().find(|a| a.id == id).ok_or_else(|| invalid(format!("không có âm thanh '{id}'")))?;
            match prop {
                "volume" => a.volume = ranged(kfs, 0.0, 4.0, "volume")?,
                other => return Err(invalid(format!("âm thanh không có thuộc tính keyframe '{other}'"))),
            }
        }
    }
    Ok(())
}

fn flatten<T: crate::keyframe::Lerp>(a: &mut Animatable<T>) {
    let v = a.value_at(0.0);
    *a = Animatable::Static(v);
}

fn remove_keyframes(p: &mut Project, target: &Target, prop: &str) -> Result<()> {
    let id = target.id().to_string();
    match target {
        Target::Clip(_) => {
            let i = clip_index(p, &id)?;
            let c = &mut p.video[i];
            match prop {
                "position" => flatten(&mut c.transform.position),
                "scale" => flatten(&mut c.transform.scale),
                "rotation" => flatten(&mut c.transform.rotation),
                "opacity" => flatten(&mut c.transform.opacity),
                "brightness" => flatten(&mut c.color.brightness),
                "contrast" => flatten(&mut c.color.contrast),
                "saturation" => flatten(&mut c.color.saturation),
                "volume" => flatten(&mut c.volume),
                other => return Err(invalid(format!("clip không có thuộc tính '{other}'"))),
            }
        }
        Target::Text(_) => {
            let t = p.texts.iter_mut().find(|t| t.id == id).ok_or_else(|| invalid(format!("không có chữ '{id}'")))?;
            match prop {
                "position" => t.transform.position = None,
                "scale" => t.transform.scale = None,
                "opacity" => t.transform.opacity = None,
                other => return Err(invalid(format!("chữ không có thuộc tính '{other}'"))),
            }
        }
        Target::Audio(_) => {
            let a = p.audio.iter_mut().find(|a| a.id == id).ok_or_else(|| invalid(format!("không có âm thanh '{id}'")))?;
            match prop {
                "volume" => flatten(&mut a.volume),
                other => return Err(invalid(format!("âm thanh không có thuộc tính '{other}'"))),
            }
        }
    }
    Ok(())
}

/// Tóm tắt khác biệt giữa hai bản project, để hiện cho người dùng duyệt.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Diff {
    pub lines: Vec<String>,
    /// Id các clip/chữ/âm thanh bị thêm hoặc sửa (để tô viền cam).
    pub affected: Vec<String>,
    pub duration_before: f64,
    pub duration_after: f64,
}

pub fn diff(before: &Project, after: &Project) -> Diff {
    let mut d = Diff { duration_before: before.duration(), duration_after: after.duration(), ..Default::default() };
    let name = |p: &Project, media_id: &str| p.media_by_id(media_id).map(|m| m.name.clone()).unwrap_or_else(|| media_id.to_string());

    for c in &after.video {
        match before.video.iter().find(|b| b.id == c.id) {
            None => {
                d.lines.push(format!("+ Thêm clip {} ({:.1}s)", name(after, &c.media_id), c.duration()));
                d.affected.push(c.id.clone());
            }
            Some(b) => {
                let changes = clip_changes(b, c);
                if !changes.is_empty() {
                    d.lines.push(format!("~ {}: {}", name(after, &c.media_id), changes.join(", ")));
                    d.affected.push(c.id.clone());
                }
            }
        }
    }
    for b in &before.video {
        if !after.video.iter().any(|c| c.id == b.id) {
            d.lines.push(format!("− Xoá clip {} ({:.1}s)", name(before, &b.media_id), b.duration()));
        }
    }
    let order = |p: &Project| p.video.iter().map(|c| c.id.clone()).filter(|id| before.video.iter().any(|b| &b.id == id) && after.video.iter().any(|a| &a.id == id)).collect::<Vec<_>>();
    if order(before) != order(after) {
        d.lines.push("~ Đổi thứ tự clip".into());
    }

    for t in &after.texts {
        match before.texts.iter().find(|b| b.id == t.id) {
            None => {
                d.lines.push(format!("+ Thêm chữ \"{}\" ({:.1}–{:.1}s)", short(&t.text), t.start, t.end));
                d.affected.push(t.id.clone());
            }
            Some(b) if serde_json::to_value(b).ok() != serde_json::to_value(t).ok() => {
                d.lines.push(format!("~ Sửa chữ \"{}\"", short(&t.text)));
                d.affected.push(t.id.clone());
            }
            _ => {}
        }
    }
    for b in &before.texts {
        if !after.texts.iter().any(|t| t.id == b.id) {
            d.lines.push(format!("− Xoá chữ \"{}\"", short(&b.text)));
        }
    }
    for a in &after.audio {
        match before.audio.iter().find(|b| b.id == a.id) {
            None => {
                d.lines.push(format!("+ Thêm âm thanh {} tại {:.1}s", name(after, &a.media_id), a.start));
                d.affected.push(a.id.clone());
            }
            Some(b) if serde_json::to_value(b).ok() != serde_json::to_value(a).ok() => {
                d.lines.push(format!("~ Sửa âm thanh {}", name(after, &a.media_id)));
                d.affected.push(a.id.clone());
            }
            _ => {}
        }
    }
    for b in &before.audio {
        if !after.audio.iter().any(|a| a.id == b.id) {
            d.lines.push(format!("− Xoá âm thanh {}", name(before, &b.media_id)));
        }
    }
    if before.settings != after.settings {
        let s = after.settings;
        d.lines.push(format!("~ Khung hình {}x{} @{}fps", s.width, s.height, s.fps));
    }
    d
}

fn short(s: &str) -> String {
    let t: String = s.chars().take(30).collect();
    if s.chars().count() > 30 { format!("{t}…") } else { t }
}

fn clip_changes(b: &VideoClip, a: &VideoClip) -> Vec<String> {
    let mut v = Vec::new();
    if (b.src_in, b.src_out) != (a.src_in, a.src_out) {
        v.push(format!("cắt {:.1}–{:.1}s", a.src_in, a.src_out));
    }
    if b.speed != a.speed {
        v.push(format!("tốc độ {}×", a.speed));
    }
    if b.color.preset != a.color.preset {
        v.push(format!("filter {}", a.color.preset.label()));
    }
    if b.color != a.color && b.color.preset == a.color.preset {
        v.push("chỉnh màu".into());
    }
    if b.transform != a.transform {
        v.push(if a.transform.scale.is_animated() || a.transform.position.is_animated() { "chuyển động (keyframe)".into() } else { "vị trí/kích thước".into() });
    }
    if b.motion_blur != a.motion_blur {
        v.push(if a.motion_blur > 0 { format!("motion blur {} khung", a.motion_blur) } else { "tắt motion blur".into() });
    }
    if b.transition_in != a.transition_in {
        v.push(match a.transition_in {
            Some(t) => format!("chuyển cảnh {:?} {:.1}s", t.kind, t.duration),
            None => "bỏ chuyển cảnh".into(),
        });
    }
    if b.volume != a.volume {
        v.push("âm lượng".into());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Project {
        let mut p = Project::new("test");
        p.media.push(MediaItem {
            id: "m1".into(),
            path: "a.mp4".into(),
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
        });
        p
    }

    fn ops(json: &str) -> Vec<Op> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn them_tach_xoa() {
        let p = sample();
        let p = apply(&p, &ops(r#"[{"op":"add_clip","media_id":"m1","src_in":1,"src_out":7,"id":"c1"}]"#)).unwrap();
        assert_eq!(p.duration(), 6.0);
        let p = apply(&p, &ops(r#"[{"op":"split_clip","clip_id":"c1","at":2,"new_id":"c2"}]"#)).unwrap();
        assert_eq!(p.video.len(), 2);
        assert_eq!(p.video[0].src_out, 3.0);
        assert_eq!(p.video[1].src_in, 3.0);
        let p = apply(&p, &ops(r#"[{"op":"remove_clip","clip_id":"c1"}]"#)).unwrap();
        assert_eq!(p.video.len(), 1);
    }

    #[test]
    fn loi_thi_khong_doi_gi() {
        let p = sample();
        let r = apply(&p, &ops(r#"[{"op":"add_clip","media_id":"m1","id":"c1"},{"op":"add_clip","media_id":"m1","src_out":99}]"#));
        assert!(r.is_err());
        assert!(p.video.is_empty());
    }

    #[test]
    fn chuyen_canh_rut_ngan_timeline() {
        let p = sample();
        let p = apply(&p, &ops(r#"[
            {"op":"add_clip","media_id":"m1","src_in":0,"src_out":4,"id":"a"},
            {"op":"add_clip","media_id":"m1","src_in":4,"src_out":8,"id":"b"},
            {"op":"set_clip_props","clip_id":"b","transition_in":{"kind":"fade","duration":1}}
        ]"#)).unwrap();
        assert_eq!(p.duration(), 7.0);
    }

    #[test]
    fn keyframe_va_tach() {
        let p = sample();
        let p = apply(&p, &ops(r#"[
            {"op":"add_clip","media_id":"m1","src_in":0,"src_out":4,"id":"a"},
            {"op":"set_keyframes","target":{"kind":"clip","id":"a"},"property":"scale",
             "keyframes":[{"t":0,"value":1},{"t":4,"value":1.4}]},
            {"op":"split_clip","clip_id":"a","at":2,"new_id":"b"}
        ]"#)).unwrap();
        assert!((p.video[1].transform.scale.value_at(0.0) - 1.2).abs() < 1e-9);
        assert!((p.video[1].transform.scale.value_at(2.0) - 1.4).abs() < 1e-9);
        let d = diff(&sample(), &p);
        assert_eq!(d.affected.len(), 2);
    }

    #[test]
    fn cat_dau_doi_keyframe() {
        let p = sample();
        let p = apply(&p, &ops(r#"[
            {"op":"add_clip","media_id":"m1","src_in":0,"src_out":4,"id":"a"},
            {"op":"set_keyframes","target":{"kind":"clip","id":"a"},"property":"opacity",
             "keyframes":[{"t":2,"value":0},{"t":3,"value":1}]},
            {"op":"trim_clip","clip_id":"a","src_in":1}
        ]"#)).unwrap();
        assert_eq!(p.video[0].transform.opacity.times(), vec![1.0, 2.0]);
    }
}
