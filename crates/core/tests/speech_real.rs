//! Test gói giọng nói từ đầu đến cuối bằng một whisper-cli GIẢ (script in ra JSON
//! đúng định dạng `-ojf` của whisper.cpp), vì máy test không có model Whisper.
//! Bỏ qua nếu không có ffmpeg hoặc python3. Chỉ chạy trên Linux/macOS.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use vlogcut_core::{ffmpeg, media, ops, render, speech, store};

const FAKE: &str = r#"#!/usr/bin/env python3
import sys, json
args = sys.argv[1:]
base = args[args.index("-of") + 1]
lang = args[args.index("-l") + 1]
assert "-ojf" in args
words = [(" Xin",0.2,0.5),(" chào",0.5,0.9),(",",0.9,0.9),(" ừm",1.4,1.8),(",",1.8,1.8),(" các",2.2,2.5),(" bạn",2.5,2.9),(".",2.9,2.9),
         (" Hôm",5.0,5.3),(" nay",5.3,5.6),(" đi",5.6,5.9),(" chơi",5.9,6.3),(" vãi",6.3,6.6),(" luôn",6.6,7.0),(".",7.0,7.0)]
toks = [{"text":"[_BEG_]","offsets":{"from":0,"to":0},"p":1.0}]
toks += [{"text":t,"offsets":{"from":int(a*1000),"to":int(b*1000)},"p":0.9} for t,a,b in words]
sys.stderr.write("whisper_print_progress_callback: progress =  50%\n")
out = {"result":{"language":lang},"transcription":[{"offsets":{"from":0,"to":7200},"text":"".join(t for t,_,_ in words),"tokens":toks}]}
open(base + ".json","w",encoding="utf-8").write(json.dumps(out, ensure_ascii=False))
sys.stderr.write("whisper_print_progress_callback: progress = 100%\n")
"#;

#[test]
fn nhan_dang_don_loi_phu_de() {
    if !ffmpeg::info().found || std::process::Command::new("python3").arg("--version").output().is_err() {
        eprintln!("bỏ qua: thiếu ffmpeg hoặc python3");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let cli = bin.join("whisper-cli");
    std::fs::write(&cli, FAKE).unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let model = bin.join("ggml-test.bin");
    std::fs::write(&model, b"x").unwrap();

    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    let mut c = ffmpeg::command(&ffmpeg::ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=640x360:r=30:d=8", "-f", "lavfi", "-i", "sine=f=300:d=8", "-shortest", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"]).arg(src.join("noi.mp4"));
    ffmpeg::run(c).unwrap();

    store::create(dir, "t").unwrap();
    let m = media::probe(&src.join("noi.mp4")).unwrap();
    let p = store::upsert_media(dir, vec![m]).unwrap();
    let m = p.media[0].clone();

    // ---- Nhận dạng ----
    let tools = speech::Tools { cli: Some(cli.to_string_lossy().into()), model: Some(model.to_string_lossy().into()), gpu: false };
    let mut last = 0.0;
    let t = speech::transcribe(dir, &m, &tools, &speech::TranscribeOptions::default(), &AtomicBool::new(false), &mut |x| last = x).unwrap();
    assert_eq!(last, 1.0);
    assert_eq!(t.words().count(), 11);
    assert!(speech::load_transcript(dir, &m.id).is_some());

    // ---- Timeline: 1 clip 0–8 s ----
    let add: Vec<ops::Op> = serde_json::from_str(&format!(r#"[{{"op":"set_settings","width":640,"height":360}},{{"op":"add_clip","media_id":"{}","id":"c1"}}]"#, m.id)).unwrap();
    store::commit(dir, &add, None).unwrap();
    let p = store::load(dir).unwrap();

    // ---- Dọn lời: bỏ "ừm", khoảng lặng 2.9–5.0, từ "vãi" ----
    let opts = speech::CleanupOptions { remove_words: vec!["vãi".into()], ..Default::default() };
    let plan = speech::cleanup_plan(dir, &p, &opts).unwrap();
    eprintln!("{}", plan.description);
    assert_eq!((plan.fillers, plan.pauses, plan.words), (1, 1, 1), "{:?}", (plan.fillers, plan.pauses, plan.words));
    let cleaned = ops::apply(&p, &plan.ops).unwrap();
    assert!((p.duration() - cleaned.duration() - plan.removed_secs).abs() < 0.05);
    assert!(cleaned.video.len() >= 3);
    // Đoạn chứa "ừm" (1.4–1.8 s nguồn) không còn trong clip nào.
    assert!(cleaned.video.iter().all(|c| !(c.src_in < 1.5 && c.src_out > 1.7)));
    store::commit(dir, &plan.ops, None).unwrap();

    // ---- Phụ đề karaoke trên timeline đã dọn ----
    let p = store::load(dir).unwrap();
    let (sub_ops, desc) = speech::subtitle_plan(dir, &p, &speech::SubtitleOptions::default()).unwrap();
    eprintln!("{desc}");
    let with_subs = ops::apply(&p, &sub_ops).unwrap();
    let subs: Vec<_> = with_subs.texts.iter().filter(|t| t.id.starts_with(speech::SUB_PREFIX)).collect();
    assert!(!subs.is_empty());
    assert!(subs.iter().all(|t| !t.text.contains("ừm") && !t.text.contains("vãi")), "phụ đề không có ừm/vãi: {:?}", subs.iter().map(|t| &t.text).collect::<Vec<_>>());
    assert!(subs.iter().all(|t| t.style.karaoke && !t.words.is_empty()));
    // Phụ đề nằm trong thời lượng video.
    assert!(subs.iter().all(|t| t.end <= with_subs.duration() + 0.01));
    let ass = render::build_ass(&with_subs);
    assert!(ass.contains("\\kf"));

    // Tạo lại phụ đề thì thay phụ đề cũ, không bị nhân đôi.
    store::commit(dir, &sub_ops, None).unwrap();
    let p2 = store::load(dir).unwrap();
    let (again, _) = speech::subtitle_plan(dir, &p2, &speech::SubtitleOptions::default()).unwrap();
    let after = ops::apply(&p2, &again).unwrap();
    assert_eq!(after.texts.len(), subs.len());

    // ---- Xuất video có phụ đề karaoke ----
    let out = dir.join("out.mp4");
    render::render(dir, &after, &render::RenderOptions { out_path: out.clone(), fast: true, encoder: None }, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert!(Path::new(&out).exists());
}
