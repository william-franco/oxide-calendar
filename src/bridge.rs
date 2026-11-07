//! Connects Slint callbacks to the view model and pushes snapshots back into the window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, Timelike};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::viewmodel::{CalendarViewModel, EditorData, EditorInput, Snapshot, View, hm_of, today};
use crate::{AgendaData, AllDayData, AppWindow, BlockData, CategoryData, DayColumnData, MonthCellData, OxideTheme};

const REMINDER_INTERVAL: Duration = Duration::from_secs(30);
const MESSAGE_DURATION: Duration = Duration::from_secs(4);

struct Bridge {
    vm: Rc<RefCell<CalendarViewModel>>,
    /// Month shown by the mini-calendar. Independent of the main view's anchor.
    mini: Cell<(i32, u32)>,
}

fn list<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}

fn date_key(date: NaiveDate) -> SharedString {
    date.format("%Y-%m-%d").to_string().into()
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

fn notify(ui: &AppWindow, text: &str) {
    ui.set_message(text.into());
    ui.set_message_shown(true);
    let weak = ui.as_weak();
    Timer::single_shot(MESSAGE_DURATION, move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_message_shown(false);
        }
    });
}

fn show_editor(ui: &AppWindow, data: EditorData) {
    ui.set_editing(data.editing);
    ui.set_recurring(data.recurring);
    ui.set_f_title(data.title.into());
    ui.set_f_description(data.description.into());
    ui.set_f_location(data.location.into());
    ui.set_f_category(data.category_index);
    ui.set_f_all_day(data.all_day);
    ui.set_f_start_date(data.start_date.into());
    ui.set_f_start_time(data.start_time.into());
    ui.set_f_end_date(data.end_date.into());
    ui.set_f_end_time(data.end_time.into());
    ui.set_f_recurrence(data.recurrence_index);
    ui.set_f_reminder(data.reminder_index);
    ui.set_f_apply_series(true);
    ui.set_f_error("".into());
    ui.set_editor_shown(true);
}

fn read_input(ui: &AppWindow) -> EditorInput {
    EditorInput {
        title: ui.get_f_title().to_string(),
        description: ui.get_f_description().to_string(),
        location: ui.get_f_location().to_string(),
        category_index: ui.get_f_category(),
        all_day: ui.get_f_all_day(),
        start_date: ui.get_f_start_date().to_string(),
        start_time: ui.get_f_start_time().to_string(),
        end_date: ui.get_f_end_date().to_string(),
        end_time: ui.get_f_end_time().to_string(),
        recurrence_index: ui.get_f_recurrence(),
        reminder_index: ui.get_f_reminder(),
        apply_to_series: ui.get_f_apply_series(),
    }
}

fn apply(ui: &AppWindow, vm: &CalendarViewModel, snap: Snapshot, mini: (i32, u32)) {
    ui.set_view_title(snap.title.into());
    ui.set_view_index(match snap.view {
        View::Month => 0,
        View::Week => 1,
        View::Day => 2,
        View::Agenda => 3,
    });

    ui.set_month_cells(list(
        snap.month_cells
            .into_iter()
            .map(|cell| MonthCellData {
                key: date_key(cell.date),
                label: cell.label.into(),
                in_month: cell.in_month,
                today: cell.today,
                items: list(cell.items.iter().map(|(text, _)| SharedString::from(text.as_str())).collect()),
                accents: list(cell.items.iter().map(|(_, accent)| *accent).collect()),
                more: cell.more as i32,
            })
            .collect(),
    ));

    ui.set_day_columns(list(
        snap.days
            .into_iter()
            .map(|day| DayColumnData {
                key: date_key(day.date),
                label: day.label.into(),
                today: day.today,
                all_day: list(
                    day.all_day
                        .into_iter()
                        .map(|c| AllDayData {
                            event_id: c.event_id as i32,
                            start_key: c.start_key.into(),
                            title: c.title.into(),
                            accent: c.accent,
                        })
                        .collect(),
                ),
                blocks: list(
                    day.blocks
                        .into_iter()
                        .map(|b| BlockData {
                            event_id: b.event_id as i32,
                            start_key: b.start_key.into(),
                            title: b.title.into(),
                            subtitle: b.subtitle.into(),
                            top: b.top_min as f32,
                            height: b.height_min as f32,
                            column: b.column as i32,
                            columns: b.columns as i32,
                            accent: b.accent,
                        })
                        .collect(),
                ),
            })
            .collect(),
    ));

    ui.set_agenda_items(list(
        snap.agenda
            .into_iter()
            .map(|row| AgendaData {
                event_id: row.event_id as i32,
                start_key: row.start_key.into(),
                title: row.title.into(),
                when: row.when.into(),
                location: row.location.into(),
                accent: row.accent,
            })
            .collect(),
    ));
    ui.set_agenda_empty(snap.empty_message.into());

    ui.set_categories(list(
        snap.categories
            .iter()
            .map(|c| CategoryData {
                id: c.id as i32,
                name: c.name.clone().into(),
                accent: c.accent,
                visible: c.visible,
            })
            .collect(),
    ));
    ui.set_category_names(list(snap.categories.iter().map(|c| SharedString::from(c.name.as_str())).collect()));

    let now = Local::now();
    ui.set_now_minutes((now.hour() * 60 + now.minute()) as f32);

    let anchor = vm.anchor();
    ui.set_mini_year(mini.0);
    ui.set_mini_month(mini.1 as i32);
    ui.set_mini_selected_year(anchor.year());
    ui.set_mini_selected_month(anchor.month() as i32);
    ui.set_mini_selected_day(anchor.day() as i32);
    let t = today();
    ui.set_mini_today_year(t.year());
    ui.set_mini_today_month(t.month() as i32);
    ui.set_mini_today_day(t.day() as i32);
}

