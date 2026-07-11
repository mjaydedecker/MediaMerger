use iced::Color;

pub struct Palette {
    pub card: Color,
    pub view: Color,
    pub body_bg: Color,
    pub border: Color,
    pub separator: Color,
    pub fg: Color,
    pub dim: Color,
    pub faint: Color,
    pub chip_bg: Color,
    pub chip_border: Color,
    pub btn_bg: Color,
    pub btn_hover: Color,
    pub accent: Color,
    pub accent_text: Color,
    pub accent_fg: Color,
    pub accent_soft: Color,
    pub success_fg: Color,
    pub success_soft: Color,
    pub warn_fg: Color,
    pub warn_soft: Color,
    pub danger_fg: Color,
    pub danger_soft: Color,
    pub wave: Color,
    pub headerbar: Color,
}

fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let h: String = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect()
    } else {
        h.to_string()
    };
    let n = u32::from_str_radix(&h, 16).unwrap_or(0x3584e4);
    (((n >> 16) & 255) as u8, ((n >> 8) & 255) as u8, (n & 255) as u8)
}

fn rgba(hex: &str, a: f32) -> Color {
    let (r, g, b) = hex_to_rgb(hex);
    Color::from_rgba8(r, g, b, a)
}

/// Ports the mockup's `shade(h, amt)`: amt < 0 darkens toward black by
/// |amt|; amt >= 0 lightens toward white by amt. Used to derive readable
/// accent-colored text against the theme's own background.
fn shade(hex: &str, amt: f32) -> Color {
    let (r, g, b) = hex_to_rgb(hex);
    let target: f32 = if amt < 0.0 { 0.0 } else { 255.0 };
    let p = amt.abs();
    let mix = |c: u8| -> u8 { (c as f32 + (target - c as f32) * p).round() as u8 };
    Color::from_rgb8(mix(r), mix(g), mix(b))
}

pub fn build(is_dark: bool, accent_hex: &str) -> Palette {
    if is_dark {
        Palette {
            card: rgba("#323232", 1.0),
            view: rgba("#1c1c1c", 1.0),
            body_bg: rgba("#242424", 1.0),
            border: rgba("#ffffff", 0.11),
            separator: rgba("#ffffff", 0.09),
            fg: rgba("#ffffff", 0.95),
            dim: rgba("#ffffff", 0.66),
            faint: rgba("#ffffff", 0.46),
            chip_bg: rgba("#ffffff", 0.09),
            chip_border: rgba("#ffffff", 0.11),
            btn_bg: rgba("#ffffff", 0.10),
            btn_hover: rgba("#ffffff", 0.17),
            accent: rgba(accent_hex, 1.0),
            accent_text: rgba("#ffffff", 1.0),
            accent_fg: shade(accent_hex, 0.42),
            accent_soft: rgba(accent_hex, 0.26),
            success_fg: rgba("#8ff0a4", 1.0),
            success_soft: rgba("#2ec27e", 0.20),
            warn_fg: rgba("#f8e45c", 1.0),
            warn_soft: rgba("#e5a50a", 0.20),
            danger_fg: rgba("#ff7b63", 1.0),
            danger_soft: rgba("#e01b24", 0.22),
            wave: rgba("#ffffff", 0.24),
            headerbar: rgba("#2e2e2e", 1.0),
        }
    } else {
        Palette {
            card: rgba("#ffffff", 1.0),
            view: rgba("#ffffff", 1.0),
            body_bg: rgba("#fafafb", 1.0),
            border: rgba("#000000", 0.09),
            separator: rgba("#000000", 0.07),
            fg: rgba("#000000", 0.87),
            dim: rgba("#000000", 0.55),
            faint: rgba("#000000", 0.40),
            chip_bg: rgba("#000000", 0.055),
            chip_border: rgba("#000000", 0.08),
            btn_bg: rgba("#000000", 0.06),
            btn_hover: rgba("#000000", 0.11),
            accent: rgba(accent_hex, 1.0),
            accent_text: rgba("#ffffff", 1.0),
            accent_fg: shade(accent_hex, -0.22),
            accent_soft: rgba(accent_hex, 0.13),
            success_fg: rgba("#1a7f4b", 1.0),
            success_soft: rgba("#2ec27e", 0.16),
            warn_fg: rgba("#9a5b00", 1.0),
            warn_soft: rgba("#e5a50a", 0.16),
            danger_fg: rgba("#c01c28", 1.0),
            danger_soft: rgba("#e01b24", 0.11),
            wave: rgba("#000000", 0.20),
            headerbar: rgba("#ffffff", 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_palette_matches_mockup_body_background() {
        let p = build(true, "#3584e4");
        assert_eq!(p.body_bg, Color::from_rgb8(0x24, 0x24, 0x24));
    }

    #[test]
    fn light_palette_matches_mockup_body_background() {
        let p = build(false, "#3584e4");
        assert_eq!(p.body_bg, Color::from_rgb8(0xfa, 0xfa, 0xfb));
    }

    #[test]
    fn accent_color_is_used_directly_for_the_accent_field() {
        let p = build(true, "#9141ac");
        assert_eq!(p.accent, Color::from_rgb8(0x91, 0x41, 0xac));
    }

    #[test]
    fn shade_lightens_toward_white_for_positive_amount() {
        let lightened = shade("#3584e4", 1.0);
        assert_eq!(lightened, Color::from_rgb8(0xff, 0xff, 0xff), "amt=1.0 should fully reach white");
    }

    #[test]
    fn shade_darkens_toward_black_for_negative_amount() {
        let darkened = shade("#3584e4", -1.0);
        assert_eq!(darkened, Color::from_rgb8(0x00, 0x00, 0x00), "amt=-1.0 should fully reach black");
    }

    #[test]
    fn dark_palette_headerbar_matches_mockup() {
        let p = build(true, "#3584e4");
        assert_eq!(p.headerbar, Color::from_rgb8(0x2e, 0x2e, 0x2e));
    }

    #[test]
    fn light_palette_headerbar_matches_mockup() {
        let p = build(false, "#3584e4");
        assert_eq!(p.headerbar, Color::from_rgb8(0xff, 0xff, 0xff));
    }
}
