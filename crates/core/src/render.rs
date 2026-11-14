//! Xuất video: biến timeline thành một lệnh FFmpeg.
//!
//! Quy trình:
//! 1. `build_plan` dựng danh sách input + "filtergraph" (sơ đồ xử lý) dạng chữ.
//! 2. Ghi filtergraph và phụ đề (.ass) ra thư mục tạm.
//! 3. Chạy ffmpeg trong thư mục tạm đó, đọc tiến độ qua `-progress pipe:1`.
//!
//! Tách bước 1 riêng giúp test được mà không cần chạy ffmpeg.

use crate::error::{invalid, CoreError, Result};
use crate::ffmpeg::{self, command, ffmpeg_bin, Encoder};
use crate::keyframe::{ffmpeg_expr, fmt_num, Animatable, Vec2};
use crate::project::*;
use crate::store::data_dir;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

/// Kế hoạch render: các tham số input và filtergraph.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Mỗi phần tử là tham số của một input, vd. ["-ss","1.5","-t","3","-i","a.mp4"].
    pub inputs: Vec<Vec<String>>,
    pub graph: String,
    /// Nội dung file phụ đề .ass (nếu có chữ).
    pub ass: Option<String>,
    pub duration: f64,
    pub settings: Settings,
}

pub const ASS_FILE: &str = "subs.ass";

