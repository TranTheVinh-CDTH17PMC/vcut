//! In tempo và nhịp của một file nhạc: `cargo run -p vlogcut-core --example beats_cli -- bai.mp3`
fn main() {
    let path = std::env::args().nth(1).expect("cần đường dẫn file nhạc");
    let b = vlogcut_core::beats::detect(&path, 600.0).unwrap();
    println!("{:.1} BPM, tin cậy {:.2}, {} nhịp, nhịp đầu: {:?}", b.bpm, b.confidence, b.beats.len(), &b.beats[..b.beats.len().min(6)]);
}
