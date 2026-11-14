//! Test phân tích với video thật do ffmpeg tạo ra. Bỏ qua nếu không có ffmpeg.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use vlogcut_core::analysis;
use vlogcut_core::project::SceneTag;
use vlogcut_core::{ffmpeg, media, store};

fn gen(dir: &Path, name: &str, args: &[&str]) {
    let mut c = ffmpeg::command(&ffmpeg::ffmpeg_bin());
    c.args(["-hide_banner", "-loglevel", "error", "-y"]).args(args).arg(dir.join(name));
    ffmpeg::run(c).expect("tạo media mẫu");
}

#[test]
fn phan_tich_canh() {
    if !ffmpeg::info().found {
        eprintln!("bỏ qua: không có ffmpeg");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    fn with<'a>(a: &[&'a str]) -> Vec<&'a str> {
        [a, &["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"][..]].concat()
    }

    gen(&src, "net.mp4", &with(&["-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=4"]));
    gen(&src, "mo.mp4", &with(&["-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=4", "-vf", "gblur=sigma=14"]));
    gen(&src, "toi.mp4", &with(&["-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=4", "-vf", "eq=brightness=-0.75"]));
    // Rung: cắt khung với vị trí ngẫu nhiên mỗi khung hình.
    gen(&src, "rung.mp4", &with(&["-f", "lavfi", "-i", "smptehdbars=s=1600x900:r=30:d=4", "-vf", "noise=alls=20:allf=t,crop=1280:720:'160+60*(random(1)-0.5)':'90+60*(random(2)-0.5)'"]));
    // Trời/mây: ít chi tiết, chuyển động chậm. Không được coi là mờ hay rung.
    gen(&src, "troi.mp4", &with(&["-f", "lavfi", "-i", "gradients=s=1280x720:r=30:d=4:speed=0.02:c0=0x6699cc:c1=0xffffff:c2=0x335577:n=3"]));
    // Lia máy đều: có chuyển động nhưng không rung.
    gen(&src, "lia.mp4", &with(&["-f", "lavfi", "-i", "testsrc2=s=2560x720:r=30:d=4", "-vf", "crop=1280:720:'t*250':0"]));
    // Có điểm cắt ở giây thứ 3: ghép hai nguồn khác hẳn nhau.
    gen(&src, "cat.mp4", &with(&["-f", "lavfi", "-i", "smptebars=s=1280x720:r=30:d=3", "-f", "lavfi", "-i", "mandelbrot=s=1280x720:r=30", "-filter_complex", "[1]trim=duration=3,setpts=PTS-STARTPTS[b];[0][b]concat=n=2:v=1"]));
    // Bản trùng của net.mp4 (mã hoá lại, hơi mờ hơn chút).
    gen(&src, "net_ban2.mp4", &with(&["-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=4", "-vf", "gblur=sigma=1"]));
    // Video có tiếng "nói": âm 200 Hz bật tắt theo nhịp âm tiết + tiếng ồn nền.
    gen(&src, "noi.mp4", &[
        "-f", "lavfi", "-i", "color=c=0x406080:s=1280x720:r=30:d=5",
        "-f", "lavfi", "-i", "aevalsrc='0.3*sin(2*PI*220*t)*(0.5+0.5*sin(2*PI*1500*t))*gt(sin(2*PI*3.3*t)+0.3*sin(2*PI*7*t),0.2)+0.003*(random(0)-0.5)':s=44100:d=5",
        "-shortest", "-vf", "drawgrid=w=40:h=40:t=2:c=white", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac",
    ]);

    store::create(dir, "t").unwrap();
    let mut items = Vec::new();
    for f in media::collect_files(&[src]) {
        let mut m = media::probe(&f).unwrap();
        m.thumb = media::make_thumbnail(dir, &m).unwrap();
        items.push(m);
    }
    let p = store::upsert_media(dir, items).unwrap();
    assert!(p.media[0].modified_at.is_some());

    let results = Mutex::new(Vec::new());
    analysis::analyze_all(dir, &p.media, 2, &AtomicBool::new(false), &|_, m, r| {
        let mut m = m.clone();
        m.scenes = r.expect("phân tích lỗi");
        results.lock().unwrap().push(m);
    });
    let mut media = results.into_inner().unwrap();
    analysis::finalize(&mut media);

    let get = |n: &str| media.iter().find(|m| m.name == n).unwrap();
    let tags = |n: &str| get(n).scenes.iter().flat_map(|s| s.tags.clone()).collect::<Vec<_>>();
    for m in &media {
        for s in &m.scenes {
            eprintln!(
                "{:12} {:5.1}-{:5.1} shot{} sharp {:8.1} detail {:8.1} ratio {:5.2} bright {:5.1} motion {:4.2} shake {:4.2} db {:?} {:?} score {}",
                m.name, s.start, s.end, s.shot, s.sharpness.unwrap(), s.detail.unwrap(), s.sharpness.unwrap() / s.detail.unwrap().max(0.01), s.brightness.unwrap(), s.motion.unwrap(), s.shake.unwrap(), s.audio_db.map(|d| d.round()), s.tags, s.score
            );
        }
    }
    assert!(tags("net.mp4").contains(&SceneTag::Good));
    assert!(tags("mo.mp4").contains(&SceneTag::Blurry));
    assert!(tags("toi.mp4").contains(&SceneTag::Dark));
    assert!(tags("rung.mp4").contains(&SceneTag::Shaky), "rung.mp4 phải có nhãn Rung");
    assert!(!tags("net.mp4").contains(&SceneTag::Shaky));
    assert!(!tags("troi.mp4").contains(&SceneTag::Blurry), "trời không phải mờ");
    assert!(!tags("troi.mp4").contains(&SceneTag::Shaky), "trời không phải rung");
    assert!(!tags("lia.mp4").contains(&SceneTag::Shaky), "lia đều không phải rung");
    assert!(get("lia.mp4").scenes[0].motion.unwrap() > 1.0, "lia phải có chuyển động");
    assert_eq!(get("cat.mp4").scenes.iter().map(|s| s.shot).max(), Some(1), "cat.mp4 phải có 2 cảnh quay");
    let cut = get("cat.mp4").scenes.iter().find(|s| s.shot == 1).unwrap().start;
    assert!((cut - 3.0).abs() < 0.4, "điểm cắt ở {cut}");
    assert!(tags("net_ban2.mp4").contains(&SceneTag::Duplicate) || tags("net.mp4").contains(&SceneTag::Duplicate));
    assert!(tags("noi.mp4").contains(&SceneTag::Speech), "noi.mp4 phải có nhãn Có người nói");
    assert!(get("net.mp4").scenes.iter().all(|s| s.thumb.as_ref().map(|t| Path::new(t).exists()).unwrap_or(false)));
}
