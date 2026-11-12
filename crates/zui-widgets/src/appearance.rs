use crate::interaction::Interaction;
use zui_paint::{Painter, Path};
use zui_types::{Color, Rect};

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualState {
    pub disabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub focus_visible: bool,
}

impl VisualState {
    pub fn from_choice<Id: Copy + Eq>(
        choice: &crate::choice::ChoiceState<Id>,
        id: Id,
        disabled: bool,
    ) -> Self {
        Self {
            disabled,
            hovered: choice.hovered == Some(id),
            pressed: (choice.interaction.mouse
                && choice.pressed == Some(id)
                && choice.hovered == Some(id))
                || (choice.interaction.space && choice.active == Some(id)),
            focus_visible: choice.interaction.focused
                && choice.interaction.focus_visible
                && choice.active == Some(id)
                && !disabled,
        }
    }

    pub fn from_interaction(interaction: Interaction, disabled: bool) -> Self {
        Self {
            disabled,
            hovered: interaction.hovered,
            pressed: interaction.space || (interaction.mouse && interaction.hovered),
            focus_visible: interaction.focused && interaction.focus_visible && !disabled,
        }
    }

    pub fn resolve<Palette: Copy>(
        self,
        normal: Palette,
        hovered: Palette,
        pressed: Palette,
        disabled: Palette,
    ) -> Palette {
        if self.disabled {
            disabled
        } else if self.pressed {
            pressed
        } else if self.hovered {
            hovered
        } else {
            normal
        }
    }
}

