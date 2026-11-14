//! Filter màu dựng sẵn. Mỗi preset có:
//! - chuỗi filter FFmpeg (dùng khi xuất video, chính xác)
//! - chuỗi CSS filter (dùng xem trước trong app, gần đúng)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterPreset {
    #[default]
    None,
    Cinematic,
    Vintage,
    Cool,
    Warm,
    BlackWhite,
    Vivid,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetInfo {
    pub id: FilterPreset,
    pub label: &'static str,
    pub css: &'static str,
}

impl FilterPreset {
    pub const ALL: [FilterPreset; 7] = [
        FilterPreset::None,
        FilterPreset::Cinematic,
        FilterPreset::Vintage,
        FilterPreset::Cool,
        FilterPreset::Warm,
        FilterPreset::BlackWhite,
        FilterPreset::Vivid,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FilterPreset::None => "Gốc",
            FilterPreset::Cinematic => "Cinematic",
            FilterPreset::Vintage => "Vintage",
            FilterPreset::Cool => "Lạnh",
            FilterPreset::Warm => "Ấm",
            FilterPreset::BlackWhite => "Đen trắng",
            FilterPreset::Vivid => "Rực rỡ",
        }
    }

    /// Filter FFmpeg. `None` trả về chuỗi rỗng.
    pub fn ffmpeg(self) -> &'static str {
        match self {
            FilterPreset::None => "",
            FilterPreset::Cinematic => {
                "colorbalance=rs=-0.05:bs=0.07:rh=0.07:gh=0.02:bh=-0.05,eq=contrast=1.1:saturation=0.88"
            }
            FilterPreset::Vintage => "curves=preset=vintage,eq=saturation=0.85",
            FilterPreset::Cool => "colorbalance=rs=-0.08:bs=0.08:rm=-0.05:bm=0.05,eq=saturation=0.95",
            FilterPreset::Warm => "colorbalance=rs=0.08:bs=-0.08:rm=0.05:bm=-0.05,eq=saturation=1.05",
            FilterPreset::BlackWhite => "hue=s=0,eq=contrast=1.15",
            FilterPreset::Vivid => "eq=saturation=1.35:contrast=1.06",
        }
    }

    pub fn css(self) -> &'static str {
        match self {
            FilterPreset::None => "",
            FilterPreset::Cinematic => "contrast(1.1) saturate(0.88) hue-rotate(-6deg)",
            FilterPreset::Vintage => "sepia(0.35) saturate(0.85) contrast(0.95)",
            FilterPreset::Cool => "saturate(0.95) hue-rotate(12deg) brightness(1.02)",
            FilterPreset::Warm => "sepia(0.18) saturate(1.1) hue-rotate(-8deg)",
            FilterPreset::BlackWhite => "grayscale(1) contrast(1.15)",
            FilterPreset::Vivid => "saturate(1.35) contrast(1.06)",
        }
    }

    pub fn all_info() -> Vec<PresetInfo> {
        Self::ALL.iter().map(|p| PresetInfo { id: *p, label: p.label(), css: p.css() }).collect()
    }
}
