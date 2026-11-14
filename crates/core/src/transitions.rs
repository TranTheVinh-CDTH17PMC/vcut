//! Danh sách hiệu ứng chuyển cảnh.
//!
//! Một bảng duy nhất (`ALL`) dùng chung cho render, giao diện (qua `app_info`) và
//! Claude (mô tả trong MCP). Mỗi mục gồm:
//! - `id`: tên lưu trong project.json, vd. `"slide_left"`.
//! - `xfade`: kiểu chuyển cảnh gốc của filter `xfade` trong FFmpeg (56 kiểu có sẵn).
//! - `motion`: hiệu ứng chuyển động thêm ở **cuối clip trước** và **đầu clip sau**
//!   (xoay, zoom xuyên, rung, lia nhanh, nhiễu, chớp sáng) — kiểu CapCut. Các hiệu ứng
//!   này ghép từ filter FFmpeg thường (scale, crop, rotate, avgblur…) nên render nhanh,
//!   không dùng biểu thức tính từng điểm ảnh (chậm gấp hàng chục lần).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Hiệu ứng chuyển động thêm quanh điểm chuyển cảnh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    None,
    /// Clip trước phóng to lao vào, clip sau thu từ to về bình thường.
    ZoomThrough,
    /// Xoay + phóng to. `true` = theo chiều kim đồng hồ.
    Spin(bool),
    /// Rung lắc mạnh dần rồi tắt.
    Shake,
    /// Lia máy nhanh: nhoè theo chiều ngang (`true`) hoặc dọc (`false`).
    Whip(bool),
    /// Lệch màu RGB + nhiễu hạt.
    Glitch,
    /// Sáng loá lên rồi tắt.
    Flash,
}

/// Một kiểu chuyển cảnh trong bảng.
#[derive(Debug, Serialize)]
pub struct TransitionInfo {
    pub id: &'static str,
    /// Tên tiếng Việt hiện trong app.
    pub name: &'static str,
    /// Nhóm để chia tab trong app.
    pub group: &'static str,
    #[serde(skip)]
    pub xfade: &'static str,
    #[serde(skip)]
    pub motion: Motion,
}

const fn t(id: &'static str, name: &'static str, group: &'static str, xfade: &'static str, motion: Motion) -> TransitionInfo {
    TransitionInfo { id, name, group, xfade, motion }
}

use Motion::*;

pub const GROUPS: &[&str] = &["Động", "Cơ bản", "Trượt", "Quét", "Hình", "Hiệu ứng"];

