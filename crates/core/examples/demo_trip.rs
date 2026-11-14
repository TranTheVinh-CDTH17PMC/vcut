//! Tạo một "chuyến đi" mẫu: nhiều clip qua 2 ngày, có cảnh mờ, tối, rung, trùng,
//! rồi nhập và phân tích như app làm. Dùng để thử màn hình Tự dựng.
//!
//! Chạy: `cargo run -p vlogcut-core --example demo_trip -- <thư mục>`

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use vlogcut_core::{analysis, ffmpeg, media, store};

fn gen(dir: &Path, name: &str, when: &str, args: &[&str]) {
    let mut c = ffmpeg::command(&ffmpeg::ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y"]).args(args);
    if !name.ends_with(".mp3") {
        c.args(["-c:v", "libx264", "-preset", "veryfast", "-pix_fmt", "yuv420p"]);
    }
    c.args(["-metadata", &format!("creation_time={when}")]).arg(dir.join(name));
    ffmpeg::run(c).expect("ffmpeg");
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("cần đường dẫn thư mục"));
    let src = dir.join("footage");
    std::fs::create_dir_all(&src).unwrap();
    let d1 = "2026-10-12T08:00:00Z";
    let d2 = "2026-10-13T06:00:00Z";
    gen(&src, "01_ho_xuan_huong.mp4", d1, &["-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=12"]);
    gen(&src, "02_banh_can.mp4", d1, &["-f", "lavfi", "-i", "smptehdbars=s=1280x720:r=30:d=6", "-f", "lavfi", "-i", "aevalsrc='0.3*sin(2*PI*220*t)*(0.5+0.5*sin(2*PI*1500*t))*gt(sin(2*PI*3.3*t)+0.3*sin(2*PI*7*t),0.2)':s=44100:d=6", "-shortest", "-c:a", "aac"]);
    gen(&src, "03_doc_doi_rung.mp4", d1, &["-f", "lavfi", "-i", "smptehdbars=s=1600x900:r=30:d=5", "-vf", "noise=alls=20:allf=t,crop=1280:720:'160+60*(random(1)-0.5)':'90+60*(random(2)-0.5)'"]);
    gen(&src, "04_cho_dem.mp4", d1, &["-f", "lavfi", "-i", "mandelbrot=s=1280x720:r=30", "-t", "7"]);
    gen(&src, "05_cho_dem_lan2.mp4", d1, &["-f", "lavfi", "-i", "mandelbrot=s=1280x720:r=30", "-t", "7", "-vf", "gblur=sigma=1.5"]);
    gen(&src, "06_quan_cafe_toi.mp4", d1, &["-f", "lavfi", "-i", "testsrc=s=1280x720:r=30:d=5", "-vf", "eq=brightness=-0.7"]);
    gen(&src, "07_san_may.mp4", d2, &["-f", "lavfi", "-i", "gradients=s=1280x720:r=30:d=10:speed=0.02:c0=0x6699cc:c1=0xffffff:c2=0x335577:n=3"]);
    gen(&src, "08_leo_nui_mo.mp4", d2, &["-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=6", "-vf", "gblur=sigma=12"]);
    gen(&src, "09_doi_thong.mp4", d2, &["-f", "lavfi", "-i", "life=s=320x180:mold=10:r=30:ratio=0.1:death_color=#1d3b2a:life_color=#6fcf7f", "-t", "6", "-vf", "scale=1280:720:flags=neighbor"]);
    gen(&src, "10_hoang_hon.mp4", d2, &["-f", "lavfi", "-i", "smptebars=s=1280x720:r=30:d=4", "-f", "lavfi", "-i", "gradients=s=1280x720:r=30:d=5:c0=orange:c1=purple:n=2", "-filter_complex", "[0][1]concat=n=2:v=1"]);
    gen(&src, "lofi_chill.mp3", "", &["-f", "lavfi", "-i", "sine=f=220:d=60"]);

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
    let todo: Vec<_> = p.media.iter().filter(|m| m.kind != vlogcut_core::project::MediaKind::Audio).cloned().collect();
    let lock = std::sync::Mutex::new(());
    analysis::analyze_all(&dir, &todo, 2, &AtomicBool::new(false), &|prog, m, r| {
        let _g = lock.lock().unwrap();
        store::set_scenes(&dir, &m.id, r.unwrap()).unwrap();
        println!("Phân tích {}/{} {}", prog.done, prog.total, prog.name);
    });
    println!("Xong: {}", dir.display());

    // Tham số thứ hai "build": dựng nhanh, áp dụng đề xuất và xuất video.
    if std::env::args().nth(2).as_deref() == Some("build") {
        let p = store::load(&dir).unwrap();
        let brief = store::Brief { target_secs: Some(30.0), ..Default::default() };
        let plan = vlogcut_core::autoedit::quick_plan(&p, &brief).unwrap();
        println!("{}", plan.description);
        let prop = store::propose(&dir, &plan.description, plan.ops).unwrap();
        for l in &prop.diff.lines {
            println!("  {l}");
        }
        let c = store::accept_proposal(&dir).unwrap();
        let out = dir.join("dung_nhanh.mp4");
        let opts = vlogcut_core::render::RenderOptions { out_path: out.clone(), fast: true, encoder: None };
        vlogcut_core::render::render(&dir, &c.project, &opts, &AtomicBool::new(false), &mut |_| {}).unwrap();
        println!("Đã xuất {}", out.display());
    }
}
