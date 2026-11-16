//! The app's menus as the iPad's menu bar: [`orb_ui::menu::App::menus`] is the one definition of
//! them, this turns it into the flat rows `src/menus.m` builds `UIMenu`s from, and the id the
//! chosen item sends back goes to [`orb_ui::menu::App::menu_command`].
//!
//! The rows and their strings live in this thread's state: UIKit builds and runs the menu bar on
//! the main thread, which is the one eframe draws the panel on.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use eframe::egui::Context;
use orb_ui::menu::{Menu, MenuItem, Shortcut};

/// One line of the menu bar, as `src/menus.m` reads it. Keep in step with `OrbMenuRow` there.
#[repr(C)]
pub struct Row {
    /// The menu's index in [`Menu`]'s order (0 File, 1 Mods, 2 Options, 3 Help).
    pub menu: c_int,
    /// [`Kind`].
    pub kind: c_int,
    /// 1 inside a submenu, else 0.
    pub depth: c_int,
    pub enabled: c_int,
    /// -1 when the line is not checkable, else 0 or 1.
    pub checked: c_int,
    /// 1 Cmd, 2 Shift, 4 Option.
    pub modifiers: c_int,
    pub title: *const c_char,
    pub id: *const c_char,
    /// The key of the shortcut (`""` when there is none).
    pub key: *const c_char,
}

/// What a [`Row`] is.
pub mod kind {
    pub const ACTION: i32 = 0;
    pub const CHECK: i32 = 1;
    pub const RADIO: i32 = 2;
    pub const CHOICE: i32 = 3;
    pub const SUBMENU: i32 = 4;
    pub const SEPARATOR: i32 = 5;
    pub const LABEL: i32 = 6;
    /// The first row of a menu: its title.
    pub const MENU: i32 = 7;
}

struct State {
    menus: Vec<Menu>,
    rows: Vec<Row>,
    /// The rows' strings; the `Row` pointers point into these.
    text: Vec<CString>,
    actions: Vec<String>,
    ctx: Option<Context>,
}

thread_local! {
    static STATE: RefCell<State> = const {
        RefCell::new(State {
            menus: Vec::new(),
            rows: Vec::new(),
            text: Vec::new(),
            actions: Vec::new(),
            ctx: None,
        })
    };
}

/// Hands the menus to the menu bar; true when they changed, so the bar must be rebuilt.
pub fn publish(menus: Vec<Menu>, ctx: &Context) -> bool {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.ctx = Some(ctx.clone());
        if state.menus == menus {
            return false;
        }
        state.menus = menus;
        build_rows(&mut state);
        true
    })
}

/// The ids of the items chosen since the last call.
pub fn take_actions() -> Vec<String> {
    STATE.with(|state| std::mem::take(&mut state.borrow_mut().actions))
}

fn build_rows(state: &mut State) {
    state.rows.clear();
    state.text.clear();
    let menus = std::mem::take(&mut state.menus);
    for (index, menu) in menus.iter().enumerate() {
        let at = row(state, index, kind::MENU, 0);
        state.rows[at].title = text(state, &menu.title);
        for item in &menu.items {
            push(state, index, 0, item);
        }
    }
    state.menus = menus;
}

/// Adds one row of `menu` at `depth`; returns its index.
fn row(state: &mut State, menu: usize, kind: i32, depth: i32) -> usize {
    state.rows.push(Row {
        menu: menu as c_int,
        kind,
        depth,
        enabled: 1,
        checked: -1,
        modifiers: 0,
        title: std::ptr::null(),
        id: std::ptr::null(),
        key: std::ptr::null(),
    });
    state.rows.len() - 1
}

/// Keeps `text` alive and answers the pointer the rows hold.
fn text(state: &mut State, value: &str) -> *const c_char {
    state.text.push(CString::new(value).unwrap_or_default());
    state.text.last().map_or(std::ptr::null(), |s| s.as_ptr())
}

fn push(state: &mut State, menu: usize, depth: i32, item: &MenuItem) {
    match item {
        MenuItem::Action {
            id,
            title,
            shortcut,
            enabled,
            hint: _,
        } => {
            let at = row(state, menu, kind::ACTION, depth);
            state.rows[at].title = text(state, title);
            state.rows[at].id = text(state, id);
            state.rows[at].enabled = i32::from(*enabled);
            if let Some(shortcut) = shortcut {
                state.rows[at].key = text(state, &key_of(shortcut));
                state.rows[at].modifiers = modifiers_of(shortcut);
            }
        }
        MenuItem::Check { id, title, checked } => {
            let at = row(state, menu, kind::CHECK, depth);
            state.rows[at].title = text(state, title);
            state.rows[at].id = text(state, id);
            state.rows[at].checked = i32::from(*checked);
        }
        MenuItem::Radio { id, title, checked } => {
            let at = row(state, menu, kind::RADIO, depth);
            state.rows[at].title = text(state, title);
            state.rows[at].id = text(state, id);
            state.rows[at].checked = i32::from(*checked);
        }
        MenuItem::Choice {
            id,
            title,
            selected,
        } => {
            let at = row(state, menu, kind::CHOICE, depth);
            state.rows[at].title = text(state, title);
            state.rows[at].id = text(state, id);
            state.rows[at].checked = i32::from(*selected);
        }
        MenuItem::Submenu {
            title,
            selected,
            items,
        } => {
            let at = row(state, menu, kind::SUBMENU, depth);
            state.rows[at].title = text(state, title);
            state.rows[at].checked = i32::from(*selected);
            for item in items {
                push(state, menu, depth + 1, item);
            }
        }
        MenuItem::Label(label) => {
            let at = row(state, menu, kind::LABEL, depth);
            state.rows[at].title = text(state, label);
        }
        MenuItem::Separator => {
            row(state, menu, kind::SEPARATOR, depth);
        }
    }
}

/// The key a `UIKeyCommand` takes: a single letter, lower case.
fn key_of(shortcut: &Shortcut) -> String {
    let name = shortcut.key.name();
    if name.chars().count() == 1 {
        name.to_ascii_lowercase()
    } else {
        String::new()
    }
}

fn modifiers_of(shortcut: &Shortcut) -> c_int {
    i32::from(shortcut.command) | i32::from(shortcut.shift) << 1 | i32::from(shortcut.alt) << 2
}

/// The rows, in menu order, with a [`kind::MENU`] row before each menu's items.
#[unsafe(no_mangle)]
pub extern "C" fn orb_ios_menu_rows(count: *mut c_int) -> *const Row {
    STATE.with(|state| {
        let state = state.borrow();
        if !count.is_null() {
            // SAFETY: the caller passed a pointer to its own int.
            unsafe { *count = state.rows.len() as c_int };
        }
        state.rows.as_ptr()
    })
}

/// What the menu bar chose: the item's id, queued for the panel's next frame.
#[unsafe(no_mangle)]
pub extern "C" fn orb_ios_menu_action(id: *const c_char) {
    if id.is_null() {
        return;
    }
    // SAFETY: the caller passes a NUL-terminated string that outlives the call.
    let id = unsafe { CStr::from_ptr(id) }.to_string_lossy().into_owned();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.actions.push(id);
        if let Some(ctx) = &state.ctx {
            ctx.request_repaint();
        }
    });
}