/// Toàn bộ kiểu chuyển cảnh. Thứ tự trong mỗi nhóm = thứ tự hiện trong app.
pub static ALL: &[TransitionInfo] = &[
    // Động (kiểu CapCut): xfade gốc + chuyển động ở hai đầu clip.
    t("zoom_through", "Zoom xuyên", "Động", "fade", ZoomThrough),
    t("spin", "Xoay", "Động", "fade", Spin(true)),
    t("spin_ccw", "Xoay ngược", "Động", "fade", Spin(false)),
    t("shake", "Rung", "Động", "fade", Shake),
    t("whip_left", "Lia trái", "Động", "slideleft", Whip(true)),
    t("whip_right", "Lia phải", "Động", "slideright", Whip(true)),
    t("whip_up", "Lia lên", "Động", "slideup", Whip(false)),
    t("whip_down", "Lia xuống", "Động", "slidedown", Whip(false)),
    t("glitch", "Nhiễu sóng", "Động", "fadefast", Glitch),
    t("flash", "Chớp sáng", "Động", "fadewhite", Flash),
    // Cơ bản
    t("fade", "Mờ dần", "Cơ bản", "fade", None),
    t("fade_fast", "Mờ nhanh", "Cơ bản", "fadefast", None),
    t("fade_slow", "Mờ chậm", "Cơ bản", "fadeslow", None),
    t("dissolve", "Hoà tan", "Cơ bản", "dissolve", None),
    t("fade_black", "Qua màn đen", "Cơ bản", "fadeblack", None),
    t("fade_white", "Loé trắng", "Cơ bản", "fadewhite", None),
    t("fade_grays", "Qua đen trắng", "Cơ bản", "fadegrays", None),
    t("distance", "Hoà sáng", "Cơ bản", "distance", None),
    // Trượt
    t("slide_left", "Trượt trái", "Trượt", "slideleft", None),
    t("slide_right", "Trượt phải", "Trượt", "slideright", None),
    t("slide_up", "Trượt lên", "Trượt", "slideup", None),
    t("slide_down", "Trượt xuống", "Trượt", "slidedown", None),
    t("smooth_left", "Lướt trái", "Trượt", "smoothleft", None),
    t("smooth_right", "Lướt phải", "Trượt", "smoothright", None),
    t("smooth_up", "Lướt lên", "Trượt", "smoothup", None),
    t("smooth_down", "Lướt xuống", "Trượt", "smoothdown", None),
    t("cover_left", "Phủ từ phải", "Trượt", "coverleft", None),
    t("cover_right", "Phủ từ trái", "Trượt", "coverright", None),
    t("cover_up", "Phủ từ dưới", "Trượt", "coverup", None),
    t("cover_down", "Phủ từ trên", "Trượt", "coverdown", None),
    t("reveal_left", "Kéo ra trái", "Trượt", "revealleft", None),
    t("reveal_right", "Kéo ra phải", "Trượt", "revealright", None),
    t("reveal_up", "Kéo lên", "Trượt", "revealup", None),
    t("reveal_down", "Kéo xuống", "Trượt", "revealdown", None),
    // Quét
    t("wipe_left", "Quét trái", "Quét", "wipeleft", None),
    t("wipe_right", "Quét phải", "Quét", "wiperight", None),
    t("wipe_up", "Quét lên", "Quét", "wipeup", None),
    t("wipe_down", "Quét xuống", "Quét", "wipedown", None),
    t("wipe_tl", "Quét góc ↖", "Quét", "wipetl", None),
    t("wipe_tr", "Quét góc ↗", "Quét", "wipetr", None),
    t("wipe_bl", "Quét góc ↙", "Quét", "wipebl", None),
    t("wipe_br", "Quét góc ↘", "Quét", "wipebr", None),
    t("diag_tl", "Chéo ↖", "Quét", "diagtl", None),
    t("diag_tr", "Chéo ↗", "Quét", "diagtr", None),
    t("diag_bl", "Chéo ↙", "Quét", "diagbl", None),
    t("diag_br", "Chéo ↘", "Quét", "diagbr", None),
    t("radial", "Quay kim đồng hồ", "Quét", "radial", None),
    // Hình
    t("circle_open", "Mở tròn", "Hình", "circleopen", None),
    t("circle_close", "Đóng tròn", "Hình", "circleclose", None),
    t("circle_crop", "Khung tròn", "Hình", "circlecrop", None),
    t("rect_crop", "Khung vuông", "Hình", "rectcrop", None),
    t("vert_open", "Mở dọc", "Hình", "vertopen", None),
    t("vert_close", "Đóng dọc", "Hình", "vertclose", None),
    t("horz_open", "Mở ngang", "Hình", "horzopen", None),
    t("horz_close", "Đóng ngang", "Hình", "horzclose", None),
    t("squeeze_h", "Ép ngang", "Hình", "squeezeh", None),
    t("squeeze_v", "Ép dọc", "Hình", "squeezev", None),
    // Hiệu ứng
    t("zoom_in", "Phóng to", "Hiệu ứng", "zoomin", None),
    t("pixelize", "Vỡ điểm ảnh", "Hiệu ứng", "pixelize", None),
    t("hblur", "Nhoè ngang", "Hiệu ứng", "hblur", None),
    t("slice_left", "Cắt lát trái", "Hiệu ứng", "hlslice", None),
    t("slice_right", "Cắt lát phải", "Hiệu ứng", "hrslice", None),
    t("slice_up", "Cắt lát lên", "Hiệu ứng", "vuslice", None),
    t("slice_down", "Cắt lát xuống", "Hiệu ứng", "vdslice", None),
    t("wind_left", "Gió trái", "Hiệu ứng", "hlwind", None),
    t("wind_right", "Gió phải", "Hiệu ứng", "hrwind", None),
    t("wind_up", "Gió lên", "Hiệu ứng", "vuwind", None),
    t("wind_down", "Gió xuống", "Hiệu ứng", "vdwind", None),
];

