//! The notice queue behind the dynamic island chip.
//!
//! The chip normally shows the clock. Whenever something happens that the user
//! should know about (a tile was started, a start failed, the launchpad is empty
//! but a continuous launch was requested, ...) the caller posts a notice here.
//! Notices are *not* shown on top of each other: the first one replaces the
//! clock for [`NOTICE_DURATION`], the next one starts after it disappeared, and
//! when the queue is empty the clock comes back.
//!
//! The queue owns the timer, so nothing else has to know how the sequence is
//! driven; the only requirement is that the [`IslandRef`] is kept alive for as
//! long as the window exists (it is dropped together with the window).

use crate::logs;
use crate::windows::main_window::MainWindow;
use slint::{ComponentHandle, Timer, Weak};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

/// Log location shared by every line of this module.
const LOG_LOCATION: &str = "windows::dynamic_island";

/// How long one notice stays visible before the next one (or the clock) shows.
pub const NOTICE_DURATION: Duration = Duration::from_millis(3_000);

/// One line shown in the island instead of the clock.
struct Notice {
    /// Already localised text of the notice.
    text: String,
    /// `true` paints the text red (a failure), `false` black (a success).
    error: bool,
}

/// State of the island: what waits in the queue, whether something is on screen
/// right now, and when the current notice ends.
pub struct Island {
    /// Window the notices are shown in; upgraded for every update so a late
    /// timer callback cannot touch a destroyed window.
    app: Weak<MainWindow>,
    /// Notices waiting for their turn, oldest first.
    pending: VecDeque<Notice>,
    /// True while a notice is on screen (and thus while the clock is paused).
    showing: bool,
    /// Single shot timer that advances the queue.
    timer: Timer,
    /// Callback that restores the clock when the queue ran empty. It is a plain
    /// function pointer so this module does not have to know how the clock is
    /// formatted.
    idle: fn(&MainWindow),
}

/// Shared handle of the island state.
pub type IslandRef = Rc<RefCell<Island>>;

/// Creates the queue for `app`; `idle` is called whenever the island goes quiet
/// again (it writes the clock into the chip).
pub fn new(app: &MainWindow, idle: fn(&MainWindow)) -> IslandRef {
    Rc::new(RefCell::new(Island {
        app: app.as_weak(),
        pending: VecDeque::new(),
        showing: false,
        timer: Timer::default(),
        idle,
    }))
}

/// True while a notice is on screen, i.e. while the clock must not overwrite it.
pub fn is_busy(island: &IslandRef) -> bool {
    island.borrow().showing
}

/// Posts a notice.
///
/// When the island is idle the notice is shown immediately, otherwise it waits
/// behind the notices that are already queued - the caller never has to think
/// about timing.
pub fn notify(island: &IslandRef, text: impl Into<String>, error: bool) {
    let notice = Notice {
        text: text.into(),
        error,
    };

    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!(
            "Notice queued ({}): \"{}\".",
            if notice.error { "error" } else { "info" },
            notice.text
        ),
        "info",
    );

    // The queue is only touched to push and to read `showing`; the borrowing
    // ends before `advance` runs, so the timer callback below can borrow again.
    let start_now = {
        let mut state = island.borrow_mut();
        state.pending.push_back(notice);
        !state.showing
    };
    if start_now {
        island.borrow_mut().showing = true;
        advance(island);
    }
}

/// Pops the next notice and shows it, or restores the clock when none is left.
///
/// Called once when a notice is posted to an idle island and once per timer
/// expiry afterwards.
fn advance(island: &IslandRef) {
    let next = island.borrow_mut().pending.pop_front();

    let Some(notice) = next else {
        // Queue empty: the clock takes over again. `showing` is cleared first so
        // the idle callback is allowed to write into the chip.
        let idle = {
            let mut state = island.borrow_mut();
            state.showing = false;
            state.idle
        };
        // The weak handle is copied out first: the borrow of the state must not
        // be held while the idle callback runs, otherwise a callback that wants
        // to post a notice itself would panic on the borrow.
        let window = island.borrow().app.clone();
        if let Some(app) = window.upgrade() {
            // The red/green flag belongs to the notice, not to the clock, so it
            // has to be cleared before the clock text goes back in.
            app.set_island_error(false);
            idle(&app);
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "Notice queue empty, the clock is shown again.",
                "info",
            );
        }
        return;
    };

    let window = island.borrow().app.clone();
    let Some(app) = window.upgrade() else {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "The window vanished, dropping the remaining notices.",
            "warn",
        );
        island.borrow_mut().pending.clear();
        island.borrow_mut().showing = false;
        return;
    };

    // Two properties only: the text and whether it is a failure. The chip itself
    // animates its width, so no size is passed from here.
    app.set_dynamic_island_text(notice.text.as_str().into());
    app.set_island_error(notice.error);
    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!("Notice shown: \"{}\".", notice.text),
        "info",
    );

    arm_timer(island);
}

/// Starts the single shot timer that ends the currently shown notice.
fn arm_timer(island: &IslandRef) {
    // A clone of the handle, so the callback owns what it needs; the timer itself
    // lives in the state that stays alive with the window.
    let next_state = island.clone();
    let state = island.borrow();
    state.timer.start(
        slint::TimerMode::SingleShot,
        NOTICE_DURATION,
        move || {
            advance(&next_state);
        },
    );
}
