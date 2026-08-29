//! Paint commands, clip shapes, vector paths, and validation.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum ClipShape {
    Rect(Rect),
    RoundedRect { rect: Rect, radius: Dip },
    Path { path: IconPath },
}

impl ClipShape {
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Rect(rect) | Self::RoundedRect { rect, .. } => *rect,
            Self::Path { path } => {
                let mut bounds = None;
                for point in path.points() {
                    bounds = Some(match bounds {
                        Some(bounds) => union_rect(
                            bounds,
                            Rect {
                                origin: point,
                                size: zui_core::Size::default(),
                            },
                        ),
                        None => Rect {
                            origin: point,
                            size: zui_core::Size::default(),
                        },
                    });
                }
                bounds.unwrap_or_default()
            }
        }
    }
}

pub(crate) fn transform_clip_shape(shape: &ClipShape, transform: Transform) -> ClipShape {
    match shape {
        ClipShape::Rect(rect) => ClipShape::Rect(transform.rect(*rect)),
        ClipShape::RoundedRect { rect, radius } => ClipShape::RoundedRect {
            rect: transform.rect(*rect),
            radius: *radius,
        },
        ClipShape::Path { path } => ClipShape::Path {
            path: path.transformed(transform),
        },
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaintCommand {
    Clear(Color),
    Rect {
        rect: Rect,
        color: Color,
    },
    RoundedRect {
        rect: Rect,
        radius: Dip,
        color: Color,
    },
    Line {
        start: Point,
        end: Point,
        width: Dip,
        color: Color,
    },
    Text {
        text: String,
        origin: Point,
        color: Color,
        scale: u32,
    },
    Icon {
        rect: Rect,
        path: IconPath,
        color: Color,
        stroke: Dip,
    },
    PathFill {
        path: IconPath,
        color: Color,
    },
    PathStroke {
        path: IconPath,
        width: Dip,
        color: Color,
    },
    Image {
        rect: Rect,
        image: ImageId,
        opacity: f32,
    },
    /// Begins a nested clip scope. `PopClip` restores the previous scope.
    PushClip(ClipShape),
    PopClip,
    PushTransform(Transform),
    PopTransform,
    PushOpacity(f32),
    PopOpacity,
}

impl PaintCommand {
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Self::Clear(_)
            | Self::PushTransform(_)
            | Self::PopTransform
            | Self::PushOpacity(_)
            | Self::PopOpacity
            | Self::PopClip => None,
            Self::Rect { rect, .. } | Self::RoundedRect { rect, .. } | Self::Image { rect, .. } => {
                Some(*rect)
            }
            Self::PushClip(shape) => Some(shape.bounds()),
            Self::Line {
                start, end, width, ..
            } => Some(line_bounds(*start, *end, *width)),
            Self::Text {
                text,
                origin,
                scale,
                ..
            } => Some(Rect {
                origin: *origin,
                size: zui_core::Size {
                    width: measure_text(text, *scale),
                    height: Dip((*scale).max(1) as f32 * 7.0),
                },
            }),
            Self::Icon { rect, .. } => Some(*rect),
            Self::PathFill { path, .. } | Self::PathStroke { path, .. } => Some(path_bounds(path)),
        }
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        match self {
            Self::Clear(color) => validate_color(*color),
            Self::Rect { rect, color } => {
                validate_rect(*rect)?;
                validate_color(*color)
            }
            Self::RoundedRect {
                rect,
                radius,
                color,
            } => {
                validate_rect(*rect)?;
                if !radius.0.is_finite() || radius.0 < 0.0 {
                    return Err(RenderError::InvalidCommand(
                        "rounded radius must be finite and non-negative".into(),
                    ));
                }
                validate_color(*color)
            }
            Self::Line {
                start,
                end,
                width,
                color,
            } => {
                if !point_is_finite(*start)
                    || !point_is_finite(*end)
                    || !width.0.is_finite()
                    || width.0 <= 0.0
                {
                    return Err(RenderError::InvalidCommand(
                        "line geometry is invalid".into(),
                    ));
                }
                validate_color(*color)
            }
            Self::Text {
                origin,
                scale,
                color,
                ..
            } => {
                if !point_is_finite(*origin) || *scale == 0 {
                    return Err(RenderError::InvalidCommand(
                        "text geometry is invalid".into(),
                    ));
                }
                validate_color(*color)
            }
            Self::Icon {
                rect,
                path,
                stroke,
                color,
            } => {
                validate_rect(*rect)?;
                if !stroke.0.is_finite() || stroke.0 <= 0.0 {
                    return Err(RenderError::InvalidCommand(
                        "icon geometry is invalid".into(),
                    ));
                }
                validate_path(path)?;
                validate_color(*color)
            }
            Self::PathFill { path, color } => {
                validate_path(path)?;
                validate_color(*color)
            }
            Self::PathStroke { path, width, color } => {
                validate_path(path)?;
                if !width.0.is_finite() || width.0 <= 0.0 {
                    return Err(RenderError::InvalidCommand(
                        "path stroke width is invalid".into(),
                    ));
                }
                validate_color(*color)
            }
            Self::Image { rect, opacity, .. } => {
                validate_rect(*rect)?;
                if !opacity.is_finite() || !(0.0..=1.0).contains(opacity) {
                    return Err(RenderError::InvalidCommand(
                        "image opacity is outside 0..=1".into(),
                    ));
                }
                Ok(())
            }
            Self::PushClip(shape) => {
                validate_rect(shape.bounds())?;
                if let ClipShape::RoundedRect { radius, .. } = shape {
                    if !radius.0.is_finite() || radius.0 < 0.0 {
                        return Err(RenderError::InvalidCommand(
                            "rounded clip radius must be finite and non-negative".into(),
                        ));
                    }
                }
                if let ClipShape::Path { path } = shape {
                    if path.points().any(|point| !point_is_finite(point))
                        || path.to_lyon().is_none()
                    {
                        return Err(RenderError::InvalidCommand(
                            "path clip contains invalid commands or non-finite points".into(),
                        ));
                    }
                }
                Ok(())
            }
            Self::PopClip | Self::PopTransform | Self::PopOpacity => Ok(()),
            Self::PushTransform(transform) => {
                if transform.matrix.iter().all(|value| value.is_finite()) {
                    Ok(())
                } else {
                    Err(RenderError::InvalidCommand(
                        "transform contains a non-finite value".into(),
                    ))
                }
            }
            Self::PushOpacity(value) => {
                if value.is_finite() && (0.0..=1.0).contains(value) {
                    Ok(())
                } else {
                    Err(RenderError::InvalidCommand(
                        "opacity is outside 0..=1".into(),
                    ))
                }
            }
        }
    }

    /// Validates both individual commands and scoped state transitions.
    pub fn validate_sequence(commands: &[Self]) -> Result<(), RenderError> {
        let mut clips = 0_usize;
        let mut transforms = 0_usize;
        let mut opacities = 0_usize;
        for command in commands {
            command.validate()?;
            match command {
                Self::PushClip(_) => clips += 1,
                Self::PopClip if clips == 0 => {
                    return Err(RenderError::InvalidCommand("clip stack underflow".into()))
                }
                Self::PopClip => clips -= 1,
                Self::PushTransform(_) => transforms += 1,
                Self::PopTransform if transforms == 0 => {
                    return Err(RenderError::InvalidCommand(
                        "transform stack underflow".into(),
                    ))
                }
                Self::PopTransform => transforms -= 1,
                Self::PushOpacity(_) => opacities += 1,
                Self::PopOpacity if opacities == 0 => {
                    return Err(RenderError::InvalidCommand(
                        "opacity stack underflow".into(),
                    ))
                }
                Self::PopOpacity => opacities -= 1,
                _ => {}
            }
        }
        if clips != 0 || transforms != 0 || opacities != 0 {
            return Err(RenderError::InvalidCommand(
                "unbalanced scoped paint state".into(),
            ));
        }
        Ok(())
    }

    pub fn is_draw_command(&self) -> bool {
        !matches!(
            self,
            Self::Clear(_)
                | Self::PushClip(_)
                | Self::PopClip
                | Self::PushTransform(_)
                | Self::PopTransform
                | Self::PushOpacity(_)
                | Self::PopOpacity
        )
    }
}

