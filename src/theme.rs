//! App-wide appearance tokens. Numeric preferences keep their original System/Light/Dark IDs.
use rune::Color;
use rune_kit::Kit;

pub struct Theme {
    pub name: &'static str,
    dark: Option<bool>,
    // Page, surface, text, secondary text, border, accent.
    colors: Option<[&'static str; 6]>,
}

// IDs are persisted: append new themes, never reorder existing entries.
pub const THEMES: &[Theme] = &[
    Theme {
        name: "System",
        dark: None,
        colors: None,
    },
    Theme {
        name: "Light",
        dark: Some(false),
        colors: None,
    },
    Theme {
        name: "Dark",
        dark: Some(true),
        colors: None,
    },
    Theme {
        name: "Tango",
        dark: Some(true),
        colors: Some([
            "#2e3436", "#252a2c", "#eeeeec", "#babdb6", "#555753", "#8cc4e8",
        ]),
    },
    Theme {
        name: "Solarized Light",
        dark: Some(false),
        colors: Some([
            "#fdf6e3", "#eee8d5", "#32474f", "#52676f", "#93a1a1", "#006b80",
        ]),
    },
    Theme {
        name: "Solarized Dark",
        dark: Some(true),
        colors: Some([
            "#002b36", "#073642", "#eee8d5", "#93a1a1", "#586e75", "#40b4aa",
        ]),
    },
    Theme {
        name: "Gruvbox Light",
        dark: Some(false),
        colors: Some([
            "#fbf1c7", "#f2e5bc", "#3c3836", "#665c54", "#bdae93", "#9d0006",
        ]),
    },
    Theme {
        name: "Gruvbox Dark",
        dark: Some(true),
        colors: Some([
            "#282828", "#1d2021", "#ebdbb2", "#bdae93", "#504945", "#fabd2f",
        ]),
    },
    Theme {
        name: "Dracula",
        dark: Some(true),
        colors: Some([
            "#282a36", "#21222c", "#f8f8f2", "#b5b6ce", "#6272a4", "#bd93f9",
        ]),
    },
    Theme {
        name: "Nord",
        dark: Some(true),
        colors: Some([
            "#2e3440", "#272c36", "#eceff4", "#b8c5d9", "#4c566a", "#88c0d0",
        ]),
    },
    Theme {
        name: "GitHub Light",
        dark: Some(false),
        colors: Some([
            "#ffffff", "#f6f8fa", "#1f2328", "#59636e", "#d1d9e0", "#9a6700",
        ]),
    },
    Theme {
        name: "GitHub Dark",
        dark: Some(true),
        colors: Some([
            "#0d1117", "#161b22", "#e6edf3", "#9198a1", "#30363d", "#e3b341",
        ]),
    },
];

pub fn get(id: u8) -> &'static Theme {
    THEMES.get(usize::from(id)).unwrap_or(&THEMES[0])
}

pub fn next(id: u8) -> u8 {
    ((usize::from(id) + 1) % THEMES.len()) as u8
}