pub fn build_plan(p: &Project) -> Result<Plan> {
    if p.video.is_empty() {
        return Err(invalid("timeline chưa có clip video nào"));
    }
    let s = p.settings;
    let (w, h, fps) = (s.width, s.height, s.fps);
    let layout = p.video_layout();
    let mut inputs: Vec<Vec<String>> = Vec::new();
    let mut g = String::new();

    // ---- 1. Từng clip video: hình [v{i}] và tiếng [a{i}] ----
    for (i, clip) in p.video.iter().enumerate() {
        let media = p.media_by_id(&clip.media_id).ok_or_else(|| invalid(format!("clip {} trỏ tới media không tồn tại", clip.id)))?;
        let dur = layout[i].duration;
        let src_len = clip.src_out - clip.src_in;
        let k = inputs.len();
        if media.kind == MediaKind::Image {
            inputs.push(strs(&["-loop", "1", "-framerate", &fps.to_string(), "-t", &fmt_num(src_len), "-i", &media.path]));
        } else {
            inputs.push(strs(&["-ss", &fmt_num(clip.src_in), "-t", &fmt_num(src_len), "-i", &media.path]));
        }

        let mut chain: Vec<String> = Vec::new();
        chain.push(format!("setpts=(PTS-STARTPTS)/{}", fmt_num(clip.speed)));
        chain.push(format!("fps={fps}"));
        chain.push(format!("scale={w}:{h}:force_original_aspect_ratio=increase:flags=bicubic,crop={w}:{h},setsar=1"));
        let preset = clip.color.preset.ffmpeg();
        if !preset.is_empty() {
            chain.push(preset.to_string());
        }
        if let Some(eq) = eq_filter(&clip.color) {
            chain.push(eq);
        }
        chain.extend(transform_filters(&clip.transform, w, h, fps));
        if clip.motion_blur >= 2 {
            chain.push(format!("tmix=frames={}", clip.motion_blur));
        }
        // tpad + trim: đảm bảo clip dài đúng `dur`, kể cả khi file ngắn hơn chút vì làm tròn.
        // fps + settb ở cuối: xfade đòi hai đầu vào cùng tốc độ khung và timebase cố định.
        chain.push(format!(
            "format=yuv420p,tpad=stop_mode=clone:stop_duration=1,trim=duration={},setpts=PTS-STARTPTS,fps={fps},settb=AVTB",
            fmt_num(dur)
        ));
        writeln!(g, "[{k}:v]{}[v{i}];", chain.join(",")).unwrap();

        // Âm thanh của clip
        let mut achain: Vec<String> = Vec::new();
        let src = if media.kind == MediaKind::Video && media.has_audio {
            achain.push("asetpts=PTS-STARTPTS".into());
            achain.extend(atempo_chain(clip.speed));
            format!("[{k}:a]")
        } else {
            achain.push("anullsrc=r=48000:cl=stereo".into());
            String::new()
        };
        achain.push("aformat=sample_rates=48000:channel_layouts=stereo".into());
        if let Some(v) = volume_filter(&clip.volume) {
            achain.push(v);
        }
        achain.push(format!("apad,atrim=duration={},asetpts=PTS-STARTPTS", fmt_num(dur)));
        writeln!(g, "{src}{}[a{i}];", achain.join(",")).unwrap();
    }

    // ---- 2. Nối các clip: xfade khi có chuyển cảnh, concat khi không ----
    let (mut cv, mut ca) = ("v0".to_string(), "a0".to_string());
    let mut cur_len = layout[0].duration;
    for i in 1..p.video.len() {
        let tr = layout[i].transition;
        let (nv, na) = (format!("jv{i}"), format!("ja{i}"));
        if tr > 0.0 {
            let kind = p.video[i].transition_in.map(|t| t.kind).unwrap_or_default();
            writeln!(
                g,
                "[{cv}][v{i}]xfade=transition={}:duration={}:offset={}[{nv}];",
                kind.xfade_name(),
                fmt_num(tr),
                fmt_num(cur_len - tr)
            )
            .unwrap();
            writeln!(g, "[{ca}][a{i}]acrossfade=d={}[{na}];", fmt_num(tr)).unwrap();
            cur_len += layout[i].duration - tr;
        } else {
            writeln!(g, "[{cv}][{ca}][v{i}][a{i}]concat=n=2:v=1:a=1[c{nv}][{na}];").unwrap();
            writeln!(g, "[c{nv}]fps={fps},settb=AVTB[{nv}];").unwrap();
            cur_len += layout[i].duration;
        }
        cv = nv;
        ca = na;
    }
    let total = cur_len;

    // ---- 3. Chữ (phụ đề .ass) ----
    let ass = if p.texts.is_empty() { None } else { Some(build_ass(p)) };
    if ass.is_some() {
        writeln!(g, "[{cv}]ass=filename={ASS_FILE}[vtxt];").unwrap();
        cv = "vtxt".into();
    }
    writeln!(g, "[{cv}]null[vout];").unwrap();

    // ---- 4. Nhạc nền / giọng đọc ----
    let mut duck_labels = Vec::new();
    let mut plain_labels = Vec::new();
    for (j, a) in p.audio.iter().enumerate() {
        let media = p.media_by_id(&a.media_id).ok_or_else(|| invalid(format!("âm thanh {} trỏ tới media không tồn tại", a.id)))?;
        if a.start >= total {
            continue; // nằm ngoài video
        }
        let k = inputs.len();
        let len = a.duration();
        inputs.push(strs(&["-ss", &fmt_num(a.src_in), "-t", &fmt_num(len), "-i", &media.path]));
        let mut chain = vec!["asetpts=PTS-STARTPTS".to_string(), "aformat=sample_rates=48000:channel_layouts=stereo".to_string()];
        if let Some(v) = volume_filter(&a.volume) {
            chain.push(v);
        }
        if a.fade_in > 0.0 {
            chain.push(format!("afade=t=in:st=0:d={}", fmt_num(a.fade_in.min(len))));
        }
        if a.fade_out > 0.0 {
            let d = a.fade_out.min(len);
            chain.push(format!("afade=t=out:st={}:d={}", fmt_num(len - d), fmt_num(d)));
        }
        let ms = (a.start * 1000.0).round() as i64;
        if ms > 0 {
            chain.push(format!("adelay=delays={ms}|{ms}"));
        }
        let label = format!("m{j}");
        writeln!(g, "[{k}:a]{}[{label}];", chain.join(",")).unwrap();
        if a.duck { duck_labels.push(label) } else { plain_labels.push(label) }
    }

    let mut mix_inputs = Vec::new();
    if duck_labels.is_empty() {
        mix_inputs.push(ca.clone());
    } else {
        writeln!(g, "[{ca}]asplit=2[amain][asc];").unwrap();
        mix_label(&mut g, &duck_labels, "duckmix");
        // Tiếng của clip video điều khiển việc giảm nhạc nền.
        writeln!(g, "[duckmix][asc]sidechaincompress=threshold=0.03:ratio=8:attack=20:release=400[ducked];").unwrap();
        mix_inputs.push("amain".into());
        mix_inputs.push("ducked".into());
    }
    if !plain_labels.is_empty() {
        mix_label(&mut g, &plain_labels, "plainmix");
        mix_inputs.push("plainmix".into());
    }
    // loudnorm: đưa độ to về -14 LUFS (chuẩn mạng xã hội), đỉnh không quá -1.5 dB.
    let tail = format!("atrim=duration={},loudnorm=I=-14:TP=-1.5:LRA=11,aresample=48000", fmt_num(total));
    if mix_inputs.len() == 1 {
        writeln!(g, "[{}]{tail}[aout]", mix_inputs[0]).unwrap();
    } else {
        let ins: String = mix_inputs.iter().map(|l| format!("[{l}]")).collect();
        writeln!(g, "{ins}amix=inputs={}:duration=first:normalize=0,{tail}[aout]", mix_inputs.len()).unwrap();
    }

    Ok(Plan { inputs, graph: g, ass, duration: total, settings: s })
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn mix_label(g: &mut String, labels: &[String], out: &str) {
    if labels.len() == 1 {
        writeln!(g, "[{}]anull[{out}];", labels[0]).unwrap();
    } else {
        let ins: String = labels.iter().map(|l| format!("[{l}]")).collect();
        writeln!(g, "{ins}amix=inputs={}:duration=longest:normalize=0[{out}];", labels.len()).unwrap();
    }
}

/// `atempo` chỉ nhận 0.5..2 mỗi lần, nên tốc độ lớn hơn phải ghép nhiều lần.
fn atempo_chain(speed: f64) -> Vec<String> {
    let mut out = Vec::new();
    let mut s = speed;
    if (s - 1.0).abs() < 1e-6 {
        return out;
    }
    while s > 2.0 {
        out.push("atempo=2".into());
        s /= 2.0;
    }
    while s < 0.5 {
        out.push("atempo=0.5".into());
        s /= 0.5;
    }
    out.push(format!("atempo={}", fmt_num(s)));
    out
}

fn is_default(a: &Animatable<f64>, def: f64) -> bool {
    matches!(a, Animatable::Static(v) if (*v - def).abs() < 1e-9)
}

fn eq_filter(c: &ColorSettings) -> Option<String> {
    if is_default(&c.brightness, 0.0) && is_default(&c.contrast, 1.0) && is_default(&c.saturation, 1.0) {
        return None;
    }
    let animated = c.brightness.is_animated() || c.contrast.is_animated() || c.saturation.is_animated();
    Some(format!(
        "eq=brightness='{}':contrast='{}':saturation='{}':eval={}",
        ffmpeg_expr(&c.brightness, "t"),
        ffmpeg_expr(&c.contrast, "t"),
        ffmpeg_expr(&c.saturation, "t"),
        if animated { "frame" } else { "init" }
    ))
}

fn volume_filter(v: &Animatable<f64>) -> Option<String> {
    if is_default(v, 1.0) {
        return None;
    }
    Some(format!("volume='{}':eval={}", ffmpeg_expr(v, "t"), if v.is_animated() { "frame" } else { "once" }))
}

/// Tách Animatable<Vec2> thành hai Animatable<f64> (x và y) để dựng biểu thức.
fn split_xy(a: &Animatable<Vec2>) -> (Animatable<f64>, Animatable<f64>) {
    use crate::keyframe::Keyframe;
    match a {
        Animatable::Static(v) => (Animatable::Static(v.x), Animatable::Static(v.y)),
        Animatable::Keyframes(k) => (
            Animatable::Keyframes(k.iter().map(|k| Keyframe { t: k.t, value: k.value.x, easing: k.easing }).collect()),
            Animatable::Keyframes(k.iter().map(|k| Keyframe { t: k.t, value: k.value.y, easing: k.easing }).collect()),
        ),
    }
}

/// Filter cho vị trí, zoom, xoay, độ mờ.
fn transform_filters(t: &Transform, w: u32, h: u32, fps: u32) -> Vec<String> {
    let mut out = Vec::new();
    let pos_default = matches!(t.position, Animatable::Static(v) if v.x == 0.0 && v.y == 0.0);
    let scale_default = is_default(&t.scale, 1.0);
    let (px, py) = split_xy(&t.position);

    if !(pos_default && scale_default) {
        let static_small = matches!(t.scale, Animatable::Static(s) if s < 1.0) && !t.position.is_animated();
        if static_small {
            // Thu nhỏ: đặt hình lên nền đen.
            let Animatable::Static(s) = t.scale else { unreachable!() };
            out.push(format!("scale=trunc(iw*{0}/2)*2:trunc(ih*{0}/2)*2", fmt_num(s)));
            out.push(format!(
                "pad={w}:{h}:'max(0,min(ow-iw,(ow-iw)/2+({})*ow))':'max(0,min(oh-ih,(oh-ih)/2+({})*oh))':black",
                ffmpeg_expr(&px, "0"),
                ffmpeg_expr(&py, "0")
            ));
        } else {
            // Phóng to + lia: zoompan. Phóng to trước 2 lần để chuyển động mượt, đỡ rung.
            // Biến `it` = thời gian (giây) của khung đầu vào, tính từ đầu clip.
            // Vị trí dương = nội dung dời sang phải/xuống => khung cắt dời ngược lại.
            out.push(format!("scale={}:{}", w * 2, h * 2));
            out.push(format!(
                "zoompan=z='max(1,{z})':x='max(0,min(iw-iw/zoom,(iw-iw/zoom)/2-({x})*iw))':y='max(0,min(ih-ih/zoom,(ih-ih/zoom)/2-({y})*ih))':d=1:s={w}x{h}:fps={fps}",
                z = ffmpeg_expr(&t.scale, "it"),
                x = ffmpeg_expr(&px, "it"),
                y = ffmpeg_expr(&py, "it"),
            ));
        }
    }
    if !is_default(&t.rotation, 0.0) {
        out.push(format!("rotate=a='({})*PI/180':c=black:ow={w}:oh={h}", ffmpeg_expr(&t.rotation, "t")));
    }
    if !is_default(&t.opacity, 1.0) {
        // Độ mờ = trộn với nền đen.
        match &t.opacity {
            Animatable::Static(o) => {
                out.push(format!("format=gbrp,colorchannelmixer=rr={0}:gg={0}:bb={0}", fmt_num(*o)));
            }
            anim => {
                let e = ffmpeg_expr(anim, "T");
                out.push(format!("format=gbrp,geq=r='r(X,Y)*({e})':g='g(X,Y)*({e})':b='b(X,Y)*({e})'"));
            }
        }
    }
    out
}

// ---------- Phụ đề ASS ----------

fn ass_time(t: f64) -> String {
    let cs = (t.max(0.0) * 100.0).round() as u64;
    format!("{}:{:02}:{:02}.{:02}", cs / 360000, (cs / 6000) % 60, (cs / 100) % 60, cs % 100)
}

/// "#RRGGBB" -> "&HBBGGRR&" (ASS ghi màu ngược thứ tự).
fn ass_color(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return "&HFFFFFF&".into();
    }
    format!("&H{}{}{}&", &h[4..6], &h[2..4], &h[0..2]).to_uppercase()
}

