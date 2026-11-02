//! Color tokens and shared style strings for the redesigned screens.
#![allow(dead_code)]

pub const BG: &str = "#f7f7fc";
pub const HEADER_BG: &str = "#efeff3";
pub const SURFACE: &str = "#ffffff";
pub const TEXT: &str = "#1f2133";
pub const TEXT_MUTED: &str = "#8a8d9f";
pub const BORDER: &str = "#e6e6ef";
pub const PRIMARY: &str = "#3f51b5";
pub const PRIMARY_SOFT: &str = "#e2e5f7";
pub const POSITIVE: &str = "#17a589";
pub const NEGATIVE: &str = "#e5527f";
pub const NEGATIVE_SOFT: &str = "#fde3ea";
pub const POSITIVE_SOFT: &str = "#ddf3ee";
pub const MONTH_BG: &str = "#fcdcd8";
pub const MONTH_FG: &str = "#c0262d";
pub const CARD_RADIUS: &str = "16px";

/// White rounded card
pub const CARD: &str = "background:#fff; border-radius:16px; padding:14px 16px;";
/// Indigo section heading
pub const SECTION_TITLE: &str = "margin:0; font-size:17px; font-weight:700; color:#3f51b5;";

// The constants below mirror the color tokens above (consts cannot be interpolated into other consts)
pub const DANGER: &str = "#c0262d";
/// Form field label
pub const LABEL: &str = "font-size:13px; font-weight:600; color:#8a8d9f; margin:0 0 6px;";
pub const INPUT: &str = "width:100%; box-sizing:border-box; border:1px solid #e6e6ef; border-radius:12px; padding:10px 12px; font-size:15px; outline:none; background:#fff; color:#1f2133;";
pub const BTN_PRIMARY: &str = "border:none; border-radius:0; padding:12px 16px; background:#3f51b5; color:#fff; font-size:15px; font-weight:600; cursor:pointer;";
pub const BTN_SECONDARY: &str = "border:none; border-radius:0; padding:12px 16px; background:#e2e5f7; color:#3f51b5; font-size:15px; font-weight:600; cursor:pointer;";
pub const BTN_DANGER: &str = "border:none; border-radius:0; padding:12px 16px; background:#fde3ea; color:#c0262d; font-size:15px; font-weight:600; cursor:pointer;";
pub const CHIP: &str = "border:1px solid #e6e6ef; border-radius:0; padding:8px 14px; font-size:14px; background:transparent; color:#8a8d9f; cursor:pointer; white-space:nowrap;";
pub const CHIP_ACTIVE: &str = "border:1px solid #e2e5f7; border-radius:0; padding:8px 14px; font-size:14px; background:#e2e5f7; color:#3f51b5; font-weight:600; cursor:pointer; white-space:nowrap;";
/// Full-screen dim layer behind a dialog
pub const OVERLAY: &str = "position:fixed; inset:0; background:rgba(31,33,51,.45); display:flex; align-items:center; justify-content:center; z-index:100; padding:16px;";
pub const DIALOG: &str = "background:#fff; border-radius:20px; padding:22px; width:100%; max-width:340px; box-sizing:border-box;";
pub const ROW_DIVIDER: &str = "border-bottom:1px solid #e6e6ef;";
