use std::collections::HashMap;

/// A 24-bit RGB color.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A label chip's background and foreground colors.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LabelColor {
    pub background: Rgb,
    pub foreground: Rgb,
}

/// Derive a stable color for the label at first-seen `index`. The hue is spread
/// by the golden angle so nearby indices look distinct; the foreground shares
/// the background's hue but is lighter on a dark chip and darker on a light one.
pub fn create_label_color(index: usize) -> LabelColor {
    let hue = (index as f64 * GOLDEN_ANGLE) % 360.0;
    let saturation = 0.55;
    let background_lightness = BACKGROUND_LIGHTNESS[index % BACKGROUND_LIGHTNESS.len()];
    let background = hsl_to_rgb(hue, saturation, background_lightness);

    let foreground_lightness = if background_lightness < 0.5 {
        (background_lightness + 0.45).min(0.95)
    } else {
        (background_lightness - 0.45).max(0.05)
    };
    let foreground = hsl_to_rgb(hue, saturation, foreground_lightness);

    LabelColor {
        background,
        foreground,
    }
}

/// Render one label as an ANSI-colored chip with a space of padding on each
/// side of the text, ending with a reset.
pub fn render_label(label: &str, color: LabelColor) -> String {
    let fg = color.foreground;
    let bg = color.background;
    format!(
        "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m {label} \x1b[0m",
        fg.r, fg.g, fg.b, bg.r, bg.g, bg.b
    )
}

/// Render a note's labels as colored chips, showing at most `max_visible` and
/// appending `+N more` when some are hidden. `palette` keeps a repeated label's
/// color stable across notes in the same render.
pub fn render_labels(labels: &[String], max_visible: usize, palette: &mut LabelPalette) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let visible = labels.len().min(max_visible);
    let mut chips: Vec<String> = labels[..visible]
        .iter()
        .map(|label| render_label(label, palette.color_for(label)))
        .collect();
    let hidden = labels.len() - visible;
    if hidden > 0 {
        chips.push(format!("+{hidden} more"));
    }
    chips.join(" ")
}

/// Remembers which color each distinct label was assigned during a single
/// render, so a repeated label keeps its color. Never persisted between runs.
#[derive(Default)]
pub struct LabelPalette {
    assigned: HashMap<String, LabelColor>,
    next_index: usize,
}

impl LabelPalette {
    /// A palette with no assignments yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The color for `label`, assigning the next sequential color on first sight.
    pub fn color_for(&mut self, label: &str) -> LabelColor {
        if let Some(color) = self.assigned.get(label) {
            return *color;
        }
        let color = create_label_color(self.next_index);
        self.next_index += 1;
        self.assigned.insert(label.to_string(), color);
        color
    }
}

/// Convert an HSL color (hue in degrees `[0, 360)`, saturation and lightness in
/// `[0, 1]`) to 24-bit RGB.
fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> Rgb {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h_prime = hue / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c / 2.0;
    Rgb {
        r: ((r1 + m) * 255.0).round() as u8,
        g: ((g1 + m) * 255.0).round() as u8,
        b: ((b1 + m) * 255.0).round() as u8,
    }
}

/// The golden angle, in degrees — spreads hues so adjacent indices differ.
const GOLDEN_ANGLE: f64 = 137.507_764_05;

/// Background lightness cycled by index so chips vary between dark and light,
/// which in turn drives the foreground contrast direction.
const BACKGROUND_LIGHTNESS: [f64; 3] = [0.30, 0.70, 0.45];

#[cfg(test)]
mod tests {
    use super::*;

    fn brightness(color: Rgb) -> u32 {
        color.r as u32 + color.g as u32 + color.b as u32
    }

    #[test]
    fn hsl_primaries_convert_to_rgb() {
        assert_eq!(hsl_to_rgb(0.0, 1.0, 0.5), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(hsl_to_rgb(120.0, 1.0, 0.5), Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(hsl_to_rgb(240.0, 1.0, 0.5), Rgb { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn distinct_indices_get_distinct_backgrounds() {
        assert_ne!(
            create_label_color(0).background,
            create_label_color(1).background
        );
    }

    #[test]
    fn foreground_is_lighter_on_a_dark_background() {
        let color = create_label_color(0);
        assert!(
            brightness(color.foreground) > brightness(color.background),
            "expected a lighter foreground on a dark chip"
        );
    }

    #[test]
    fn foreground_is_darker_on_a_light_background() {
        let color = create_label_color(1);
        assert!(
            brightness(color.foreground) < brightness(color.background),
            "expected a darker foreground on a light chip"
        );
    }

    #[test]
    fn render_label_wraps_padded_text_in_color_codes() {
        let color = LabelColor {
            background: Rgb {
                r: 10,
                g: 20,
                b: 30,
            },
            foreground: Rgb {
                r: 200,
                g: 210,
                b: 220,
            },
        };
        let chip = render_label("feature", color);
        assert!(
            chip.contains(" feature "),
            "expected padded text in: {chip:?}"
        );
        assert!(
            chip.contains("\x1b[48;2;10;20;30m"),
            "missing background: {chip:?}"
        );
        assert!(
            chip.contains("\x1b[38;2;200;210;220m"),
            "missing foreground: {chip:?}"
        );
        assert!(chip.ends_with("\x1b[0m"), "missing reset: {chip:?}");
    }

    #[test]
    fn palette_reuses_color_for_repeated_label() {
        let mut palette = LabelPalette::new();
        let first = palette.color_for("backend");
        let _other = palette.color_for("frontend");
        let again = palette.color_for("backend");
        assert_eq!(first, again);
    }

    #[test]
    fn palette_assigns_colors_in_first_seen_order() {
        let mut palette = LabelPalette::new();
        assert_eq!(palette.color_for("a"), create_label_color(0));
        assert_eq!(palette.color_for("b"), create_label_color(1));
        assert_eq!(palette.color_for("a"), create_label_color(0));
    }

    #[test]
    fn render_labels_empty_is_blank() {
        let mut palette = LabelPalette::new();
        assert_eq!(render_labels(&[], 3, &mut palette), "");
    }

    #[test]
    fn render_labels_under_limit_shows_all_without_overflow() {
        let mut palette = LabelPalette::new();
        let labels = vec!["feature".to_string(), "backend".to_string()];
        let out = render_labels(&labels, 3, &mut palette);
        assert!(out.contains(" feature "), "missing feature in: {out:?}");
        assert!(out.contains(" backend "), "missing backend in: {out:?}");
        assert!(!out.contains("more"), "unexpected overflow in: {out:?}");
    }

    #[test]
    fn render_labels_over_limit_truncates_with_overflow_marker() {
        let mut palette = LabelPalette::new();
        let labels = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ];
        let out = render_labels(&labels, 3, &mut palette);
        assert!(out.contains(" a "), "missing first label in: {out:?}");
        assert!(
            out.contains("+2 more"),
            "missing overflow marker in: {out:?}"
        );
        assert!(!out.contains(" d "), "hidden label leaked in: {out:?}");
    }
}