fn ass_alpha(opacity: f64) -> String {
    let a = ((1.0 - opacity.clamp(0.0, 1.0)) * 255.0).round() as u8;
    format!("&H{a:02X}&")
}

fn ass_escape(text: &str) -> String {
    text.replace('\\', "/").replace('{', "(").replace('}', ")").replace("\r\n", "\\N").replace('\n', "\\N")
}

/// Chữ karaoke: mỗi từ kèm `\kf<centi-giây>` = thời gian đến từ kế tiếp.
/// Khoảng chờ trước từ đầu tiên dùng `\k` với chuỗi rỗng.
fn karaoke_text(t: &TextClip) -> String {
    let cs = |sec: f64| (sec.max(0.0) * 100.0).round() as i64;
    let mut out = String::new();
    let first = t.words[0].start;
    if first > 0.01 {
        out.push_str(&format!("{{\\k{}}}", cs(first)));
    }
    for (i, w) in t.words.iter().enumerate() {
        let next = t.words.get(i + 1).map(|n| n.start).unwrap_or(w.end);
        let dur = (next - w.start).max(w.end - w.start).max(0.05);
        let sep = if i + 1 < t.words.len() { " " } else { "" };
        out.push_str(&format!("{{\\kf{}}}{}{sep}", cs(dur), ass_escape(&w.text)));
    }
    out
}

