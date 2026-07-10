//! Left tool rail: 60px column of labeled icon buttons. Media and Music open
//! the tool panel, Record jumps back to the record view, the rest are
//! post-MVP and render disabled with a "soon" tooltip.

use eframe::egui::{
    Align2, CornerRadius, FontFamily, FontId, Frame, Panel, Rect, Sense, Stroke, Ui, pos2, vec2,
};

use super::{EditorAction, Tool};
use crate::theme;

struct RailItem {
    icon: &'static str,
    label: &'static str,
    kind: ItemKind,
}

enum ItemKind {
    Tool(Tool),
    Record,
    Soon,
}

const ITEMS: &[RailItem] = &[
    RailItem {
        icon: "\u{1f5bc}",
        label: "Media",
        kind: ItemKind::Tool(Tool::Media),
    },
    RailItem {
        icon: "\u{1f3b5}",
        label: "Music",
        kind: ItemKind::Tool(Tool::Music),
    },
    RailItem {
        icon: "\u{1f3a4}",
        label: "Voice",
        kind: ItemKind::Soon,
    },
    RailItem {
        icon: "\u{23fa}",
        label: "Record",
        kind: ItemKind::Record,
    },
    RailItem {
        icon: "T",
        label: "Text",
        kind: ItemKind::Soon,
    },
    RailItem {
        icon: "\u{1f4ac}",
        label: "Captions",
        kind: ItemKind::Soon,
    },
];

const PLUGINS: RailItem = RailItem {
    icon: "\u{1f9e9}",
    label: "Plugins",
    kind: ItemKind::Soon,
};

pub fn show(ui: &mut Ui, open_tool: &mut Option<Tool>) -> Option<EditorAction> {
    let mut action = None;
    Panel::left("toolrail")
        .exact_size(theme::RAIL_WIDTH)
        .frame(Frame::new().fill(theme::BG_PANEL_DARK))
        .show_separator_line(false)
        .show(ui, |ui| {
            let rail = ui.max_rect();
            ui.painter().vline(
                rail.max.x - 0.5,
                rail.y_range(),
                Stroke::new(1.0, theme::BORDER),
            );
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                for item in ITEMS {
                    if let Some(a) = rail_button(ui, item, open_tool) {
                        action = Some(a);
                    }
                    ui.add_space(4.0);
                }
            });
            // Plugins pins to the rail bottom.
            let bottom = Rect::from_min_size(
                pos2(rail.min.x + (rail.width() - 44.0) / 2.0, rail.max.y - 54.0),
                vec2(44.0, 46.0),
            );
            let mut bottom_ui = ui.new_child(eframe::egui::UiBuilder::new().max_rect(bottom));
            rail_button(&mut bottom_ui, &PLUGINS, open_tool);
        });
    action
}

fn rail_button(ui: &mut Ui, item: &RailItem, open_tool: &mut Option<Tool>) -> Option<EditorAction> {
    let enabled = !matches!(item.kind, ItemKind::Soon);
    let (rect, response) = ui.allocate_exact_size(
        vec2(44.0, 46.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let active = matches!(item.kind, ItemKind::Tool(t) if *open_tool == Some(t));
    let bg = if active {
        theme::BG_CONTROL
    } else if enabled && response.hovered() {
        theme::BG_CONTROL_HOVER
    } else {
        eframe::egui::Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::RADIUS_BUTTON), bg);
    if active {
        ui.painter().rect_filled(
            Rect::from_min_size(rect.left_top() + vec2(-6.0, 14.0), vec2(2.0, 18.0)),
            CornerRadius::same(1),
            theme::ACCENT,
        );
    }
    let fg = if !enabled {
        theme::TEXT_LABEL
    } else if active {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    ui.painter().text(
        rect.center() - vec2(0.0, 7.0),
        Align2::CENTER_CENTER,
        item.icon,
        FontId::proportional(14.0),
        fg,
    );
    ui.painter().text(
        rect.center() + vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        item.label,
        FontId::new(8.5, FontFamily::Proportional),
        fg,
    );
    if !enabled {
        response.on_hover_text("Coming soon");
        return None;
    }
    if response.clicked() {
        match item.kind {
            ItemKind::Tool(tool) => {
                *open_tool = if *open_tool == Some(tool) {
                    None
                } else {
                    Some(tool)
                };
            }
            ItemKind::Record => return Some(EditorAction::OpenRecord),
            ItemKind::Soon => {}
        }
    }
    None
}
