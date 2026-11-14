//! Test review với nhạc có nhịp rõ và các điểm cắt cố tình lệch nhịp. Bỏ qua nếu không có ffmpeg.

use std::path::Path;
use vlogcut_core::{ffmpeg, media, ops::Op, review, store};

fn gen(dir: &Path, name: &str, args: &[&str]) {
    let mut c = ffmpeg::command(&ffmpeg::ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y"]).args(args).arg(dir.join(name));
    ffmpeg::run(c).expect("tạo media mẫu");
}

#[test]
fn review_bat_cat_lech_nhip() {
    if !ffmpeg::info().found {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    gen(&src, "a.mp4", &["-f", "lavfi", "-i", "testsrc2=s=640x360:r=30:d=10", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"]);
    // Trống 120 BPM: nhịp tại 0, 0.5, 1.0, ...
    gen(&src, "drums.wav", &["-f", "lavfi", "-i", "aevalsrc='0.8*sin(2*PI*60*t)*exp(-30*mod(t,0.5))':s=44100:d=12"]);
    store::create(dir, "t").unwrap();
    let items: Vec<_> = media::collect_files(&[src]).iter().map(|f| media::probe(f).unwrap()).collect();
    let p = store::upsert_media(dir, items).unwrap();
    let id = |n: &str| p.media.iter().find(|m| m.name == n).unwrap().id.clone();
    // Cắt ở 2.0 (đúng nhịp), 4.25 (lệch 0.25), 6.0 (đúng nhịp). Chữ quá ngắn để đọc.
    let json = format!(
        r#"[
        {{"op":"set_settings","width":640,"height":360}},
        {{"op":"add_clip","media_id":"{a}","src_in":0,"src_out":2.0,"id":"c1"}},
        {{"op":"add_clip","media_id":"{a}","src_in":2.0,"src_out":4.25,"id":"c2"}},
        {{"op":"add_clip","media_id":"{a}","src_in":4.25,"src_out":6.0,"id":"c3"}},
        {{"op":"add_clip","media_id":"{a}","src_in":6.0,"src_out":8.0,"id":"c4"}},
        {{"op":"add_text","text":"Chữ dài nhưng hiện quá nhanh","start":1,"end":1.6}},
        {{"op":"add_audio","media_id":"{d}","start":0,"src_in":0,"src_out":8,"role":"music","fade_out":1}}
    ]"#,
        a = id("a.mp4"),
        d = id("drums.wav")
    );
    let ops: Vec<Op> = serde_json::from_str(&json).unwrap();
    let c = store::commit(dir, &ops, None).unwrap();
    let r = review::review(dir, &c.project, None, 12).unwrap();
    eprintln!("{}", r.report);
    assert!(r.report.contains("2/3 cuts land"), "phải có 2/3 điểm cắt đúng nhịp");
    assert!(r.report.contains("cut before c3"), "phải chỉ ra điểm cắt lệch trước c3");
    assert!(r.report.contains("too short to read"));
    assert!(r.report.contains("reuse") == false);
    assert_eq!(r.sheets.len(), 1);
    assert!(r.draft.exists());
}
