use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotRole {
    Manager,
    Board,
    Worker(u32),
}

impl fmt::Display for SlotRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manager => write!(f, "manager"),
            Self::Board => write!(f, "board"),
            Self::Worker(idx) => write!(f, "worker:{}", idx),
        }
    }
}

impl FromStr for SlotRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower == "manager" {
            Ok(Self::Manager)
        } else if lower == "board" {
            Ok(Self::Board)
        } else if lower == "worker" {
            Ok(Self::Worker(0))
        } else if let Some(rest) = lower
            .strip_prefix("worker:")
            .or_else(|| lower.strip_prefix("worker/"))
            .or_else(|| lower.strip_prefix("worker-"))
            .or_else(|| lower.strip_prefix("worker_"))
        {
            let idx = rest
                .parse::<u32>()
                .map_err(|e| format!("invalid worker index in '{}': {}", s, e))?;
            Ok(Self::Worker(idx))
        } else if let Ok(idx) = lower.parse::<u32>() {
            Ok(Self::Worker(idx))
        } else {
            Err(format!("unknown slot role: '{}'", s))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabSlotLocation {
    pub tab_name: String,
    pub slot_index: u32,
    pub row: u32,
    pub col: u32,
    pub total_rows: u32,
    pub total_cols: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RectBounds {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutConfig {
    pub pane_max: u32,
    pub tab_max: u32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            pane_max: 2,
            tab_max: 8,
        }
    }
}

pub struct LayoutAllocator;

impl LayoutAllocator {
    pub fn allocate_slot(role: &SlotRole, config: &LayoutConfig) -> TabSlotLocation {
        match role {
            SlotRole::Manager => TabSlotLocation {
                tab_name: "main".to_string(),
                slot_index: 0,
                row: 0,
                col: 0,
                total_rows: 1,
                total_cols: 2,
            },
            SlotRole::Board => TabSlotLocation {
                tab_name: "main".to_string(),
                slot_index: 1,
                row: 0,
                col: 1,
                total_rows: 2,
                total_cols: 2,
            },
            SlotRole::Worker(idx) => {
                let pane_max = config.pane_max;
                if *idx < pane_max {
                    TabSlotLocation {
                        tab_name: "main".to_string(),
                        slot_index: 2 + idx,
                        row: *idx,
                        col: 1,
                        total_rows: pane_max.max(1),
                        total_cols: 2,
                    }
                } else {
                    let tab_max = config.tab_max.max(1);
                    let overflow_idx = idx - pane_max;
                    let tab_num = (overflow_idx / tab_max) + 1;
                    let slot_in_tab = overflow_idx % tab_max;
                    let col = slot_in_tab % 2;
                    let row = slot_in_tab / 2;
                    let total_rows = (tab_max + 1) / 2;

                    TabSlotLocation {
                        tab_name: format!("over-{}", tab_num),
                        slot_index: slot_in_tab,
                        row,
                        col,
                        total_rows: total_rows.max(1),
                        total_cols: 2,
                    }
                }
            }
        }
    }

    pub fn compute_rect_bounds(location: &TabSlotLocation) -> RectBounds {
        if location.tab_name == "main" {
            if location.col == 0 {
                // Manager occupies full height on the left half
                RectBounds {
                    x: 0.0,
                    y: 0.0,
                    w: 0.5,
                    h: 1.0,
                }
            } else if location.slot_index == 1 {
                // Board occupies top-right half
                RectBounds {
                    x: 0.5,
                    y: 0.0,
                    w: 0.5,
                    h: 0.5,
                }
            } else {
                // Workers on main tab stack vertically in the bottom-right quadrant
                let total_rows = location.total_rows.max(1) as f32;
                let worker_h = 0.5 / total_rows;
                let y = 0.5 + (location.row as f32 * worker_h);
                RectBounds {
                    x: 0.5,
                    y,
                    w: 0.5,
                    h: worker_h,
                }
            }
        } else {
            // Balanced 2-column grid covering full screen on overflow tabs
            let total_cols = location.total_cols.max(1) as f32;
            let total_rows = location.total_rows.max(1) as f32;
            let w = 1.0 / total_cols;
            let h = 1.0 / total_rows;
            let x = location.col as f32 * w;
            let y = location.row as f32 * h;
            RectBounds { x, y, w, h }
        }
    }
}
