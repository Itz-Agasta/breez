//! Wallpaper gradient presets.
//!
//! The id is what `project.json` stores in `style.wallpaper`. Both the egui
//! preview and the export compositor look the stops up here, so a project
//! renders the same background in each.

/// (id, gradient top, gradient bottom) as sRGB bytes.
pub const WALLPAPERS: &[(&str, [u8; 3], [u8; 3])] = &[
    ("aurora", [0x14, 0x55, 0x52], [0x67, 0x2f, 0xa8]),
    ("sunset", [0xc2, 0x55, 0x1f], [0x4c, 0x1d, 0x95]),
    ("dusk", [0x1e, 0x29, 0x3b], [0x6d, 0x28, 0x59]),
    ("ocean", [0x0c, 0x4a, 0x6e], [0x15, 0x5e, 0x75]),
    ("forest", [0x14, 0x53, 0x2d], [0x36, 0x53, 0x14]),
    ("ember", [0x7f, 0x1d, 0x1d], [0xc2, 0x41, 0x0c]),
    ("mono", [0x26, 0x26, 0x26], [0x0f, 0x0f, 0x0f]),
];

/// Gradient stops for a wallpaper id. Unknown ids fall back to the first
/// preset, because `Style::clamp` deliberately leaves them alone rather than
/// rewriting a project authored by a newer build.
pub fn wallpaper_colors(id: &str) -> ([u8; 3], [u8; 3]) {
    WALLPAPERS
        .iter()
        .find(|(name, _, _)| *name == id)
        .map(|(_, top, bottom)| (*top, *bottom))
        .unwrap_or((WALLPAPERS[0].1, WALLPAPERS[0].2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallpaper_colors_should_fall_back_to_the_first_preset() {
        assert_eq!(wallpaper_colors("nope"), wallpaper_colors("aurora"));
    }

    #[test]
    fn wallpaper_colors_should_return_the_named_preset() {
        assert_eq!(
            wallpaper_colors("mono"),
            ([0x26, 0x26, 0x26], [0x0f, 0x0f, 0x0f])
        );
    }
}
