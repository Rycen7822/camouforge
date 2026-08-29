//! 补充语义色：日志级别状态色（gpui-component 主题之外的少量自定义色）。

use gpui::Hsla;

const fn hcl(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla { h, s, l, a }
}

pub const GREEN: Hsla = hcl(145.0 / 360.0, 0.6, 0.45, 1.0);
pub const RED: Hsla = hcl(4.0 / 360.0, 0.75, 0.55, 1.0);
pub const YELLOW: Hsla = hcl(42.0 / 360.0, 0.85, 0.55, 1.0);

pub fn log_level_color(level: &str) -> Hsla {
    match level {
        "error" => RED,
        "warn" => YELLOW,
        _ => GREEN,
    }
}
