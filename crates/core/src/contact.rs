//! Contact sheet: ghép nhiều thumbnail cảnh thành một ảnh lưới có đánh số.
//!
//! Claude không xem video. Thay vào đó, MCP server gửi cho Claude vài ảnh lưới này
//! (mỗi ảnh 24 cảnh) cùng bảng số liệu; số in trên ảnh khớp với số trong bảng.
//! Một ảnh lưới thay cho 24 ảnh rời nên tốn ít lượt dùng Claude hơn nhiều.

use crate::error::{CoreError, Result};
use image::{imageops, ImageBuffer, Rgb, RgbImage};

pub const CELL_W: u32 = 256;
pub const CELL_H: u32 = 144;
pub const COLS: u32 = 6;
pub const PER_SHEET: usize = 24;

/// Phông chữ số 3x5 điểm ảnh, mỗi hàng là 3 bit.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b010, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

/// Vẽ số `n` tại (x, y) với mỗi điểm ảnh của phông phóng to `k` lần, có nền tối.
fn draw_number(img: &mut RgbImage, x: u32, y: u32, n: u32, k: u32, color: Rgb<u8>) {
    let text = n.to_string();
    let w = text.len() as u32 * 4 * k + k;
    let h = 7 * k;
    for yy in y..(y + h).min(img.height()) {
        for xx in x..(x + w).min(img.width()) {
            img.put_pixel(xx, yy, Rgb([15, 17, 21]));
        }
    }
    for (ci, ch) in text.bytes().enumerate() {
        let glyph = DIGITS[(ch - b'0') as usize];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) != 0 {
                    for dy in 0..k {
                        for dx in 0..k {
                            let px = x + k + ci as u32 * 4 * k + col * k + dx;
                            let py = y + k + row as u32 * k + dy;
                            if px < img.width() && py < img.height() {
                                img.put_pixel(px, py, color);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Một ô trong lưới.
pub struct Cell<'a> {
    pub number: u32,
    pub thumb: Option<&'a str>,
    /// Viền đỏ cho cảnh bị loại, để Claude nhận ra ngay.
    pub dropped: bool,
}

/// Tạo ảnh lưới JPEG kích thước ô mặc định (16:9). Ảnh không mở được thì ô để trống (vẫn có số).
pub fn contact_sheet(cells: &[Cell]) -> Result<Vec<u8>> {
    contact_sheet_with(cells, CELL_W, CELL_H, COLS)
}

/// Như `contact_sheet` nhưng tự chọn kích thước ô và số cột (vd. ô dọc cho video 9:16).
pub fn contact_sheet_with(cells: &[Cell], cell_w: u32, cell_h: u32, cols: u32) -> Result<Vec<u8>> {
    let rows = (cells.len() as u32).div_ceil(cols).max(1);
    let gap = 4;
    let (w, h) = (cols * (cell_w + gap) + gap, rows * (cell_h + gap) + gap);
    let mut img: RgbImage = ImageBuffer::from_pixel(w, h, Rgb([30, 33, 40]));
    for (i, cell) in cells.iter().enumerate() {
        let (cx, cy) = (gap + (i as u32 % cols) * (cell_w + gap), gap + (i as u32 / cols) * (cell_h + gap));
        if let Some(t) = cell.thumb.and_then(|p| image::open(p).ok()) {
            // Giữ nguyên tỉ lệ (video dọc vẫn hiện đủ), căn giữa ô.
            let thumb = t.resize(cell_w, cell_h, imageops::FilterType::Triangle).to_rgb8();
            let (ox, oy) = (cx + (cell_w - thumb.width()) / 2, cy + (cell_h - thumb.height()) / 2);
            imageops::overlay(&mut img, &thumb, ox as i64, oy as i64);
        }
        if cell.dropped {
            let red = Rgb([220, 60, 60]);
            for t in 0..3 {
                for x in cx..cx + cell_w {
                    img.put_pixel(x, cy + t, red);
                    img.put_pixel(x, cy + cell_h - 1 - t, red);
                }
                for y in cy..cy + cell_h {
                    img.put_pixel(cx + t, y, red);
                    img.put_pixel(cx + cell_w - 1 - t, y, red);
                }
            }
        }
        draw_number(&mut img, cx + 4, cy + 4, cell.number, 4, Rgb([255, 214, 102]));
    }
    let mut out = std::io::Cursor::new(Vec::new());
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 78);
    enc.encode_image(&img).map_err(|e| CoreError::InvalidOp(format!("không tạo được ảnh lưới: {e}")))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tao_luoi() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.png");
        ImageBuffer::from_pixel(320, 180, Rgb([200u8, 100, 50])).save(&p).unwrap();
        let ps = p.to_string_lossy().to_string();
        let cells: Vec<Cell> = (1..=8).map(|n| Cell { number: n, thumb: if n % 2 == 0 { Some(ps.as_str()) } else { None }, dropped: n == 3 }).collect();
        let jpg = contact_sheet(&cells).unwrap();
        let img = image::load_from_memory(&jpg).unwrap();
        assert_eq!(img.width(), COLS * (CELL_W + 4) + 4);
        assert_eq!(img.height(), 2 * (CELL_H + 4) + 4);
    }
}
