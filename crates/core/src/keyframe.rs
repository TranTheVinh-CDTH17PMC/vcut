//! Keyframe: giá trị thay đổi theo thời gian.
//!
//! Một thuộc tính (vd. `scale` của clip) hoặc là giá trị cố định, hoặc là danh
//! sách keyframe. `t` của keyframe tính bằng giây **từ đầu clip** trên timeline,
//! nên kéo clip đi chỗ khác không làm lệch chuyển động.

use serde::{Deserialize, Serialize};

/// Kiểu chuyển động giữa hai keyframe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Giữ nguyên giá trị đến keyframe sau rồi nhảy.
    Hold,
}

impl Easing {
    /// Biến tiến độ tuyến tính `u` (0..1) thành tiến độ có gia tốc.
    pub fn apply(self, u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Easing::Linear => u,
            Easing::EaseIn => u * u,
            Easing::EaseOut => u * (2.0 - u),
            Easing::EaseInOut => u * u * (3.0 - 2.0 * u),
            Easing::Hold => 0.0,
        }
    }

    /// Cùng công thức nhưng dạng biểu thức FFmpeg (biến `u`).
    pub fn ffmpeg_expr(self, u: &str) -> String {
        match self {
            Easing::Linear => u.to_string(),
            Easing::EaseIn => format!("({u})*({u})"),
            Easing::EaseOut => format!("({u})*(2-({u}))"),
            Easing::EaseInOut => format!("({u})*({u})*(3-2*({u}))"),
            Easing::Hold => "0".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe<T> {
    /// Giây tính từ đầu clip.
    pub t: f64,
    pub value: T,
    /// Kiểu chuyển động từ keyframe này tới keyframe kế tiếp.
    #[serde(default)]
    pub easing: Easing,
}

/// Điểm 2D, dùng cho vị trí.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

/// "Trait" giống interface: kiểu nào cài trait này thì nội suy được.
/// Nhờ vậy `Animatable<f64>` và `Animatable<Vec2>` dùng chung một code.
pub trait Lerp: Copy {
    fn lerp(a: Self, b: Self, u: f64) -> Self;
}

impl Lerp for f64 {
    fn lerp(a: f64, b: f64, u: f64) -> f64 {
        a + (b - a) * u
    }
}

impl Lerp for Vec2 {
    fn lerp(a: Vec2, b: Vec2, u: f64) -> Vec2 {
        Vec2 { x: f64::lerp(a.x, b.x, u), y: f64::lerp(a.y, b.y, u) }
    }
}

/// Giá trị cố định HOẶC danh sách keyframe.
///
/// `#[serde(untagged)]`: trong JSON, `1.0` là cố định, `[{"t":0,"value":1}]`
/// là keyframe. Không cần trường phân loại.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Animatable<T> {
    Static(T),
    Keyframes(Vec<Keyframe<T>>),
}

impl<T: Default> Default for Animatable<T> {
    fn default() -> Self {
        Animatable::Static(T::default())
    }
}

impl<T: Lerp> Animatable<T> {
    pub fn is_animated(&self) -> bool {
        matches!(self, Animatable::Keyframes(k) if !k.is_empty())
    }

    /// Giá trị tại thời điểm `t` (giây từ đầu clip).
    pub fn value_at(&self, t: f64) -> T {
        match self {
            Animatable::Static(v) => *v,
            Animatable::Keyframes(kfs) => value_at(kfs, t),
        }
    }

    /// Các mốc thời gian keyframe (để hiện chấm trên timeline).
    pub fn times(&self) -> Vec<f64> {
        match self {
            Animatable::Static(_) => vec![],
            Animatable::Keyframes(k) => k.iter().map(|k| k.t).collect(),
        }
    }

    /// Dời mọi keyframe một khoảng `dt` (dùng khi cắt đầu clip).
    pub fn shift(&mut self, dt: f64) {
        if let Animatable::Keyframes(kfs) = self {
            for k in kfs.iter_mut() {
                k.t += dt;
            }
        }
    }

    /// Tách làm hai tại `at`: phần trước giữ nguyên thời gian, phần sau bắt đầu
    /// từ 0. Chèn keyframe nội suy ở điểm cắt để chuyển động liền mạch.
    pub fn split(&self, at: f64) -> (Animatable<T>, Animatable<T>) {
        match self {
            Animatable::Static(v) => (Animatable::Static(*v), Animatable::Static(*v)),
            Animatable::Keyframes(kfs) => {
                let mid = value_at(kfs, at);
                // Easing tại điểm cắt lấy theo đoạn đang chứa điểm cắt.
                let easing = kfs.iter().rev().find(|k| k.t <= at).map(|k| k.easing).unwrap_or_default();
                let mut left: Vec<Keyframe<T>> = kfs.iter().filter(|k| k.t < at).cloned().collect();
                left.push(Keyframe { t: at, value: mid, easing });
                let mut right = vec![Keyframe { t: 0.0, value: mid, easing }];
                right.extend(kfs.iter().filter(|k| k.t > at).map(|k| Keyframe { t: k.t - at, ..k.clone() }));
                (Animatable::Keyframes(left), Animatable::Keyframes(right))
            }
        }
    }
}

/// Nội suy trên danh sách keyframe (giả định đã sắp xếp theo `t`).
pub fn value_at<T: Lerp>(kfs: &[Keyframe<T>], t: f64) -> T {
    // `match` trên slice: rỗng, hoặc lấy phần tử đầu/cuối.
    let (first, last) = match (kfs.first(), kfs.last()) {
        (Some(f), Some(l)) => (f, l),
        _ => panic!("value_at gọi với danh sách keyframe rỗng"),
    };
    if t <= first.t {
        return first.value;
    }
    if t >= last.t {
        return last.value;
    }
    // `windows(2)` duyệt từng cặp keyframe liền nhau.
    for pair in kfs.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t >= a.t && t <= b.t {
            let span = b.t - a.t;
            if span <= f64::EPSILON {
                return b.value;
            }
            let u = a.easing.apply((t - a.t) / span);
            return T::lerp(a.value, b.value, u);
        }
    }
    last.value
}