pub fn build_ass(p: &Project) -> String {
    let s = p.settings;
    let mut out = String::new();
    writeln!(out, "[Script Info]\nScriptType: v4.00+\nPlayResX: {}\nPlayResY: {}\nWrapStyle: 0\nScaledBorderAndShadow: yes\n", s.width, s.height).unwrap();
    writeln!(out, "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding").unwrap();
    let margin = (s.width as f64 * 0.06).round();
    writeln!(out, "Style: Default,Arial,72,&H00FFFFFF,&H00FFFFFF,&H00000000,&H64000000,-1,0,0,0,100,100,0,0,1,4,0,5,{margin},{margin},40,1\n").unwrap();
    writeln!(out, "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text").unwrap();

    for t in &p.texts {
        let st = &t.style;
        let base = format!(
            "\\an5\\fn{}\\fs{}\\c{}\\3c{}\\bord{}\\b{}",
            st.font,
            fmt_num(st.size),
            ass_color(&st.color),
            ass_color(&st.outline_color),
            fmt_num(st.outline),
            if st.bold { 1 } else { 0 }
        );
        let text = if st.karaoke && !t.words.is_empty() { karaoke_text(t) } else { ass_escape(&t.text) };
        // Karaoke: \k tô từ màu phụ (\2c, chưa nói) sang màu chính (\c, đã nói).
        let base = if st.karaoke && !t.words.is_empty() {
            format!("{base}\\c{}\\2c{}", ass_color(&st.highlight_color), ass_color(&st.color))
        } else {
            base
        };
        let px = |v: Vec2| (v.x * s.width as f64, v.y * s.height as f64);
        let pos = t.transform.position.clone().unwrap_or(Animatable::Static(t.base_position()));
        let scale = t.transform.scale.clone().unwrap_or(Animatable::Static(1.0));
        let opacity = t.transform.opacity.clone().unwrap_or(Animatable::Static(1.0));
        let animated = pos.is_animated() || scale.is_animated() || opacity.is_animated();
        let len = t.end - t.start;

        if !animated {
            let (x, y) = px(pos.value_at(0.0));
            let (sc, op) = (scale.value_at(0.0) * 100.0, opacity.value_at(0.0));
            let common = format!("{base}\\fscx{0}\\fscy{0}\\alpha{1}", fmt_num(sc), ass_alpha(op));
            let tags = match st.anim_in {
                TextAnim::None => format!("{common}\\pos({},{})", fmt_num(x), fmt_num(y)),
                TextAnim::Fade => format!("{common}\\pos({},{})\\fad(250,150)", fmt_num(x), fmt_num(y)),
                TextAnim::Pop => format!(
                    "{base}\\alpha{a}\\pos({x},{y})\\fscx{s0}\\fscy{s0}\\t(0,200,\\fscx{s1}\\fscy{s1})",
                    a = ass_alpha(op),
                    x = fmt_num(x),
                    y = fmt_num(y),
                    s0 = fmt_num(sc * 0.6),
                    s1 = fmt_num(sc),
                ),
                TextAnim::SlideUp => format!(
                    "{common}\\move({},{},{},{},0,300)\\fad(200,150)",
                    fmt_num(x),
                    fmt_num(y + s.height as f64 * 0.04),
                    fmt_num(x),
                    fmt_num(y)
                ),
            };
            writeln!(out, "Dialogue: 0,{},{},Default,,0,0,0,,{{{tags}}}{text}", ass_time(t.start), ass_time(t.end)).unwrap();
            continue;
        }

        // Có keyframe: chia thành nhiều đoạn, mỗi đoạn chuyển động thẳng giữa hai mốc.
        let mut marks: Vec<f64> = vec![0.0, len];
        for a in [pos.times(), scale.times(), opacity.times()] {
            marks.extend(a.into_iter().filter(|x| *x > 0.0 && *x < len));
        }
        marks.sort_by(f64::total_cmp);
        marks.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        for seg in marks.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let (x1, y1) = px(pos.value_at(a));
            let (x2, y2) = px(pos.value_at(b));
            let (s1, s2) = (scale.value_at(a) * 100.0, scale.value_at(b) * 100.0);
            let (o1, o2) = (opacity.value_at(a), opacity.value_at(b));
            let ms = ((b - a) * 1000.0).round();
            let tags = format!(
                "{base}\\move({},{},{},{},0,{ms})\\fscx{s1}\\fscy{s1}\\alpha{}\\t(0,{ms},\\fscx{s2}\\fscy{s2}\\alpha{})",
                fmt_num(x1),
                fmt_num(y1),
                fmt_num(x2),
                fmt_num(y2),
                ass_alpha(o1),
                ass_alpha(o2),
                s1 = fmt_num(s1),
                s2 = fmt_num(s2),
            );
            writeln!(out, "Dialogue: 0,{},{},Default,,0,0,0,,{{{tags}}}{text}", ass_time(t.start + a), ass_time(t.start + b)).unwrap();
        }
    }
    out
}

