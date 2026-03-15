use ratatui::style::Color;

pub const THEME_NAMES: &[&str] = &["ember", "nord", "dracula", "gruvbox", "catppuccin", "mono"];

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub accent: Color,       // burn rate, primary highlights, selected shortcut letter
    pub secondary: Color,    // input tokens, cache
    pub tertiary: Color,     // output tokens, cost numbers
    pub muted: Color,        // borders, dim labels
    pub danger: Color,       // over budget
    pub success: Color,      // under budget
    pub text: Color,         // primary text
    pub text_dim: Color,     // secondary text
    pub bar_filled: Color,   // progress bar fill
    pub bar_empty: Color,    // progress bar background
}

pub fn get_theme(name: &str) -> Theme {
    match name {
        "nord" => Theme {
            name: "nord",
            accent: Color::Rgb(136, 192, 208),    // nord8
            secondary: Color::Rgb(143, 188, 187),  // nord7
            tertiary: Color::Rgb(235, 203, 139),   // nord13
            muted: Color::Rgb(76, 86, 106),        // nord3
            danger: Color::Rgb(191, 97, 106),      // nord11
            success: Color::Rgb(163, 190, 140),    // nord14
            text: Color::Rgb(236, 239, 244),       // nord6
            text_dim: Color::Rgb(129, 161, 193),   // nord9
            bar_filled: Color::Rgb(136, 192, 208),
            bar_empty: Color::Rgb(59, 66, 82),
        },
        "dracula" => Theme {
            name: "dracula",
            accent: Color::Rgb(255, 121, 198),     // pink
            secondary: Color::Rgb(139, 233, 253),  // cyan
            tertiary: Color::Rgb(241, 250, 140),   // yellow
            muted: Color::Rgb(98, 114, 164),       // comment
            danger: Color::Rgb(255, 85, 85),       // red
            success: Color::Rgb(80, 250, 123),     // green
            text: Color::Rgb(248, 248, 242),       // foreground
            text_dim: Color::Rgb(98, 114, 164),
            bar_filled: Color::Rgb(189, 147, 249), // purple
            bar_empty: Color::Rgb(68, 71, 90),
        },
        "gruvbox" => Theme {
            name: "gruvbox",
            accent: Color::Rgb(254, 128, 25),      // orange
            secondary: Color::Rgb(131, 165, 152),  // aqua
            tertiary: Color::Rgb(250, 189, 47),    // yellow
            muted: Color::Rgb(124, 111, 100),      // gray
            danger: Color::Rgb(251, 73, 52),       // red
            success: Color::Rgb(184, 187, 38),     // green
            text: Color::Rgb(235, 219, 178),       // fg
            text_dim: Color::Rgb(168, 153, 132),
            bar_filled: Color::Rgb(254, 128, 25),
            bar_empty: Color::Rgb(60, 56, 54),
        },
        "catppuccin" => Theme {
            name: "catppuccin",
            accent: Color::Rgb(245, 194, 231),     // pink
            secondary: Color::Rgb(148, 226, 213),  // teal
            tertiary: Color::Rgb(249, 226, 175),   // yellow
            muted: Color::Rgb(88, 91, 112),        // overlay0
            danger: Color::Rgb(243, 139, 168),     // red
            success: Color::Rgb(166, 227, 161),    // green
            text: Color::Rgb(205, 214, 244),       // text
            text_dim: Color::Rgb(147, 153, 178),   // overlay1
            bar_filled: Color::Rgb(203, 166, 247), // mauve
            bar_empty: Color::Rgb(49, 50, 68),     // mantle
        },
        "mono" => Theme {
            name: "mono",
            accent: Color::White,
            secondary: Color::Gray,
            tertiary: Color::White,
            muted: Color::DarkGray,
            danger: Color::LightRed,
            success: Color::LightGreen,
            text: Color::White,
            text_dim: Color::Gray,
            bar_filled: Color::White,
            bar_empty: Color::DarkGray,
        },
        _ => Theme {
            // "ember" - default
            name: "ember",
            accent: Color::Rgb(255, 107, 53),      // warm orange
            secondary: Color::Rgb(78, 205, 196),   // teal
            tertiary: Color::Rgb(255, 230, 109),   // gold
            muted: Color::Rgb(85, 85, 85),         // dim gray
            danger: Color::Rgb(255, 23, 68),       // red
            success: Color::Rgb(0, 230, 118),      // green
            text: Color::Rgb(240, 240, 240),       // near white
            text_dim: Color::Rgb(160, 160, 160),   // light gray
            bar_filled: Color::Rgb(255, 107, 53),
            bar_empty: Color::Rgb(50, 50, 50),
        },
    }
}
