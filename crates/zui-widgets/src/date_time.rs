//! Calendar arithmetic is deterministic; locale, time zones and clock access stay outside.
use crate::{
    ButtonView, Container, PopupPlacement, PopupView, TextInputView, TextView,
    action_button::ActionButtonView, column, commit_input::CommitInputView, grid, row,
};
use alloc::{rc::Rc, string::String, vec::Vec};
use zui_api::{
    action::ActionId,
    layout::{Dimension, Edges, LayoutStyle, Track},
    view::View,
};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}
impl Date {
    pub fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        let value = Self { year, month, day };
        value.is_valid().then_some(value)
    }
    pub fn days_in_month(year: u16, month: u8) -> u8 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if year.is_multiple_of(400) || year.is_multiple_of(4) && !year.is_multiple_of(100) {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        }
    }
    pub fn is_valid(self) -> bool {
        (1..=9999).contains(&self.year)
            && (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= Self::days_in_month(self.year, self.month)
    }
    pub fn weekday(self) -> Option<u8> {
        if !self.is_valid() {
            return None;
        }
        let y = self.year as u32 - 1;
        let mut days = 365 * y + y / 4 - y / 100 + y / 400;
        for month in 1..self.month {
            days += Self::days_in_month(self.year, month) as u32;
        }
        Some(((days + self.day as u32 - 1) % 7) as u8)
    }
    pub fn shift_month(self, delta: i32) -> Option<Self> {
        if !self.is_valid() {
            return None;
        }
        let n = ((self.year as i32 - 1) * 12 + self.month as i32 - 1).checked_add(delta)?;
        if !(0..9999 * 12).contains(&n) {
            return None;
        }
        let year = (n / 12 + 1) as u16;
        let month = (n % 12 + 1) as u8;
        Self::new(year, month, self.day.min(Self::days_in_month(year, month)))
    }
}
impl core::fmt::Display for Date {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateEvent {
    Open,
    MonthChanged { year: u16, month: u8 },
    Selected(Date),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateError {
    InvalidDate,
    InvalidTime,
    InvalidRange,
}
impl core::fmt::Display for DateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "date error: {self:?}")
    }
}
impl core::error::Error for DateError {}
#[derive(Clone)]
pub struct CalendarView {
    pub year: u16,
    pub month: u8,
    pub selected: Option<Date>,
    pub min: Option<Date>,
    pub max: Option<Date>,
    pub disabled: bool,
    pub theme: Theme,
    pub families: Vec<String>,
    pub action: ActionId,
}
impl CalendarView {
    pub fn themed(
        theme: &Theme,
        year: u16,
        month: u8,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            year,
            month,
            selected: None,
            min: None,
            max: None,
            disabled: false,
            theme: theme.clone(),
            families,
            action,
        }
    }
    fn valid(&self) -> Result<Date, DateError> {
        let first = Date::new(self.year, self.month, 1).ok_or(DateError::InvalidDate)?;
        if self
            .selected
            .into_iter()
            .chain(self.min)
            .chain(self.max)
            .any(|d| !d.is_valid())
        {
            return Err(DateError::InvalidDate);
        }
        if self.min.zip(self.max).is_some_and(|(a, b)| a > b)
            || self.selected.is_some_and(|d| !self.allowed(d))
        {
            return Err(DateError::InvalidRange);
        }
        Ok(first)
    }
    fn allowed(&self, d: Date) -> bool {
        self.min.is_none_or(|m| d >= m) && self.max.is_none_or(|m| d <= m)
    }
    pub fn build(&self) -> Result<View, DateError> {
        let first = self.valid()?;
        let nav = |delta, label| {
            let next = first.shift_month(delta);
            let mut button =
                ButtonView::themed(&self.theme, label, self.families.clone(), self.action);
            button.disabled = self.disabled || next.is_none();
            let next = next.unwrap_or(first);
            View::new(ActionButtonView {
                button,
                payload: DateEvent::MonthChanged {
                    year: next.year,
                    month: next.month,
                },
            })
            .styled(LayoutStyle {
                padding: Edges::all(6.0),
                ..Default::default()
            })
        };
        let title = View::new(TextView::themed(
            &self.theme,
            alloc::format!("{}年{}月", self.year, self.month),
            self.families.clone(),
            TypeRole::Body,
        ));
        let header = row([nav(-1, "上月"), title, nav(1, "下月")])
            .style(|s| s.gap = Size::new(8.0, 0.0))
            .build();
        let mut cells = Vec::new();
        for label in ["一", "二", "三", "四", "五", "六", "日"] {
            cells.push(View::new(TextView::themed(
                &self.theme,
                label,
                self.families.clone(),
                TypeRole::Caption,
            )));
        }
        for _ in 0..first.weekday().unwrap() {
            cells.push(Container::empty().build());
        }
        for day in 1..=Date::days_in_month(self.year, self.month) {
            let date = Date { day, ..first };
            let mut button = ButtonView::themed(
                &self.theme,
                alloc::format!("{day}"),
                self.families.clone(),
                self.action,
            );
            button.disabled = self.disabled || !self.allowed(date);
            if self.selected != Some(date) {
                button.style.normal.background = self.theme.color(ColorRole::Surface);
                button.style.normal.foreground = self.theme.color(ColorRole::Text);
            }
            cells.push(
                View::new(ActionButtonView {
                    button,
                    payload: DateEvent::Selected(date),
                })
                .styled(LayoutStyle {
                    min_height: Dimension::Length(32.0),
                    ..Default::default()
                }),
            );
        }
        Ok(column([
            header,
            grid([Track::Fraction(1.0); 7], cells)
                .style(|s| s.gap = Size::new(2.0, 2.0))
                .build(),
        ])
        .style(|s| {
            s.padding = Edges::all(8.0);
            s.gap = Size::new(0.0, 8.0);
        })
        .background(self.theme.color(ColorRole::Surface))
        .build())
    }
}
#[derive(Clone)]
pub struct DatePickerView {
    pub calendar: CalendarView,
    pub placeholder: String,
}
impl DatePickerView {
    pub fn themed(theme: &Theme, date: Date, families: Vec<String>, action: ActionId) -> Self {
        let mut calendar = CalendarView::themed(theme, date.year, date.month, families, action);
        calendar.selected = Some(date);
        Self {
            calendar,
            placeholder: "选择日期".into(),
        }
    }
    pub fn build(&self) -> Result<View, DateError> {
        self.calendar.valid()?;
        let mut button = ButtonView::themed(
            &self.calendar.theme,
            self.calendar
                .selected
                .map_or_else(|| self.placeholder.clone(), |d| alloc::format!("{d}")),
            self.calendar.families.clone(),
            self.calendar.action,
        );
        button.disabled = self.calendar.disabled;
        button.trailing_icon = Some(crate::Icon::builtin(crate::IconKind::ChevronDown));
        Ok(View::new(ActionButtonView {
            button,
            payload: DateEvent::Open,
        })
        .styled(LayoutStyle {
            padding: Edges::all(8.0),
            ..Default::default()
        }))
    }
    pub fn popup(&self, anchor: Rect, dismiss: ActionId) -> Result<View, DateError> {
        Ok(PopupView::new(
            PopupPlacement::Anchor(anchor),
            Size::new(320.0, 330.0),
            dismiss,
        )
        .build(crate::ScrollView::vertical(self.calendar.build()?).build()))
    }
}
pub struct DateRangePickerView {
    pub start: DatePickerView,
    pub end: DatePickerView,
}
impl DateRangePickerView {
    pub fn build(&self) -> Result<View, DateError> {
        if self
            .start
            .calendar
            .selected
            .zip(self.end.calendar.selected)
            .is_some_and(|(a, b)| a > b)
        {
            return Err(DateError::InvalidRange);
        }
        Ok(row([self.start.build()?, self.end.build()?])
            .style(|s| {
                s.wrap = true;
                s.gap = Size::new(8.0, 8.0);
            })
            .build())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
impl Time {
    pub fn new(hour: u8, minute: u8, second: u8) -> Option<Self> {
        (hour < 24 && minute < 60 && second < 60).then_some(Self {
            hour,
            minute,
            second,
        })
    }
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 8
            || bytes[2] != b':'
            || bytes[5] != b':'
            || bytes
                .iter()
                .enumerate()
                .any(|(i, b)| i != 2 && i != 5 && !b.is_ascii_digit())
        {
            return None;
        }
        Self::new(
            text[0..2].parse().ok()?,
            text[3..5].parse().ok()?,
            text[6..8].parse().ok()?,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeChanged {
    pub time: Time,
}
pub struct TimePickerView {
    pub time: Time,
    pub editor: TextInputView,
}
impl TimePickerView {
    pub fn themed(theme: &Theme, time: Time, families: Vec<String>, action: ActionId) -> Self {
        Self {
            time,
            editor: TextInputView::themed(theme, "", families, action),
        }
    }
    pub fn build(&self) -> Result<View, DateError> {
        Time::new(self.time.hour, self.time.minute, self.time.second)
            .ok_or(DateError::InvalidTime)?;
        let mut input = self.editor.clone();
        input.text = alloc::format!(
            "{:02}:{:02}:{:02}",
            self.time.hour,
            self.time.minute,
            self.time.second
        );
        input.placeholder = "HH:MM:SS".into();
        Ok(View::new(CommitInputView {
            cancel: None,
            reset_on_blur: true,
            input,
            parse: Rc::new(|text| Time::parse(text).map(|time| TimeChanged { time })),
        }))
    }
}