impl Bridge {
    fn render(&self, ui: &AppWindow) {
        let vm = self.vm.borrow();
        match vm.snapshot() {
            Ok(snap) => apply(ui, &vm, snap, self.mini.get()),
            Err(e) => notify(ui, &e),
        }
    }

    fn with_vm<T>(&self, ui: &AppWindow, f: impl FnOnce(&mut CalendarViewModel) -> Result<T, String>) -> Option<T> {
        let result = f(&mut self.vm.borrow_mut());
        match result {
            Ok(value) => Some(value),
            Err(message) => {
                notify(ui, &message);
                None
            }
        }
    }
}

/// Wires every callback and renders the first snapshot. Returns the reminder timer, which must
/// be kept alive for reminders to keep firing.
pub fn bind(ui: &AppWindow, vm: CalendarViewModel) -> Timer {
    if let Some(dark) = vm.dark_theme() {
        ui.global::<OxideTheme>().set_dark(dark);
        ui.set_theme_saved(true);
    }

    let today_date = today();
    let bridge = Rc::new(Bridge {
        vm: Rc::new(RefCell::new(vm)),
        mini: Cell::new((today_date.year(), today_date.month())),
    });
    bridge.render(ui);

    // Registers a callback that runs `$body` with the window and the shared bridge.
    macro_rules! on {
        ($shared:ident, $setter:ident, |$ui:ident $(, $arg:ident : $ty:ty)*| $body:block) => {{
            let $shared = $shared.clone();
            let weak = ui.as_weak();
            ui.$setter(move |$($arg: $ty),*| {
                if let Some($ui) = weak.upgrade() {
                    let $ui = &$ui;
                    $body
                }
            });
        }};
    }

    on!(bridge, on_navigate, |ui, delta: i32| {
        bridge.vm.borrow_mut().navigate(delta);
        bridge.render(ui);
    });

    on!(bridge, on_go_today, |ui| {
        bridge.vm.borrow_mut().go_today();
        bridge.render(ui);
    });

    on!(bridge, on_view_changed, |ui, index: i32| {
        bridge.vm.borrow_mut().set_view(View::from_index(index));
        bridge.render(ui);
    });

    on!(bridge, on_search, |ui, query: SharedString| {
        bridge.vm.borrow_mut().set_query(&query);
        bridge.render(ui);
    });

    on!(bridge, on_toggle_category, |ui, id: i32, visible: bool| {
        if bridge.with_vm(ui, |vm| vm.set_category_visible(i64::from(id), visible)).is_some() {
            bridge.render(ui);
        }
    });

    on!(bridge, on_open_day, |ui, key: SharedString| {
        if let Some(date) = parse_date(&key) {
            bridge.vm.borrow_mut().open_day(date);
            bridge.render(ui);
        }
    });

    on!(bridge, on_mini_navigate, |ui, year: i32, month: i32| {
        if let Ok(month) = u32::try_from(month) {
            bridge.mini.set((year, month));
            bridge.render(ui);
        }
    });

    on!(bridge, on_mini_select, |ui, year: i32, month: i32, day: i32| {
        let date = u32::try_from(month)
            .ok()
            .zip(u32::try_from(day).ok())
            .and_then(|(m, d)| NaiveDate::from_ymd_opt(year, m, d));
        if let Some(date) = date {
            bridge.vm.borrow_mut().open_day(date);
            bridge.render(ui);
        }
    });

    on!(bridge, on_layout_changed, |ui, days: i32| {
        bridge.vm.borrow_mut().set_week_days(usize::try_from(days).unwrap_or(7));
        bridge.render(ui);
    });

    on!(bridge, on_slot_clicked, |ui, key: SharedString, hour: i32| {
        let Some(date) = parse_date(&key) else { return };
        let categories = bridge.vm.borrow().categories().unwrap_or_default();
        let data = bridge.vm.borrow_mut().open_new(date, u32::try_from(hour).unwrap_or(9), &categories);
        show_editor(ui, data);
    });

    on!(bridge, on_open_event, |ui, id: i32, key: SharedString| {
        let categories = bridge.vm.borrow().categories().unwrap_or_default();
        let opened = bridge.with_vm(ui, |vm| vm.open_occurrence(i64::from(id), &key, &categories));
        if let Some(data) = opened {
            show_editor(ui, data);
        }
    });

    on!(bridge, on_new_event, |ui| {
        let categories = bridge.vm.borrow().categories().unwrap_or_default();
        let date = bridge.vm.borrow().anchor();
        let data = bridge.vm.borrow_mut().open_new(date, 9, &categories);
        show_editor(ui, data);
    });

    on!(bridge, on_save, |ui| {
        let input = read_input(ui);
        let categories = bridge.vm.borrow().categories().unwrap_or_default();
        let result = bridge.vm.borrow_mut().save(&input, &categories);
        match result {
            Ok(()) => {
                ui.set_editor_shown(false);
                bridge.render(ui);
                notify(ui, "Evento salvo");
            }
            Err(message) => ui.set_f_error(message.into()),
        }
    });

    on!(bridge, on_delete_event, |ui| {
        let apply_to_series = ui.get_f_apply_series();
        let result = bridge.vm.borrow_mut().delete(apply_to_series);
        match result {
            Ok(()) => {
                ui.set_editor_shown(false);
                bridge.render(ui);
                notify(ui, "Evento excluído");
            }
            Err(message) => ui.set_f_error(message.into()),
        }
    });

    on!(bridge, on_duplicate_event, |ui| {
        let result = bridge.vm.borrow_mut().duplicate();
        match result {
            Ok(()) => {
                ui.set_editor_shown(false);
                bridge.render(ui);
                notify(ui, "Evento duplicado");
            }
            Err(message) => ui.set_f_error(message.into()),
        }
    });

    on!(bridge, on_dark_changed, |ui, dark: bool| {
        if let Err(message) = bridge.vm.borrow().set_dark_theme(dark) {
            notify(ui, &message);
        }
    });

    on!(bridge, on_close_editor, |ui| {
        bridge.vm.borrow_mut().close_editor();
        ui.set_editor_shown(false);
    });

    on!(bridge, on_import_ics, |ui| {
        let Some(path) = rfd::FileDialog::new().add_filter("iCalendar", &["ics"]).pick_file() else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => return notify(ui, &format!("Não foi possível ler o arquivo: {e}")),
        };
        let result = bridge.vm.borrow_mut().import_ics(&text);
        match result {
            Ok(summary) => {
                bridge.render(ui);
                notify(
                    ui,
                    &format!("{} evento(s) importado(s), {} ignorado(s)", summary.imported, summary.skipped),
                );
            }
            Err(message) => notify(ui, &message),
        }
    });

    on!(bridge, on_export_ics, |ui| {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("calendario.ics")
            .add_filter("iCalendar", &["ics"])
            .save_file()
        else {
            return;
        };
        let result = bridge.vm.borrow().export_ics();
        match result.and_then(|text| std::fs::write(&path, text).map_err(|e| e.to_string())) {
            Ok(()) => notify(ui, "Arquivo exportado"),
            Err(message) => notify(ui, &format!("Falha ao exportar: {message}")),
        }
    });

    // Reminders: checked every 30 seconds while the app is open.
    let reminders = Timer::default();
    let vm_for_timer = bridge.vm.clone();
    let weak = ui.as_weak();
    reminders.start(TimerMode::Repeated, REMINDER_INTERVAL, move || {
        let now = Local::now().naive_local();
        let due = vm_for_timer.borrow().due_reminders(now).unwrap_or_default();
        for reminder in due {
            let body = format!("{} às {}", reminder.title, hm_of(reminder.start));
            let _ = notify_rust::Notification::new()
                .appname("Oxide-calendar")
                .summary("Lembrete")
                .body(&body)
                .show();
            let text = format!(
                "Lembrete: {} ({} min)",
                reminder.title, reminder.minutes_before
            );
            if let Some(ui) = weak.upgrade() {
                notify(&ui, &text);
            }
        }
    });

    reminders
}