/// Sắp xếp keyframe theo thời gian và kiểm tra hợp lệ.
pub fn normalize<T>(kfs: &mut Vec<Keyframe<T>>) -> Result<(), String> {
    if kfs.is_empty() {
        return Err("danh sách keyframe rỗng".into());
    }
    if kfs.iter().any(|k| !k.t.is_finite() || k.t < 0.0) {
        return Err("thời điểm keyframe phải >= 0".into());
    }
    kfs.sort_by(|a, b| a.t.total_cmp(&b.t));
    Ok(())
}

/// Biểu thức FFmpeg cho một thuộc tính số theo biến thời gian `var`
/// (vd. `t` trong filter eq, `it` trong zoompan).
///
/// Kết quả dạng `if(lt(t,1),A,if(lt(t,2),B,C))`. Không chứa dấu nháy đơn,
/// người gọi tự bọc trong '...' khi đưa vào filtergraph.
pub fn ffmpeg_expr(anim: &Animatable<f64>, var: &str) -> String {
    match anim {
        Animatable::Static(v) => fmt_num(*v),
        Animatable::Keyframes(kfs) if kfs.is_empty() => "0".into(),
        Animatable::Keyframes(kfs) => {
            let last = kfs.last().unwrap();
            // Dựng từ cuối về đầu: biểu thức lồng nhau.
            let mut expr = fmt_num(last.value);
            for pair in kfs.windows(2).rev() {
                let (a, b) = (&pair[0], &pair[1]);
                let span = (b.t - a.t).max(1e-6);
                let u = format!("({var}-{})/{}", fmt_num(a.t), fmt_num(span));
                let eased = a.easing.ffmpeg_expr(&u);
                let seg = format!("{}+({})*({})", fmt_num(a.value), fmt_num(b.value - a.value), eased);
                expr = format!("if(lt({var},{}),{seg},{expr})", fmt_num(b.t));
            }
            let first = &kfs[0];
            format!("if(lt({var},{}),{},{expr})", fmt_num(first.t), fmt_num(first.value))
        }
    }
}

/// In số gọn, không có dạng 1e-7 mà FFmpeg không hiểu.
pub fn fmt_num(v: f64) -> String {
    let s = format!("{:.6}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kf(t: f64, v: f64) -> Keyframe<f64> {
        Keyframe { t, value: v, easing: Easing::Linear }
    }

    #[test]
    fn noi_suy_tuyen_tinh() {
        let a = Animatable::Keyframes(vec![kf(0.0, 1.0), kf(2.0, 2.0)]);
        assert_eq!(a.value_at(-1.0), 1.0);
        assert_eq!(a.value_at(1.0), 1.5);
        assert_eq!(a.value_at(5.0), 2.0);
    }

    #[test]
    fn hold_giu_gia_tri() {
        let a = Animatable::Keyframes(vec![
            Keyframe { t: 0.0, value: 1.0, easing: Easing::Hold },
            kf(2.0, 3.0),
        ]);
        assert_eq!(a.value_at(1.9), 1.0);
        assert_eq!(a.value_at(2.0), 3.0);
    }

    #[test]
    fn tach_lien_mach() {
        let a = Animatable::Keyframes(vec![kf(0.0, 0.0), kf(4.0, 4.0)]);
        let (l, r) = a.split(1.0);
        assert_eq!(l.value_at(1.0), 1.0);
        assert_eq!(r.value_at(0.0), 1.0);
        assert_eq!(r.value_at(3.0), 4.0);
    }

    #[test]
    fn json_hai_dang() {
        let s: Animatable<f64> = serde_json::from_str("1.5").unwrap();
        assert_eq!(s, Animatable::Static(1.5));
        let k: Animatable<f64> = serde_json::from_str(r#"[{"t":0,"value":1},{"t":1,"value":2,"easing":"ease_out"}]"#).unwrap();
        assert!(k.is_animated());
    }

    #[test]
    fn bieu_thuc_ffmpeg() {
        let a = Animatable::Keyframes(vec![kf(1.0, 1.0), kf(3.0, 2.0)]);
        let e = ffmpeg_expr(&a, "t");
        assert_eq!(e, "if(lt(t,1),1,if(lt(t,3),1+(1)*((t-1)/2),2))");
        assert_eq!(ffmpeg_expr(&Animatable::Static(0.25), "t"), "0.25");
    }
}