// ---------- Chạy ffmpeg ----------

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub out_path: PathBuf,
    /// true: mã hoá nhanh (bản xem thử).
    pub fast: bool,
    /// Bỏ trống = tự chọn (GPU nếu có).
    pub encoder: Option<Encoder>,
}

/// Xuất video. `progress` nhận số 0..1. Đặt `cancel` = true để dừng.
///
/// `&mut dyn FnMut(f64)`: nhận một hàm bất kỳ (closure) để báo tiến độ, nhờ vậy
/// app có thể gửi sự kiện lên giao diện mà core không cần biết gì về Tauri.
pub fn render(dir: &Path, p: &Project, opts: &RenderOptions, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<()> {
    let plan = build_plan(p)?;
    let work = data_dir(dir, "renders").join(format!("tmp-{}", new_id("job")));
    std::fs::create_dir_all(&work)?;
    std::fs::write(work.join("graph.txt"), &plan.graph)?;
    if let Some(ass) = &plan.ass {
        std::fs::write(work.join(ASS_FILE), ass)?;
    }

    let enc = opts.encoder.unwrap_or_else(ffmpeg::encoder);
    let result = run_ffmpeg(&work, &plan, enc, opts, cancel, progress);
    let result = match result {
        // GPU lỗi thì thử lại bằng CPU một lần.
        Err(CoreError::Ffmpeg(_)) if enc == Encoder::Nvenc && opts.encoder.is_none() && !cancel.load(Ordering::Relaxed) => {
            run_ffmpeg(&work, &plan, Encoder::X264, opts, cancel, progress)
        }
        other => other,
    };
    if result.is_ok() {
        let _ = std::fs::remove_dir_all(&work);
    }
    result
}

fn run_ffmpeg(work: &Path, plan: &Plan, enc: Encoder, opts: &RenderOptions, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<()> {
    let mut c = command(&ffmpeg_bin());
    c.current_dir(work);
    c.args(["-hide_banner", "-y", "-nostats", "-progress", "pipe:1"]);
    for inp in &plan.inputs {
        c.args(inp);
    }
    // ffmpeg 7+ đọc tham số từ file bằng cú pháp "-/tên", bản cũ dùng -filter_complex_script.
    if ffmpeg::version_major() >= 7 {
        c.args(["-/filter_complex", "graph.txt"]);
    } else {
        c.args(["-filter_complex_script", "graph.txt"]);
    }
    c.args(["-map", "[vout]", "-map", "[aout]"]);
    c.args(if opts.fast { enc.fast_args() } else { enc.final_args() });
    c.args(["-pix_fmt", "yuv420p", "-r", &plan.settings.fps.to_string(), "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"]);
    c.arg(&opts.out_path);
    c.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = c.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CoreError::Ffmpeg("không tìm thấy ffmpeg. Hãy cài FFmpeg và thêm vào PATH.".into())
        } else {
            CoreError::Io(e)
        }
    })?;

    // Đọc stderr ở luồng riêng, nếu không bộ đệm đầy sẽ làm ffmpeg treo.
    let mut stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let stdout = child.stdout.take().unwrap();
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CoreError::Cancelled);
        }
        if let Some(v) = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms=")) {
            if let Ok(us) = v.trim().parse::<f64>() {
                if plan.duration > 0.0 {
                    progress((us / 1_000_000.0 / plan.duration).clamp(0.0, 1.0));
                }
            }
        }
    }
    let status = child.wait()?;
    let err = err_thread.join().unwrap_or_default();
    if cancel.load(Ordering::Relaxed) {
        return Err(CoreError::Cancelled);
    }
    if !status.success() {
        return Err(CoreError::Ffmpeg(ffmpeg::tail(&err, 15)));
    }
    progress(1.0);
    Ok(())
}

