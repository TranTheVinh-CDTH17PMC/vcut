//! Test chạy ffmpeg thật: tạo video mẫu, nhập, dựng timeline có đủ hiệu ứng rồi xuất.
//! Bỏ qua nếu máy không có ffmpeg.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use vlogcut_core::{ffmpeg, media, ops::Op, render, store};

fn ffmpeg_ok() -> bool {
    ffmpeg::info().found
}

fn gen(dir: &Path, name: &str, args: &[&str]) -> std::path::PathBuf {
    let out = dir.join(name);
    let mut c = ffmpeg::command(&ffmpeg::ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y"]).args(args).arg(&out);
    ffmpeg::run(c).expect("tạo media mẫu");
    out
}

#[test]
fn xuat_video_day_du_hieu_ung() {
    if !ffmpeg_ok() {
        eprintln!("bỏ qua: không có ffmpeg");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    gen(&src, "a.mp4", &["-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=5", "-f", "lavfi", "-i", "sine=f=440:d=5", "-shortest", "-c:v", "libx264", "-pix_fmt", "yuv420p"]);
    gen(&src, "b.mp4", &["-f", "lavfi", "-i", "testsrc=s=720x1280:r=25:d=4", "-c:v", "libx264", "-pix_fmt", "yuv420p"]);
    gen(&src, "c.png", &["-f", "lavfi", "-i", "color=c=orange:s=800x600", "-frames:v", "1"]);
    gen(&src, "music.mp3", &["-f", "lavfi", "-i", "sine=f=220:d=20"]);

    store::create(dir, "demo").unwrap();
    let files = media::collect_files(&[src.clone()]);
    assert_eq!(files.len(), 4);
    let mut items = Vec::new();
    for f in &files {
        let mut m = media::probe(f).unwrap();
        m.thumb = media::make_thumbnail(dir, &m).unwrap();
        m.proxy = media::make_proxy(dir, &m).unwrap();
        items.push(m);
    }
    let p = store::upsert_media(dir, items).unwrap();
    let id = |n: &str| p.media.iter().find(|m| m.name == n).unwrap().id.clone();
    let b = p.media.iter().find(|m| m.name == "b.mp4").unwrap();
    assert_eq!((b.width, b.height), (720, 1280));
    assert!(b.proxy.is_some());

    let json = format!(
        r#"[
        {{"op":"add_clip","media_id":"{a}","src_in":0.5,"src_out":4,"id":"c1"}},
        {{"op":"add_clip","media_id":"{b}","src_in":0,"src_out":3,"id":"c2"}},
        {{"op":"add_clip","media_id":"{c}","id":"c3"}},
        {{"op":"set_clip_props","clip_id":"c2","transition_in":{{"kind":"circle_open","duration":0.6}},"preset":"vintage","speed":1.5,"motion_blur":4}},
        {{"op":"set_clip_props","clip_id":"c3","transition_in":{{"kind":"fade","duration":0.5}},"opacity":0.8,"rotation":5}},
        {{"op":"set_keyframes","target":{{"kind":"clip","id":"c1"}},"property":"scale","keyframes":[{{"t":0,"value":1}},{{"t":3,"value":1.4,"easing":"ease_in_out"}}]}},
        {{"op":"set_keyframes","target":{{"kind":"clip","id":"c1"}},"property":"position","keyframes":[{{"t":0,"value":{{"x":0,"y":0}}}},{{"t":3,"value":{{"x":0.1,"y":-0.05}}}}]}},
        {{"op":"set_keyframes","target":{{"kind":"clip","id":"c1"}},"property":"brightness","keyframes":[{{"t":0,"value":-0.3}},{{"t":2,"value":0}}]}},
        {{"op":"set_keyframes","target":{{"kind":"clip","id":"c3"}},"property":"opacity","keyframes":[{{"t":0,"value":1}},{{"t":3,"value":0}}]}},
        {{"op":"add_text","text":"Đà Lạt ơi!","start":0.3,"end":2.5,"style":{{"anim_in":"pop"}}}},
        {{"op":"add_text","text":"Ngày 2","start":3,"end":5}},
        {{"op":"set_keyframes","target":{{"kind":"text","id":"t2"}},"property":"opacity","keyframes":[{{"t":0,"value":0}},{{"t":1,"value":1}}]}},
        {{"op":"add_audio","media_id":"{m}","start":0.5,"duck":true,"fade_in":1,"fade_out":1,"volume":0.6}}
    ]"#,
        a = id("a.mp4"),
        b = id("b.mp4"),
        c = id("c.png"),
        m = id("music.mp3")
    )
    .replace(r#""id":"t2"}},"property":"opacity""#, r#""id":"PLACEHOLDER"}},"property":"opacity""#);
    // Chữ thứ hai cần id cố định để gắn keyframe.
    let json = json.replace(r#""text":"Ngày 2","start":3,"end":5}"#, r#""text":"Ngày 2","start":3,"end":5,"id":"t2"}"#).replace("PLACEHOLDER", "t2");
    let ops: Vec<Op> = serde_json::from_str(&json).unwrap();
    let c = store::commit(dir, &ops, None).unwrap();
    let expected = c.project.duration();

    let out = dir.join("out.mp4");
    let mut last = 0.0;
    render::render(dir, &c.project, &render::RenderOptions { out_path: out.clone(), fast: true, encoder: None }, &AtomicBool::new(false), &mut |p| last = p).unwrap();
    assert_eq!(last, 1.0);
    let probed = media::probe(&out).unwrap();
    assert_eq!((probed.width, probed.height), (1080, 1920));
    assert!(probed.has_audio);
    assert!((probed.duration - expected).abs() < 0.15, "dài {} thay vì {}", probed.duration, expected);

    let prev = render::render_clip_preview(dir, &c.project, "c1", &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert!(prev.exists());
}

/// Xuất một video có đủ 10 kiểu chuyển cảnh "Động" (xoay, zoom xuyên, rung, lia, nhiễu,
/// chớp sáng). Đặt biến môi trường VLOGCUT_KEEP=<thư mục> để giữ lại file xem thử.
#[test]
fn xuat_chuyen_canh_dong() {
    if !ffmpeg_ok() {
        eprintln!("bỏ qua: không có ffmpeg");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    gen(&src, "a.mp4", &["-f", "lavfi", "-i", "testsrc=s=1280x720:r=30:d=20", "-c:v", "libx264", "-pix_fmt", "yuv420p"]);
    store::create(dir, "dong").unwrap();
    let mut m = media::probe(&src.join("a.mp4")).unwrap();
    m.thumb = media::make_thumbnail(dir, &m).unwrap();
    let p = store::upsert_media(dir, vec![m]).unwrap();
    let mid = p.media[0].id.clone();

    let kinds = ["zoom_through", "spin", "spin_ccw", "shake", "whip_left", "whip_right", "whip_up", "whip_down", "glitch", "flash"];
    let mut ops: Vec<serde_json::Value> = Vec::new();
    for i in 0..=kinds.len() {
        let s = i as f64 * 1.6;
        ops.push(serde_json::json!({"op":"add_clip","media_id":mid,"src_in":s,"src_out":s + 1.6,"id":format!("c{i}")}));
        if i > 0 {
            ops.push(serde_json::json!({"op":"set_clip_props","clip_id":format!("c{i}"),"transition_in":{"kind":kinds[i-1],"duration":0.5}}));
        }
    }
    let ops: Vec<Op> = serde_json::from_value(serde_json::Value::Array(ops)).unwrap();
    let c = store::commit(dir, &ops, None).unwrap();
    let plan = render::build_plan(&c.project).unwrap();
    assert!(plan.graph.contains("rotate=") && plan.graph.contains("avgblur=") && plan.graph.contains("rgbashift="));

    if let Ok(keep) = std::env::var("VLOGCUT_KEEP") {
        std::fs::write(Path::new(&keep).join("graph.txt"), &plan.graph).unwrap();
        std::fs::write(Path::new(&keep).join("inputs.txt"), format!("{:?}", plan.inputs)).unwrap();
    }
    let out = dir.join("dong.mp4");
    let t0 = std::time::Instant::now();
    render::render(dir, &c.project, &render::RenderOptions { out_path: out.clone(), fast: true, encoder: None }, &AtomicBool::new(false), &mut |_| {}).unwrap();
    eprintln!("render {:.1}s cho {:.1}s video", t0.elapsed().as_secs_f64(), c.project.duration());
    let probed = media::probe(&out).unwrap();
    assert!((probed.duration - c.project.duration()).abs() < 0.15, "dài {} thay vì {}", probed.duration, c.project.duration());
    if let Ok(keep) = std::env::var("VLOGCUT_KEEP") {
        std::fs::copy(&out, Path::new(&keep).join("dong.mp4")).unwrap();
    }
}