pub(crate) fn padded(bounds: Rect, horizontal: f64, vertical: f64) -> Rect {
    let horizontal = horizontal.max(0.0).min(bounds.size.width / 2.0);
    let vertical = vertical.max(0.0).min(bounds.size.height / 2.0);
    Rect::new(
        bounds.origin.x + horizontal,
        bounds.origin.y + vertical,
        bounds.size.width - 2.0 * horizontal,
        bounds.size.height - 2.0 * vertical,
    )
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Surface {
    pub bounds: Rect,
    pub radius: f64,
}

impl Surface {
    pub fn path(self) -> Path {
        Path::rounded_rectangle(self.bounds, self.radius).expect("validated control surface")
    }

    pub fn fill(self, painter: &mut Painter<'_>, color: Color) {
        if color.a > 0 && self.bounds.size.width > 0.0 && self.bounds.size.height > 0.0 {
            painter.fill(self.path(), color);
        }
    }

    pub fn outline(self, painter: &mut Painter<'_>, color: Color, width: f64, inset: f64) {
        if color.a == 0
            || width <= 0.0
            || self.bounds.size.width <= 2.0 * inset
            || self.bounds.size.height <= 2.0 * inset
        {
            return;
        }
        let radius = self
            .radius
            .min(self.bounds.size.width / 2.0)
            .min(self.bounds.size.height / 2.0);
        painter.stroke(
            Surface {
                bounds: padded(self.bounds, inset, inset),
                radius: (radius - inset).max(0.0),
            }
            .path(),
            color,
            width,
        );
    }

    pub fn paint(self, painter: &mut Painter<'_>, fill: Color, border: Color) {
        self.fill(painter, fill);
        self.outline(painter, border, 1.0, 0.5);
    }

    pub fn focus(self, painter: &mut Painter<'_>, state: VisualState, color: Color) {
        if state.focus_visible
            && !state.disabled
            && self.bounds.size.width > 4.0
            && self.bounds.size.height > 4.0
        {
            self.outline(painter, color, 2.0, 1.0);
        }
    }

    pub fn joined_border(self, painter: &mut Painter<'_>, color: Color, slots: &[Rect]) {
        self.outline(painter, color, 1.0, 0.5);
        if color.a == 0 || self.bounds.size.height <= 2.0 {
            return;
        }
        for slot in slots.iter().skip(1) {
            if slot.origin.x >= 1.0 && slot.origin.x < self.bounds.size.width - 1.0 {
                painter.fill_rect(
                    Rect::new(
                        self.bounds.origin.x + slot.origin.x,
                        self.bounds.origin.y + 1.0,
                        1.0,
                        self.bounds.size.height - 2.0,
                    ),
                    color,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_paint::{Command, Scene};

    #[test]
    fn interaction_precedence_and_focus_cover_every_combination() {
        for bits in 0..64 {
            let disabled = bits & 1 != 0;
            let interaction = Interaction {
                hovered: bits & 2 != 0,
                mouse: bits & 4 != 0,
                space: bits & 8 != 0,
                focused: bits & 16 != 0,
                focus_visible: bits & 32 != 0,
            };
            let state = VisualState::from_interaction(interaction, disabled);
            let expected = if disabled {
                3
            } else if interaction.space || (interaction.mouse && interaction.hovered) {
                2
            } else if interaction.hovered {
                1
            } else {
                0
            };
            assert_eq!(state.resolve(0, 1, 2, 3), expected);
            assert_eq!(
                state.focus_visible,
                !disabled && interaction.focused && interaction.focus_visible
            );
            let palette = state.resolve((0, 10), (1, 11), (2, 12), (3, 13));
            assert_eq!(palette.1, expected + 10);
        }
    }

    #[test]
    fn choice_visuals_only_apply_to_the_armed_and_active_item() {
        let choice = crate::choice::ChoiceState {
            interaction: Interaction {
                mouse: true,
                focused: true,
                focus_visible: true,
                ..Default::default()
            },
            active: Some(1),
            hovered: Some(2),
            pressed: Some(1),
        };
        let first = VisualState::from_choice(&choice, 1, false);
        let second = VisualState::from_choice(&choice, 2, false);
        assert!(!first.pressed && first.focus_visible);
        assert!(!second.pressed && second.hovered && !second.focus_visible);
        assert!(!VisualState::from_choice(&choice, 1, true).focus_visible);
    }

    #[test]
    fn border_radius_tracks_inset_and_transparent_surfaces_emit_nothing() {
        let surface = Surface {
            bounds: Rect::new(10.0, 20.0, 40.0, 20.0),
            radius: 100.0,
        };
        let mut scene = Scene::new();
        surface.paint(&mut scene.painter(), Color::TRANSPARENT, Color::TRANSPARENT);
        assert!(scene.commands().is_empty());
        surface.outline(&mut scene.painter(), Color::BLACK, 1.0, 0.5);
        let Command::Stroke { path, width, .. } = &scene.commands()[0] else {
            panic!("expected border")
        };
        assert_eq!(*width, 1.0);
        assert_eq!(
            *path,
            Path::rounded_rectangle(Rect::new(10.5, 20.5, 39.0, 19.0), 9.5).unwrap()
        );
        assert_eq!(
            padded(Rect::new(0.0, 0.0, 2.0, 1.0), 12.0, 8.0),
            Rect::new(1.0, 0.5, 0.0, 0.0)
        );
        scene.clear();
        Surface {
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
            radius: 3.0,
        }
        .outline(&mut scene.painter(), Color::BLACK, 1.0, 0.5);
        assert!(scene.commands().is_empty());
    }

    #[test]
    fn joined_group_has_one_frame_and_one_line_per_boundary() {
        let mut scene = Scene::new();
        Surface {
            bounds: Rect::new(0.0, 0.0, 90.0, 20.0),
            radius: 4.0,
        }
        .joined_border(
            &mut scene.painter(),
            Color::BLACK,
            &[
                Rect::new(0.0, 0.0, 30.0, 20.0),
                Rect::new(30.0, 0.0, 30.0, 20.0),
                Rect::new(60.0, 0.0, 30.0, 20.0),
            ],
        );
        assert_eq!(scene.commands().len(), 3);
        assert!(matches!(scene.commands()[0], Command::Stroke { .. }));
        for (command, position) in scene.commands()[1..].iter().zip([30.0, 60.0]) {
            let Command::Fill { path, .. } = command else {
                panic!("expected shared divider")
            };
            assert_eq!(*path, Path::rectangle(Rect::new(position, 1.0, 1.0, 18.0)));
        }
    }
}
