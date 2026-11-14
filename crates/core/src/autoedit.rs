//! "Dựng nhanh": tự ghép một bản vlog từ các cảnh tốt, KHÔNG cần Claude.
//!
//! Đây là thuật toán đơn giản, dùng làm bản nháp hoặc khi chưa kết nối Claude:
//! chọn cảnh điểm cao nhất của mỗi cảnh quay, cắt theo nhịp của mood,
//! sắp theo thời gian quay, thêm filter, chuyển cảnh, chữ "Ngày 1, 2…" và nhạc nền.
//! Kết quả là danh sách `Op`, được ghi thành đề xuất để người dùng duyệt.

use crate::ops::{self, Op};
use crate::presets::FilterPreset;
use crate::project::{MediaKind, Project, TransitionKind};

fn tr(id: &str) -> TransitionKind {
    TransitionKind::from_id(id).unwrap_or_default()
}
use crate::store::Brief;
use serde_json::json;

/// Thông số theo mood.
struct Style {
    /// Độ dài mỗi cảnh (giây) khi không có tiếng nói.
    clip: f64,
    preset: FilterPreset,
    transition: Option<(TransitionKind, f64)>,
}

fn style(mood: &str) -> Style {
    match mood {
        "hype" => Style { clip: 1.3, preset: FilterPreset::Vivid, transition: None },
        "emotional" => Style { clip: 3.5, preset: FilterPreset::Warm, transition: Some((tr("fade"), 0.7)) },
        "fun" => Style { clip: 1.8, preset: FilterPreset::Vivid, transition: Some((tr("whip_left"), 0.3)) },
        _ => Style { clip: 2.6, preset: FilterPreset::Cinematic, transition: Some((tr("dissolve"), 0.4)) },
    }
}

/// Một cảnh ứng viên.
#[derive(Clone)]
struct Cand {
    media_id: String,
    kind: MediaKind,
    /// Khoá sắp xếp theo thời gian: (ngày quay, tên file, giây bắt đầu).
    taken: String,
    name: String,
    start: f64,
    end: f64,
    shot: (usize, u32),
    score: f64,
    speech: bool,
}

impl Cand {
    /// Độ dài sẽ dùng trên timeline.
    fn use_len(&self, st: &Style, keep_speech: bool) -> f64 {
        let avail = self.end - self.start;
        if self.kind == MediaKind::Image {
            return st.clip.max(2.0);
        }
        if keep_speech && self.speech {
            return avail.min(8.0);
        }
        avail.min(st.clip)
    }
}

pub struct Plan {
    pub ops: Vec<Op>,
    pub description: String,
}

