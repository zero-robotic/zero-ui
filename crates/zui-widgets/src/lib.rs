//! Standard controls depend only on portable contracts, never on the runtime.
#![no_std]
extern crate alloc;
mod accordion;
pub mod edit;
pub mod virtualization;
pub use accordion::{
    AccordionChanged, AccordionError, AccordionExpansion, AccordionId, AccordionItem, AccordionView,
};
mod canvas;
pub use canvas::{Canvas, CanvasView};
mod icon;
pub use icon::{Icon, IconKind, IconView, IconWidget};
mod image;
pub use image::{Image, ImageAlign, ImageFit, ImageView};
mod stepper;
pub use stepper::{InvalidStepperValue, StepperChanged, StepperView};
mod toggle;
pub use toggle::{ToggleButton, ToggleButtonStyle, ToggleButtonView, ToggleChanged};
mod list_box;
pub use list_box::{
    ListBox, ListBoxColors, ListBoxStyle, ListBoxView, ListItem, ListItemId,
    ListMultiSelectionChanged, ListSelectionChanged,
};
mod button;
mod checkbox;
mod choice;
pub use checkbox::{
    CheckBox, CheckBoxColors, CheckBoxStyle, CheckBoxView, CheckChanged, CheckState,
};
mod container;
mod disclosure;
pub use disclosure::{DisclosureChanged, DisclosureView};
mod appearance;
mod interaction;
mod numeric;
pub use numeric::{InvalidNumericRange, NumericRange};
mod axis;
mod progress;
mod scroll;
mod scrollbar;
mod slider;
mod switch;
pub use crate::virtualization::{Alignment as ScrollAlignment, Limits as VirtualIndexLimits};
pub use progress::{
    ProgressAnimation, ProgressBar, ProgressBarStyle, ProgressBarView, ProgressDirection,
    ProgressValue,
};
pub use scroll::{
    Scroll, ScrollAxes, ScrollChanged, ScrollView, ScrollbarStyle, VirtualList,
    VirtualListNavigate, VirtualListView,
};
pub use slider::{Slider, SliderChanged, SliderColors, SliderStyle, SliderView};
pub use switch::{Switch, SwitchChanged, SwitchColors, SwitchStyle, SwitchView};
mod radio;
mod segmented;
pub use button::{Button, ButtonActivated, ButtonColors, ButtonStyle, ButtonView};
pub use radio::{
    RadioChanged, RadioColors, RadioGroup, RadioGroupStyle, RadioGroupView, RadioOption,
    RadioOptionId, RadioOrientation,
};
pub use segmented::{
    SegmentChanged, SegmentColors, SegmentId, SegmentOption, SegmentedControl,
    SegmentedControlView, SegmentedStyle,
};
mod tabs;
pub use tabs::{
    TabChanged, TabCloseRequested, TabDetachRequested, TabId, TabOption, TabReorderRequested,
    TabsError, TabsView,
};
mod text;
mod text_input;
pub use container::{Container, column, grid, row, stack};
pub use text::{Text, TextView};
pub use text_input::{
    DraftCacheError, DraftCachePolicy, DraftCacheReport, DraftCacheUsage, TextInput,
    TextInputDraft, TextInputDraftCache, TextInputDraftMemory, TextInputError, TextInputEvent,
    TextInputStyle, TextInputView,
};

mod popup;
pub use popup::{DismissReason, Popup, PopupDismissed, PopupPlacement, PopupView};
mod action_button;
mod number_input;
pub use number_input::{NumberChanged, NumberInput, NumberInputView, NumberRejected};
mod select;
pub use select::{SelectEvent, SelectView};
mod combo_box;
pub use combo_box::{ComboBoxEvent, ComboBoxView};
mod dialog;
pub use dialog::{DialogResult, DialogView, MessageBoxView};
mod menu;
pub use menu::{
    ContextMenuRegion, ContextMenuRequested, MenuBarView, MenuBehavior, MenuError, MenuEvent,
    MenuId, MenuItem, MenuShortcut, MenuView,
};
mod tooltip;
pub use tooltip::{Tooltip, TooltipAnchor, TooltipView};
mod tree_view;
pub use tree_view::{
    TreeChildrenState, TreeDropRequested, TreeEvent, TreeItem, TreeItemId, TreeLoadRequested,
    TreeSelectionChanged, TreeView, TreeViewError,
};
mod table;
pub use table::{
    SortDirection, TableColumn, TableColumnId, TableError, TableEvent, TableRow, TableRowId,
    TableSort, TableView,
};
mod splitter;
pub use splitter::{SplitAxis, Splitter, SplitterChanged, SplitterView};
mod text_area;
pub use text_area::TextAreaView;
mod form;
pub use form::{FormField, FormFieldView, FormLayout, FormView};
mod chrome;
pub use chrome::{
    GroupBoxView, Separator, SeparatorAxis, SeparatorView, StatusBarView, ToolBarView, ToolbarView,
};
mod navigation;
pub use navigation::{
    BreadcrumbItem, BreadcrumbSelected, BreadcrumbView, NavigationError, PageChanged,
    PaginationView,
};
mod commit_input;
mod date_time;
pub use date_time::{
    CalendarView, Date, DateError, DateEvent, DatePickerView, DateRangePickerView, Time,
    TimeChanged, TimePickerView,
};
mod notification;
pub use notification::{
    NotificationDismissed, NotificationKind, NotificationView, Toast, ToastView,
};
mod file_picker;
pub use file_picker::{
    FileEntry, FileEntryId, FilePickerError, FilePickerEvent, FilePickerMode, FilePickerView,
};
mod color_picker;
pub use color_picker::{ColorChanged, ColorPickerView, parse_hex_color};
pub type SplitPaneView = SplitterView;
mod data_grid;
pub use data_grid::{
    CellAddress, DataGridCellFactory, DataGridError, DataGridEvent, DataGridView,
    DataGridVirtualRows,
};

mod hsv_picker;
pub use hsv_picker::{HsvChanged, HsvColor, HsvPicker, HsvPickerView};