pub fn find(id: &str) -> Option<&'static TransitionInfo> {
    ALL.iter().find(|t| t.id == id)
}

/// Kiểu chuyển cảnh của một clip.
///
/// Bên trong chỉ là tham chiếu `&'static` tới một mục của bảng `ALL`: chép rẻ
/// (`Copy`), so sánh bằng `id`. Khi lưu JSON chỉ ghi `id` (vd. `"spin"`), nên file
/// project cũ vẫn đọc được vì các tên cũ giữ nguyên.
#[derive(Clone, Copy)]
pub struct TransitionKind(&'static TransitionInfo);

impl TransitionKind {
    /// Tạo từ id; `None` nếu không có trong bảng.
    pub fn from_id(id: &str) -> Option<Self> {
        find(id).map(TransitionKind)
    }
    pub fn id(self) -> &'static str {
        self.0.id
    }
    pub fn info(self) -> &'static TransitionInfo {
        self.0
    }
    /// Tên kiểu trong filter `xfade` của FFmpeg.
    pub fn xfade_name(self) -> &'static str {
        self.0.xfade
    }
    pub fn motion(self) -> Motion {
        self.0.motion
    }
}

impl Default for TransitionKind {
    fn default() -> Self {
        TransitionKind::from_id("fade").expect("bảng phải có fade")
    }
}

impl PartialEq for TransitionKind {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
impl Eq for TransitionKind {}

impl std::fmt::Debug for TransitionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.id)
    }
}

// serde: tự viết Serialize/Deserialize thay vì `derive` vì ta lưu dạng chuỗi id.
impl Serialize for TransitionKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0.id)
    }
}

impl<'de> Deserialize<'de> for TransitionKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let id = String::deserialize(d)?;
        TransitionKind::from_id(&id).ok_or_else(|| serde::de::Error::custom(format!("không có kiểu chuyển cảnh \"{id}\". Các kiểu hợp lệ: {}", ids().join(", "))))
    }
}

/// Mọi id, dùng cho thông báo lỗi và mô tả gửi Claude.
pub fn ids() -> Vec<&'static str> {
    ALL.iter().map(|t| t.id).collect()
}

/// Phía của clip mà hiệu ứng chuyển động áp vào.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// Đầu clip (clip sau trong một lần chuyển cảnh).
    Head,
    /// Cuối clip (clip trước).
    Tail,
}