fn validate_path(path: &IconPath) -> Result<(), RenderError> {
    if path.points().any(|point| !point_is_finite(point)) || path.to_lyon().is_none() {
        Err(RenderError::InvalidCommand(
            "path contains invalid commands or non-finite points".into(),
        ))
    } else {
        Ok(())
    }
}

fn point_is_finite(point: Point) -> bool {
    point.x.0.is_finite() && point.y.0.is_finite()
}
fn validate_color(color: Color) -> Result<(), RenderError> {
    if [color.r, color.g, color.b, color.a]
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        Ok(())
    } else {
        Err(RenderError::InvalidCommand("color is invalid".into()))
    }
}
fn validate_rect(rect: Rect) -> Result<(), RenderError> {
    if point_is_finite(rect.origin)
        && rect.size.width.0.is_finite()
        && rect.size.height.0.is_finite()
        && rect.size.width.0 >= 0.0
        && rect.size.height.0 >= 0.0
    {
        Ok(())
    } else {
        Err(RenderError::InvalidCommand(
            "rect geometry is invalid".into(),
        ))
    }
}
fn line_bounds(start: Point, end: Point, width: Dip) -> Rect {
    let half = width.0 / 2.0;
    Rect {
        origin: Point {
            x: Dip(start.x.0.min(end.x.0) - half),
            y: Dip(start.y.0.min(end.y.0) - half),
        },
        size: zui_core::Size {
            width: Dip((start.x.0.max(end.x.0) - start.x.0.min(end.x.0)) + width.0),
            height: Dip((start.y.0.max(end.y.0) - start.y.0.min(end.y.0)) + width.0),
        },
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    EvenOdd,
    NonZero,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    QuadTo {
        control: Point,
        to: Point,
    },
    CubicTo {
        control1: Point,
        control2: Point,
        to: Point,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IconPath {
    pub commands: Vec<PathCommand>,
    pub fill_rule: FillRule,
}

impl IconPath {
    pub fn from_commands(commands: impl Into<Vec<PathCommand>>, fill_rule: FillRule) -> Self {
        Self {
            commands: commands.into(),
            fill_rule,
        }
    }

    pub fn transformed(&self, transform: Transform) -> Self {
        let map = |point: Point| transform.point(point);
        Self {
            commands: self
                .commands
                .iter()
                .map(|command| match command {
                    PathCommand::MoveTo(point) => PathCommand::MoveTo(map(*point)),
                    PathCommand::LineTo(point) => PathCommand::LineTo(map(*point)),
                    PathCommand::QuadTo { control, to } => PathCommand::QuadTo {
                        control: map(*control),
                        to: map(*to),
                    },
                    PathCommand::CubicTo {
                        control1,
                        control2,
                        to,
                    } => PathCommand::CubicTo {
                        control1: map(*control1),
                        control2: map(*control2),
                        to: map(*to),
                    },
                    PathCommand::Close => PathCommand::Close,
                })
                .collect(),
            fill_rule: self.fill_rule,
        }
    }

    fn points(&self) -> impl Iterator<Item = Point> + '_ {
        self.commands.iter().flat_map(|command| match command {
            PathCommand::MoveTo(point) | PathCommand::LineTo(point) => vec![*point],
            PathCommand::QuadTo { control, to } => vec![*control, *to],
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => vec![*control1, *control2, *to],
            PathCommand::Close => Vec::new(),
        })
    }

    pub(crate) fn to_lyon(&self) -> Option<LyonPath> {
        let mut builder = LyonPath::builder();
        let mut open = false;
        for command in &self.commands {
            match command {
                PathCommand::MoveTo(point) => {
                    if open {
                        builder.end(false);
                    }
                    builder.begin(lyon_point(point.x.0, point.y.0));
                    open = true;
                }
                PathCommand::LineTo(point) if open => {
                    builder.line_to(lyon_point(point.x.0, point.y.0));
                }
                PathCommand::QuadTo { control, to } if open => {
                    builder.quadratic_bezier_to(
                        lyon_point(control.x.0, control.y.0),
                        lyon_point(to.x.0, to.y.0),
                    );
                }
                PathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } if open => {
                    builder.cubic_bezier_to(
                        lyon_point(control1.x.0, control1.y.0),
                        lyon_point(control2.x.0, control2.y.0),
                        lyon_point(to.x.0, to.y.0),
                    );
                }
                PathCommand::Close if open => {
                    builder.end(true);
                    open = false;
                }
                _ => return None,
            }
        }
        if open {
            builder.end(false);
        }
        Some(builder.build())
    }
}

impl Default for IconPath {
    fn default() -> Self {
        Self::from_commands(Vec::new(), FillRule::EvenOdd)
    }
}