pub fn kit(id: u8, system_dark: bool) -> Kit {
    let theme = get(id);
    let dark = theme.dark.unwrap_or(system_dark);
    let mut k = Kit::neutral(dark);
    let c = |s| rune::color::hex(s).expect("valid theme color");
    k.c.page = if dark { c("#202024") } else { Color::WHITE };
    k.c.layer_01 = if dark { c("#19191d") } else { c("#f5f5f8") };
    k.c.text_accent = if dark { c("#bca5ff") } else { c("#7050bb") };
    k.c.icon_accent = k.c.text_accent;
    k.c.border_focus = k.c.text_accent;
    k.c.bg_accent = k.c.text_accent;
    if let Some(colors) = theme.colors {
        let [page, surface, text, muted, border, accent] = colors.map(c);
        let p = &mut k.c;
        p.page = page;
        p.bg_base = surface;
        p.bg_deep = page;
        p.layer_01 = surface;
        p.layer_02 = surface;
        p.layer_03 = surface;
        p.layer_04 = border;
        p.bg_inverse = text;
        p.bg_contrast = text;
        p.bg_button_neutral = surface;
        p.bg_icon_button_contrast = accent;
        p.bg_accent = accent;
        p.submit_fg = if accent.luminance() > 0.179 {
            Color::BLACK
        } else {
            Color::WHITE
        };
        p.submit_icon = p.submit_fg;
        p.text_base = text;
        p.text_muted = muted;
        p.text_faint = muted;
        p.text_inverse = page;
        p.text_contrast = page;
        p.text_accent = accent;
        p.icon_base = text;
        p.icon_muted = muted;
        p.icon_faint = muted;
        p.icon_inverse = page;
        p.icon_contrast = page;
        p.icon_accent = accent;
        p.border_muted = border;
        p.border_base = border;
        p.border_strong = muted;
        p.border_focus = accent;
        p.hover = text.alpha(0.06);
        p.pressed = text.alpha(0.1);
        p.surface_base = surface;
        p.surface_weak = surface;
        p.surface_hover = p.hover;
        p.surface_raised = surface;
        p.surface_raised_base = surface;
        p.surface_raised_hover = p.hover;
        p.surface_float = text;
        p.surface_disabled = surface;
        p.border_weak = border;
        p.border_hover = muted;
        p.border_active = accent;
        p.border_disabled = border;
        p.text_strong = text;
        p.text_weak = muted;
        p.text_invert_strong = page;
        p.icon_weaker = muted;
        p.std_border = border;
        p.std_icon = muted;
        p.std_icon_invert = page;
        p.std_icon_strong = text;
        p.std_text = text;
        k.ring = Some(|p| p.border_base);
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(a: Color, b: Color) -> f32 {
        let (a, b) = (a.luminance(), b.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn only_system_appearance_follows_the_operating_system() {
        assert!(!kit(0, false).dark);
        assert!(kit(0, true).dark);
        for id in 1..12 {
            assert_eq!(kit(id, false).c.page, kit(id, true).c.page, "theme {id}");
            assert_eq!(kit(id, false).dark, kit(id, true).dark, "theme {id}");
        }
        assert_eq!(kit(255, true).c.page, kit(0, true).c.page);
    }

    #[test]
    fn preset_text_and_selected_rows_remain_readable() {
        for id in 3..12 {
            let k = kit(id, false);
            for bg in [k.c.page, k.c.layer_01, k.c.bg_base, k.c.layer_02] {
                for fg in [k.c.text_base, k.c.text_muted, k.c.text_accent] {
                    assert!(
                        contrast(fg, bg) >= 4.5,
                        "theme {id}: {} on {}",
                        fg.hex(),
                        bg.hex()
                    );
                }
            }
            // Rune's default window composites selection in sRGB, not linear light.
            let selection = k.selection();
            let blend = |bg, fg| {
                rune::color::srgb_decode(
                    rune::color::srgb_encode(bg) * (1.0 - selection.a)
                        + rune::color::srgb_encode(fg) * selection.a,
                )
            };
            for bg in [k.c.page, k.c.layer_01] {
                let selected = Color {
                    r: blend(bg.r, selection.r),
                    g: blend(bg.g, selection.g),
                    b: blend(bg.b, selection.b),
                    a: 1.0,
                };
                assert!(
                    contrast(k.c.text_base, selected) >= 4.5,
                    "theme {id} selection"
                );
            }
            assert!(
                contrast(k.c.submit_fg, k.c.bg_icon_button_contrast) >= 4.5,
                "theme {id} primary button"
            );
        }
    }

    #[test]
    fn github_selection_is_yellow_in_both_appearances() {
        for id in [10, 11] {
            let k = kit(id, false);
            let selection = k.selection();
            assert!(selection.r > selection.g && selection.g > selection.b * 2.0);
            assert!(selection.a > 0.0 && selection.a < 1.0);
        }
    }
}