/// Filter FFmpeg cho hiệu ứng chuyển động ở một đầu clip.
///
/// - `dur`: độ dài clip (giây), `tr`: thời lượng chuyển cảnh.
/// - Thời gian trong clip đã bắt đầu từ 0 (filter đặt sau `trim,setpts=PTS-STARTPTS`).
/// - `E` (0→1) là "độ mạnh": 0 ở xa điểm chuyển, 1 đúng lúc chuyển. Ở cuối clip E tăng
///   dần, ở đầu clip E giảm dần, nên hai clip nối nhau liền mạch qua `xfade`.
pub fn motion_filters(m: Motion, side: Side, dur: f64, tr: f64, w: u32, h: u32, fps: u32) -> Vec<String> {
    if m == Motion::None || tr <= 0.0 {
        return Vec::new();
    }
    let n = crate::keyframe::fmt_num;
    let start = (dur - tr).max(0.0);
    // Biểu thức E theo biến thời gian `v` (giây tính từ đầu clip).
    let e = |v: &str| match side {
        Side::Tail => format!("clip(({v}-{})/{},0,1)", n(start), n(tr)),
        Side::Head => format!("clip(1-{v}/{},0,1)", n(tr)),
    };
    // Khoảng thời gian có hiệu ứng (`enable=` cho filter hỗ trợ bật/tắt theo thời gian).
    // `inner` = phần sát điểm chuyển (dùng cho nhoè/nhiễu để không kéo dài quá).
    let window = |inner: f64| match side {
        Side::Tail => format!("gte(t,{})", n(dur - tr * inner)),
        Side::Head => format!("lte(t,{})", n(tr * inner)),
    };
    // Phóng to quanh tâm: `scale` tính lại kích thước mỗi khung (eval=frame) rồi `crop`
    // cắt về đúng khung. Nhanh hơn `zoompan` khoảng 40 lần (zoompan tạo lại bộ scale
    // cho từng khung). `x`/`y` của crop cũng tính theo từng khung nên dùng được để rung.
    let zoom = |z: &str, x: &str, y: &str| {
        vec![
            format!("scale=w='trunc(iw*({z})/2)*2':h='trunc(ih*({z})/2)*2':eval=frame"),
            format!("crop={w}:{h}:x='{x}':y='{y}'"),
        ]
    };
    let cx = "(iw-ow)/2";
    let cy = "(ih-oh)/2";
    let et = e("t");
    let _ = fps;

    match m {
        Motion::None => Vec::new(),
        Motion::ZoomThrough => zoom(&format!("1+2.5*pow({et},2)"), cx, cy),
        Motion::Spin(cw) => {
            // Clip trước xoay ra, clip sau xoay vào theo cùng chiều → liền một vòng.
            let sign = match (cw, side) {
                (true, Side::Tail) | (false, Side::Head) => "",
                _ => "-",
            };
            let mut f = vec![format!("rotate=a='{sign}pow({et},2)*PI/3':c=black:enable='{}'", window(1.0))];
            // Phóng to theo để che các góc đen khi xoay.
            f.extend(zoom(&format!("1+1.3*{et}"), cx, cy));
            f
        }
        Motion::Shake => zoom(
            &format!("1+0.15*{et}"),
            &format!("(iw-ow)/2*(1+sin(t*53)*{et})"),
            &format!("(ih-oh)/2*(1+cos(t*41)*{et})"),
        ),
        Motion::Whip(horizontal) => {
            let (sx, sy) = if horizontal { ((w / 18).clamp(2, 1024), 1) } else { (1, (h / 18).clamp(2, 1024)) };
            vec![
                format!("avgblur=sizeX={}:sizeY={}:enable='{}'", (sx / 3).max(1), (sy / 3).max(1), window(0.9)),
                format!("avgblur=sizeX={sx}:sizeY={sy}:enable='{}'", window(0.5)),
            ]
        }
        Motion::Glitch => {
            let shift = (w / 28).max(2);
            vec![
                format!("rgbashift=rh={shift}:bh=-{shift}:rv=-{}:edge=wrap:enable='{}'", shift / 2, window(0.8)),
                format!("noise=alls=50:allf=t+u:enable='{}'", window(0.8)),
            ]
        }
        Motion::Flash => vec![format!("eq=brightness='0.9*pow({},1.5)':eval=frame", e("t"))],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_groups_known() {
        let mut seen = std::collections::HashSet::new();
        for t in ALL {
            assert!(seen.insert(t.id), "trùng id {}", t.id);
            assert!(GROUPS.contains(&t.group), "nhóm lạ {}", t.group);
        }
        assert!(ALL.len() >= 60);
    }

    #[test]
    fn old_ids_still_load() {
        for id in ["fade", "dissolve", "fade_white", "slide_left", "slide_right", "wipe_left", "circle_open", "zoom_in"] {
            let k: TransitionKind = serde_json::from_str(&format!("\"{id}\"")).unwrap();
            assert_eq!(k.id(), id);
        }
        assert!(serde_json::from_str::<TransitionKind>("\"bogus\"").is_err());
        assert_eq!(serde_json::to_string(&TransitionKind::from_id("spin").unwrap()).unwrap(), "\"spin\"");
    }

    #[test]
    fn motion_windows() {
        let f = motion_filters(Motion::ZoomThrough, Side::Tail, 4.0, 0.5, 1080, 1920, 30);
        assert!(f[0].contains("clip((t-3.5)/0.5,0,1)") && f[1].starts_with("crop=1080:1920"), "{f:?}");
        let f = motion_filters(Motion::Whip(true), Side::Head, 4.0, 0.5, 1080, 1920, 30);
        assert!(f[1].contains("lte(t,0.25)") && f[1].contains("sizeX=60"), "{f:?}");
        assert!(motion_filters(Motion::None, Side::Head, 4.0, 0.5, 1080, 1920, 30).is_empty());
    }
}