/// Render riêng một clip (độ phân giải thấp) để xem chính xác hiệu ứng nặng.
pub fn render_clip_preview(dir: &Path, p: &Project, clip_id: &str, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<PathBuf> {
    let i = p.video.iter().position(|c| c.id == clip_id).ok_or_else(|| invalid("không có clip này"))?;
    let span = p.video_layout()[i];
    let mut sub = p.clone();
    let mut clip = p.video[i].clone();
    clip.transition_in = None;
    sub.video = vec![clip];
    sub.audio.clear();

    // Thu nhỏ về cạnh ngắn ~540px cho nhanh.
    let s = p.settings;
    let f = (540.0 / s.width.min(s.height) as f64).min(1.0);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    sub.settings = Settings { width: even(s.width as f64 * f), height: even(s.height as f64 * f), fps: s.fps };
    sub.texts = p
        .texts
        .iter()
        .filter(|t| t.end > span.start && t.start < span.start + span.duration)
        .map(|t| {
            let mut t = t.clone();
            t.start = (t.start - span.start).max(0.0);
            t.end = (t.end - span.start).min(span.duration);
            t.style.size *= f;
            t.style.outline *= f;
            t
        })
        .filter(|t| t.end - t.start > 0.05)
        .collect();

    let out = data_dir(dir, "renders").join(format!("preview-{clip_id}.mp4"));
    let opts = RenderOptions { out_path: out.clone(), fast: true, encoder: None };
    render(dir, &sub, &opts, cancel, progress)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{apply, Op};

    fn project() -> Project {
        let mut p = Project::new("t");
        for (id, kind, dur) in [("m1", MediaKind::Video, 5.0), ("m2", MediaKind::Image, 0.0), ("m3", MediaKind::Audio, 20.0)] {
            p.media.push(MediaItem {
                id: id.into(),
                path: format!("{id}.x"),
                name: id.into(),
                kind,
                duration: dur,
                width: 1920,
                height: 1080,
                fps: 30.0,
                has_audio: kind != MediaKind::Image,
                created_at: None,
                gps: None,
                thumb: None,
                proxy: None,
                scenes: vec![],
            analysis_version: 0,
            modified_at: None,
            transcribed: false,
            });
        }
        p
    }

    #[test]
    fn plan_co_chuyen_canh_chu_nhac() {
        let ops: Vec<Op> = serde_json::from_str(r#"[
            {"op":"add_clip","media_id":"m1","src_in":0,"src_out":4,"id":"a"},
            {"op":"add_clip","media_id":"m2","id":"b"},
            {"op":"set_clip_props","clip_id":"b","transition_in":{"kind":"slide_left","duration":0.5},"preset":"cinematic","motion_blur":3},
            {"op":"set_keyframes","target":{"kind":"clip","id":"a"},"property":"scale","keyframes":[{"t":0,"value":1},{"t":4,"value":1.3}]},
            {"op":"add_text","text":"Đà Lạt {ơi}","start":0.5,"end":3},
            {"op":"add_audio","media_id":"m3","start":0,"duck":true,"fade_out":1}
        ]"#).unwrap();
        let p = apply(&project(), &ops).unwrap();
        let plan = build_plan(&p).unwrap();
        assert!((plan.duration - 6.5).abs() < 1e-9);
        assert_eq!(plan.inputs.len(), 3);
        assert!(plan.graph.contains("xfade=transition=slideleft:duration=0.5:offset=3.5"));
        assert!(plan.graph.contains("zoompan"));
        assert!(plan.graph.contains("tmix=frames=3"));
        assert!(plan.graph.contains("sidechaincompress"));
        let ass = plan.ass.unwrap();
        assert!(ass.contains("Đà Lạt (ơi)"));
    }

    #[test]
    fn mau_ass() {
        assert_eq!(ass_color("#FF8800"), "&H0088FF&");
        assert_eq!(ass_time(3661.5), "1:01:01.50");
    }
}
