//! Chạy review cho một project: `cargo run -p vlogcut-core --example review_cli -- <thư mục project>`
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("cần thư mục project"));
    let p = vlogcut_core::store::load(&dir).unwrap();
    let brief = vlogcut_core::store::load_brief(&dir);
    let r = vlogcut_core::review::review(&dir, &p, Some(&brief), 48).unwrap();
    println!("{}", r.report);
    for (i, s) in r.sheets.iter().enumerate() {
        let out = dir.join(format!("review-{}.jpg", i + 1));
        std::fs::write(&out, s).unwrap();
        println!("ảnh lưới: {}", out.display());
    }
}
