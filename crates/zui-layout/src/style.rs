use taffy::style_helpers::{auto, fr, length, minmax, percent};
use zui_api::layout::{Align, Dimension, Edges, Justify, LayoutKind, LayoutStyle, Overflow, Track};

pub(crate) fn convert(s: &LayoutStyle) -> Option<taffy::Style> {
    let stack = s.kind == LayoutKind::Stack;
    if stack
        && (!s.columns.is_empty() || !s.rows.is_empty() || s.wrap || s.gap != zui_types::Size::ZERO)
    {
        return None;
    }
    let dimensions = [
        s.width,
        s.height,
        s.min_width,
        s.min_height,
        s.max_width,
        s.max_height,
        s.basis,
    ];
    if !dimensions.into_iter().all(|d| match d {
        Dimension::Auto | Dimension::MinContent | Dimension::MaxContent => true,
        Dimension::Length(v) | Dimension::Percent(v) => nonnegative(v),
    }) || !values(s.padding).into_iter().all(nonnegative)
        || !values(s.margin).into_iter().all(f32::is_finite)
        || !s.gap.is_valid()
        || s.gap.width > f32::MAX as f64
        || s.gap.height > f32::MAX as f64
        || !nonnegative(s.grow)
        || !nonnegative(s.shrink)
        || !s.columns.iter().chain(&s.rows).all(|t| match t {
            Track::Auto => true,
            Track::Length(v) | Track::Percent(v) | Track::Fraction(v) => nonnegative(*v),
        })
    {
        return None;
    }
    Some(taffy::Style {
        overflow: taffy::Point {
            x: overflow(s.overflow_x),
            y: overflow(s.overflow_y),
        },
        display: if matches!(s.kind, LayoutKind::Grid | LayoutKind::Stack) {
            taffy::Display::Grid
        } else {
            taffy::Display::Flex
        },
        size: taffy::Size {
            width: dimension(s.width),
            height: dimension(s.height),
        },
        min_size: taffy::Size {
            width: bound(s.min_width)?,
            height: bound(s.min_height)?,
        },
        max_size: taffy::Size {
            width: bound(s.max_width)?,
            height: bound(s.max_height)?,
        },
        padding: taffy::Rect {
            left: length(s.padding.left),
            right: length(s.padding.right),
            top: length(s.padding.top),
            bottom: length(s.padding.bottom),
        },
        margin: taffy::Rect {
            left: length(s.margin.left),
            right: length(s.margin.right),
            top: length(s.margin.top),
            bottom: length(s.margin.bottom),
        },
        gap: taffy::Size {
            width: length(s.gap.width),
            height: length(s.gap.height),
        },
        flex_direction: if s.kind == LayoutKind::Column {
            taffy::FlexDirection::Column
        } else {
            taffy::FlexDirection::Row
        },
        flex_wrap: if s.wrap {
            taffy::FlexWrap::Wrap
        } else {
            taffy::FlexWrap::NoWrap
        },
        flex_grow: s.grow,
        flex_shrink: s.shrink,
        flex_basis: dimension(s.basis),
        align_items: alignment(s.align),
        justify_items: alignment(s.justify_items),
        align_self: s.align_self.map(alignment),
        justify_self: s.justify_self.map(alignment),
        justify_content: match s.justify {
            Justify::Start => taffy::JustifyContent::START,
            Justify::End => taffy::JustifyContent::END,
            Justify::Center => taffy::JustifyContent::CENTER,
            Justify::SpaceBetween => taffy::JustifyContent::SPACE_BETWEEN,
            Justify::SpaceAround => taffy::JustifyContent::SPACE_AROUND,
            Justify::SpaceEvenly => taffy::JustifyContent::SPACE_EVENLY,
        },
        grid_template_columns: if stack {
            vec![minmax(length(0.0), fr(1.0))]
        } else {
            s.columns.iter().map(track).collect()
        },
        grid_template_rows: if stack {
            vec![minmax(length(0.0), fr(1.0))]
        } else {
            s.rows.iter().map(track).collect()
        },
        ..Default::default()
    })
}
fn overflow(value: Overflow) -> taffy::Overflow {
    match value {
        Overflow::Visible => taffy::Overflow::Visible,
        Overflow::Clip => taffy::Overflow::Clip,
        Overflow::Scroll => taffy::Overflow::Scroll,
    }
}
fn alignment(value: Align) -> taffy::AlignItems {
    match value {
        Align::Start => taffy::AlignItems::START,
        Align::End => taffy::AlignItems::END,
        Align::Center => taffy::AlignItems::CENTER,
        Align::Stretch => taffy::AlignItems::STRETCH,
    }
}
fn nonnegative(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}
fn values(e: Edges) -> [f32; 4] {
    [e.left, e.right, e.top, e.bottom]
}
fn dimension(d: Dimension) -> taffy::Dimension {
    match d {
        Dimension::Auto => auto(),
        Dimension::Length(v) => length(v),
        Dimension::Percent(v) => percent(v),
        Dimension::MinContent => taffy::Dimension::min_content(),
        Dimension::MaxContent => taffy::Dimension::max_content(),
    }
}
fn bound(d: Dimension) -> Option<taffy::LengthPercentageAuto> {
    Some(match d {
        Dimension::Auto => auto(),
        Dimension::Length(v) => length(v),
        Dimension::Percent(v) => percent(v),
        Dimension::MinContent | Dimension::MaxContent => return None,
    })
}
fn track(t: &Track) -> taffy::GridTemplateComponent<String> {
    match *t {
        Track::Auto => auto(),
        Track::Length(v) => length(v),
        Track::Percent(v) => percent(v),
        Track::Fraction(v) => fr(v),
    }
}
