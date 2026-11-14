//! Tạo một project mẫu có sẵn media và timeline để thử giao diện.
//!
//! Chạy: `cargo run -p vlogcut-core --example demo_project -- <thư mục>`
//! Cần ffmpeg trong PATH (dùng để tạo video mẫu).

use std::path::PathBuf;
use vlogcut_core::{ffmpeg, media, ops::Op, store};

fn gen(dir: &std::path::Path, name: &str, args: &[&str]) {
    let mut c = ffmpeg::command(&ffmpeg::ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y"]).args(args).arg(dir.join(name));
    ffmpeg::run(c).expect("ffmpeg");
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("cần đường dẫn thư mục"));
    let src = dir.join("footage");
    std::fs::create_dir_all(&src).unwrap();
    gen(&src, "ho_xuan_huong.mp4", &["-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=8", "-f", "lavfi", "-i", "sine=f=330:d=8", "-shortest", "-c:v", "libx264", "-pix_fmt", "yuv420p"]);
    gen(&src, "san_may.mp4", &["-f", "lavfi", "-i", "mandelbrot=s=720x1280:r=30", "-t", "6", "-c:v", "libx264", "-pix_fmt", "yuv420p"]);
    gen(&src, "cho_dem.mp4", &["-f", "lavfi", "-i", "life=s=640x360:mold=10:r=30:ratio=0.1:death_color=#C83232:life_color=#00ff00", "-t", "5", "-vf", "scale=1280:720", "-c:v", "libx264", "-pix_fmt", "yuv420p"]);
    gen(&src, "hoang_hon.jpg", &["-f", "lavfi", "-i", "gradients=s=1200x800:c0=orange:c1=purple:n=2", "-frames:v", "1"]);
    gen(&src, "lofi_chill.mp3", &["-f", "lavfi", "-i", "sine=f=220:d=30"]);

    if !dir.join(store::PROJECT_FILE).exists() {
        store::create(&dir, "Đà Lạt 2026").unwrap();
    }
    let mut items = Vec::new();
    for f in media::collect_files(&[src]) {
        let mut m = media::probe(&f).unwrap();
        m.thumb = media::make_thumbnail(&dir, &m).unwrap();
        m.proxy = media::make_proxy(&dir, &m).unwrap();
        items.push(m);
    }
    let p = store::upsert_media(&dir, items).unwrap();
    let id = |n: &str| p.media.iter().find(|m| m.name == n).unwrap().id.clone();
    let json = format!(
        r#"[
        {{"op":"add_clip","media_id":"{a}","src_in":0,"src_out":6,"id":"c1"}},
        {{"op":"add_clip","media_id":"{b}","src_in":0,"src_out":5,"id":"c2"}},
        {{"op":"add_clip","media_id":"{c}","src_in":0,"src_out":5,"id":"c3"}},
        {{"op":"add_clip","media_id":"{d}","id":"c4"}},
        {{"op":"set_clip_props","clip_id":"c2","transition_in":{{"kind":"fade","duration":0.5}},"preset":"cinematic"}},
        {{"op":"set_clip_props","clip_id":"c3","motion_blur":4,"preset":"warm"}},
        {{"op":"set_keyframes","target":{{"kind":"clip","id":"c4"}},"property":"scale","keyframes":[{{"t":0,"value":1}},{{"t":3,"value":1.2}}]}},
        {{"op":"add_text","text":"Đà Lạt ơi!","start":0.5,"end":4,"style":{{"anim_in":"pop","size":110}}}},
        {{"op":"add_text","text":"Ngày 2 · Săn mây","start":6,"end":10}},
        {{"op":"add_audio","media_id":"{m}","start":0,"duck":true,"fade_out":2,"volume":0.7}}
    ]"#,
        a = id("ho_xuan_huong.mp4"),
        b = id("san_may.mp4"),
        c = id("cho_dem.mp4"),
        d = id("hoang_hon.jpg"),
        m = id("lofi_chill.mp3")
    );
    let ops: Vec<Op> = serde_json::from_str(&json).unwrap();
    let c = store::commit(&dir, &ops, None).unwrap();
    println!("Đã tạo project mẫu tại {}", dir.display());

    // Tham số thứ hai "render": xuất luôn video để kiểm tra.
    if std::env::args().nth(2).as_deref() == Some("render") {
        let out = dir.join("demo.mp4");
        let opts = vlogcut_core::render::RenderOptions { out_path: out.clone(), fast: true, encoder: None };
        vlogcut_core::render::render(&dir, &c.project, &opts, &std::sync::atomic::AtomicBool::new(false), &mut |_| {}).unwrap();
        println!("Đã xuất {}", out.display());
    }
}