/// Lập kế hoạch dựng. Trả lỗi dạng chữ nếu không có cảnh nào dùng được.
pub fn quick_plan(p: &Project, brief: &Brief) -> Result<Plan, String> {
    let st = style(&brief.mood);
    let mut cands = Vec::new();
    for (mi, m) in p.media.iter().enumerate() {
        if m.kind == MediaKind::Audio {
            continue;
        }
        for s in m.scenes.iter().filter(|s| s.is_kept()) {
            let end = if m.kind == MediaKind::Image { crate::ops::IMAGE_DEFAULT_SECS } else { s.end };
            if m.kind == MediaKind::Video && end - s.start < 0.6 {
                continue;
            }
            cands.push(Cand {
                media_id: m.id.clone(),
                kind: m.kind,
                taken: m.taken_at().unwrap_or("").to_string(),
                name: m.name.clone(),
                start: s.start,
                end,
                shot: (mi, s.shot),
                score: s.score,
                speech: s.tags.contains(&crate::project::SceneTag::Speech),
            });
        }
    }
    if cands.is_empty() {
        return Err("Chưa có cảnh nào dùng được. Hãy nhập video và chạy phân tích trước, hoặc đánh dấu giữ một số cảnh.".into());
    }

    // Mỗi cảnh quay giữ 1 đoạn điểm cao nhất; nếu còn thiếu thời lượng thì lấy thêm đoạn thứ 2, 3…
    let target = brief.target_secs.unwrap_or(f64::INFINITY);
    let tr = st.transition.map(|t| t.1).unwrap_or(0.0);
    let mut by_shot: std::collections::BTreeMap<(usize, u32), Vec<Cand>> = Default::default();
    for c in cands {
        by_shot.entry(c.shot).or_default().push(c);
    }
    for v in by_shot.values_mut() {
        v.sort_by(|a, b| b.score.total_cmp(&a.score));
    }
    let mut chosen: Vec<Cand> = Vec::new();
    let total = |v: &[Cand]| v.iter().map(|c| c.use_len(&st, brief.keep_speech) - tr).sum::<f64>() + tr;
    for round in 0..4 {
        let mut added = false;
        for v in by_shot.values() {
            if let Some(c) = v.get(round) {
                if round == 0 || total(&chosen) < target {
                    chosen.push(c.clone());
                    added = true;
                }
            }
        }
        if !added || total(&chosen) >= target {
            break;
        }
    }
    // Quá dài: bỏ dần cảnh điểm thấp nhất.
    while chosen.len() > 1 && total(&chosen) > target * 1.05 {
        let worst = chosen.iter().enumerate().min_by(|a, b| a.1.score.total_cmp(&b.1.score)).map(|(i, _)| i).unwrap();
        chosen.remove(worst);
    }
    chosen.sort_by(|a, b| (a.taken.as_str(), a.name.as_str()).cmp(&(b.taken.as_str(), b.name.as_str())).then(a.start.total_cmp(&b.start)));
    if !brief.chronological {
        // Mở đầu bằng cảnh đẹp nhất, phần còn lại vẫn theo thời gian.
        if let Some(best) = chosen.iter().enumerate().max_by(|a, b| a.1.score.total_cmp(&b.1.score)).map(|(i, _)| i) {
            let c = chosen.remove(best);
            chosen.insert(0, c);
        }
    }

    // Độ dài từng cảnh. Nếu ít cảnh tốt mà còn thiếu thời lượng, kéo dài các cảnh
    // (tối đa 2.5 lần nhịp chuẩn, không quá độ dài cảnh) cho tới khi đủ.
    let mut lens: Vec<f64> = chosen.iter().map(|c| c.use_len(&st, brief.keep_speech)).collect();
    if target.is_finite() {
        for _ in 0..20 {
            let cur: f64 = lens.iter().map(|l| l - tr).sum::<f64>() + tr;
            if cur >= target {
                break;
            }
            let growable: Vec<usize> = (0..chosen.len())
                .filter(|&i| {
                    let c = &chosen[i];
                    let max = if c.kind == MediaKind::Image { st.clip * 2.0 } else { (c.end - c.start).min(st.clip * 2.5).max(lens[i]) };
                    lens[i] + 0.05 < max
                })
                .collect();
            if growable.is_empty() {
                break;
            }
            let add = (target - cur) / growable.len() as f64;
            for i in growable {
                let c = &chosen[i];
                let max = if c.kind == MediaKind::Image { st.clip * 2.0 } else { (c.end - c.start).min(st.clip * 2.5).max(lens[i]) };
                lens[i] = (lens[i] + add).min(max);
            }
        }
    }

    // ---- Op cho clip ----
    let mut ops: Vec<Op> = vec![Op::ClearTimeline, Op::SetSettings { width: Some(brief.width), height: Some(brief.height), fps: None }];
    let music = brief
        .music_media_id
        .clone()
        .filter(|id| p.media_by_id(id).is_some())
        .or_else(|| p.media.iter().find(|m| m.kind == MediaKind::Audio).map(|m| m.id.clone()));
    for (i, c) in chosen.iter().enumerate() {
        let len = lens[i];
        let id = format!("auto_{i:03}");
        let (a, b) = if c.kind == MediaKind::Image {
            (0.0, len)
        } else {
            // Lấy đoạn giữa cảnh (tránh phần đầu/cuối hay rung hoặc dính chuyển cảnh).
            let mid = (c.start + c.end) / 2.0;
            let a = (mid - len / 2.0).max(c.start);
            (a, (a + len).min(c.end))
        };
        ops.push(Op::AddClip { media_id: c.media_id.clone(), src_in: Some(a), src_out: Some(b), index: None, id: Some(id.clone()) });
        let mut props = json!({ "op": "set_clip_props", "clip_id": id, "preset": st.preset });
        if i > 0 {
            if let Some((kind, d)) = st.transition {
                props["transition_in"] = json!({ "kind": kind, "duration": d });
            }
        }
        // Có nhạc nền: hạ tiếng gốc của cảnh không có người nói.
        if music.is_some() && !(brief.keep_speech && c.speech) {
            props["volume"] = json!(0.15);
        }
        ops.push(serde_json::from_value(props).expect("op hợp lệ"));
        if c.kind == MediaKind::Image {
            // Ken Burns cho ảnh tĩnh.
            ops.push(serde_json::from_value(json!({
                "op": "set_keyframes", "target": { "kind": "clip", "id": id }, "property": "scale",
                "keyframes": [{ "t": 0, "value": 1.0, "easing": "ease_in_out" }, { "t": len, "value": 1.15 }]
            })).unwrap());
        }
    }

    // Tính vị trí thật trên timeline để đặt chữ và nhạc.
    let staged = ops::apply(p, &ops).map_err(|e| e.to_string())?;
    let layout = staged.video_layout();
    let duration = staged.duration();

    if brief.place_titles {
        ops.push(Op::AddText {
            text: p.name.clone(),
            start: 0.2,
            end: (0.2 + 2.8f64).min(duration).max(0.4),
            style: Some(serde_json::from_value(json!({ "size": 110, "align": "center", "anim_in": "pop" })).unwrap()),
            id: Some("auto_title".into()),
            words: None,
        });
        // Chữ "Ngày N · dd/mm" khi sang ngày mới.
        let mut last_day = String::new();
        let mut day_no = 0;
        for (i, c) in chosen.iter().enumerate() {
            let day = c.taken.get(..10).unwrap_or("").to_string();
            if day.is_empty() || day == last_day {
                continue;
            }
            day_no += 1;
            last_day = day.clone();
            if day_no == 1 && layout[i].start < 3.0 {
                continue; // đã có tiêu đề mở đầu
            }
            let label = format!("Ngày {day_no} · {}/{}", &day[8..10], &day[5..7]);
            let s = layout[i].start + 0.2;
            ops.push(Op::AddText { text: label, start: s, end: (s + 2.5).min(duration), style: Some(serde_json::from_value(json!({ "size": 80, "align": "top", "anim_in": "slide_up" })).unwrap()), id: Some(format!("auto_day{day_no}")), words: None });
        }
    }

    if let Some(mid) = &music {
        let m = p.media_by_id(mid).unwrap();
        let len = duration.min(m.duration);
        if len > 0.5 {
            ops.push(Op::AddAudio {
                media_id: mid.clone(),
                start: 0.0,
                src_in: Some(0.0),
                src_out: Some(len),
                role: None,
                volume: Some(0.8),
                fade_in: Some(0.8f64.min(len / 4.0)),
                fade_out: Some(2.0f64.min(len / 3.0)),
                duck: Some(brief.keep_speech),
                id: Some("auto_music".into()),
            });
        }
    }

    // Kiểm tra toàn bộ một lần cho chắc.
    let final_p = ops::apply(p, &ops).map_err(|e| e.to_string())?;
    let mood = match brief.mood.as_str() {
        "hype" => "sôi động",
        "emotional" => "cảm xúc",
        "fun" => "vui nhộn",
        _ => "chill",
    };
    let mut description = format!("Dựng nhanh (không dùng Claude): {} cảnh, {:.0} giây, vibe {mood}", chosen.len(), final_p.duration());
    if music.is_none() {
        description.push_str(". Chưa có nhạc nền trong thư viện");
    }
    Ok(Plan { ops, description })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{MediaItem, Scene, SceneTag};

    fn media(id: &str, kind: MediaKind, dur: f64, taken: &str, scenes: Vec<Scene>) -> MediaItem {
        MediaItem {
            id: id.into(),
            path: id.into(),
            name: id.into(),
            kind,
            duration: dur,
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_audio: true,
            created_at: Some(taken.into()),
            gps: None,
            thumb: None,
            proxy: None,
            scenes,
            analysis_version: 1,
            modified_at: None,
            transcribed: false,
        }
    }
    fn scene(id: &str, a: f64, b: f64, shot: u32, score: f64, tags: Vec<SceneTag>) -> Scene {
        Scene { id: id.into(), start: a, end: b, shot, score, tags, ..Default::default() }
    }

    #[test]
    fn dung_nhanh_dung_do_dai_va_thu_tu() {
        let mut p = Project::new("Đà Lạt");
        p.media.push(media("v2", MediaKind::Video, 20.0, "2026-10-13T08:00:00Z", vec![
            scene("a", 0.0, 8.0, 0, 80.0, vec![SceneTag::Good]),
            scene("b", 8.0, 16.0, 0, 60.0, vec![]),
            scene("c", 16.0, 20.0, 1, 30.0, vec![SceneTag::Blurry]),
        ]));
        p.media.push(media("v1", MediaKind::Video, 12.0, "2026-10-12T08:00:00Z", vec![
            scene("d", 0.0, 6.0, 0, 70.0, vec![SceneTag::Good, SceneTag::Speech]),
            scene("e", 6.0, 12.0, 1, 75.0, vec![SceneTag::Good]),
        ]));
        p.media.push(media("img", MediaKind::Image, 0.0, "2026-10-13T09:00:00Z", vec![scene("f", 0.0, 0.0, 0, 50.0, vec![SceneTag::Photo])]));
        p.media.push(media("nhac", MediaKind::Audio, 120.0, "", vec![]));

        let brief = Brief { target_secs: Some(15.0), ..Default::default() };
        let plan = quick_plan(&p, &brief).unwrap();
        let out = ops::apply(&p, &plan.ops).unwrap();
        // Cảnh mờ bị loại, ngày 12 đứng trước ngày 13.
        assert!(out.video.iter().all(|c| !(c.media_id == "v2" && c.src_in >= 16.0)));
        assert_eq!(out.video[0].media_id, "v1");
        assert!(out.duration() <= 15.0 * 1.05 + 0.01, "dài {}", out.duration());
        assert!(out.duration() >= 14.0, "ngắn quá: {}", out.duration());
        assert_eq!(out.audio.len(), 1);
        assert!(out.texts.iter().any(|t| t.text.starts_with("Ngày 2")));
        assert_eq!((out.settings.width, out.settings.height), (1080, 1920));
        // Cảnh có tiếng nói giữ nguyên âm lượng.
        let speech = out.video.iter().find(|c| c.media_id == "v1" && c.src_in < 6.0).unwrap();
        assert_eq!(speech.volume, crate::keyframe::Animatable::Static(1.0));
    }

    #[test]
    fn khong_co_canh() {
        let p = Project::new("x");
        assert!(quick_plan(&p, &Brief::default()).is_err());
    }
}
