//! When the web output's audio needs help (task-329, `docs/web.md` section 4.1): the decisions
//! of [`crate::web::WebOutput`] after the page lost focus, the phone locked or another app took
//! the audio, as a state machine without the browser, so that it is tested natively.
//!
//! The output feeds it what it sees (the AudioContext's state, the worklet's messages, the page
//! shown, hidden or unloaded, the user's gestures, a periodic tick, all with the time in
//! milliseconds) and does what it answers ([`Action`]): nothing, `resume()` the context, leave
//! (close the context and stop the song), or rebuild the context and the worklet. The rules
//! (the user's decision of 2026-11-28: the song stops only where recovery needs it):
//!
//! - A hidden page goes on playing. Only where [`Recovery::stop_on_hidden`] is set does it stop
//!   (leave), and an unloaded page always does (`pagehide` that is not persisted).
//! - Back on the page a context that ran and no longer runs is resumed without a gesture: when that
//!   brings it back the song just goes on.
//! - A context that ran and does not run again within [`RESUME_MS`] while the page is visible is
//!   broken; so is a closed context, and a running one whose worklet posts nothing for [`STALL_MS`]
//!   while the page is visible (the audio thread stopped calling `process()`). The output then
//!   shows the song stopped at once, since nothing is heard.
//! - At a gesture a broken output, and a context that ran once and no longer runs (iOS's
//!   "interrupted", a suspended context), are rebuilt: in the gesture, because iOS lets a new
//!   context start only there. A context that never ran (the browser's autoplay rule before the
//!   first gesture) is resumed, and is broken if that does not take within [`RESUME_MS`].
//! - Nothing is judged while the page is hidden.

/// The worklet's silence, while its context runs, after which it is dead (it posts a snapshot
/// every engine call, 23 ms, and its statistics every second).
pub const STALL_MS: f64 = 1500.0;
/// How long a gesture's `resume()` may take to bring the context to running.
pub const RESUME_MS: f64 = 1500.0;
/// The period of [`Recovery::tick`] the output uses.
pub const TICK_MS: f64 = 500.0;

/// An AudioContext's state (`AudioContext.state`; "interrupted" is iOS Safari's own).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtxState {
    Running,
    Suspended,
    Interrupted,
    Closed,
}

impl CtxState {
    /// The state of `AudioContext.state`'s string; another string counts as suspended.
    pub fn parse(s: &str) -> CtxState {
        match s {
            "running" => CtxState::Running,
            "interrupted" => CtxState::Interrupted,
            "closed" => CtxState::Closed,
            _ => CtxState::Suspended,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CtxState::Running => "running",
            CtxState::Suspended => "suspended",
            CtxState::Interrupted => "interrupted",
            CtxState::Closed => "closed",
        }
    }
}

/// What the output does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// `resume()` the context.
    Resume,
    /// Close the context and stop the song: the page was hidden.
    Leave,
    /// A new context and worklet, with the song, the mod and the position given again.
    Rebuild,
}

/// Why the output is broken (the status and the console say it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Broken {
    /// The context was closed.
    Closed,
    /// The context runs, but the worklet posts nothing.
    Silent,
    /// `resume()` did not bring the context to running.
    NoResume,
    /// The page was hidden, and the context closed with it.
    Left,
    /// The new context and worklet could not be given the song (it does not convert or load).
    Restore,
}

impl Broken {
    pub fn as_str(self) -> &'static str {
        match self {
            Broken::Closed => "the audio context was closed",
            Broken::Silent => "the audio worklet stopped",
            Broken::NoResume => "the audio context did not resume",
            Broken::Left => "the page was left",
            Broken::Restore => "the song could not be given to the new audio worklet",
        }
    }
}

/// Whether the page runs in iOS or iPadOS WebKit (every browser there, and the in-app
/// browsers), where a hidden page stops the song ([`Recovery::stop_on_hidden`]; the user's
/// decision of 2026-11-28): an iPhone, iPad or iPod user agent, or iPadOS's desktop-class Mac
/// user agent with a touch screen (`navigator.maxTouchPoints` above 1; a Mac has 0).
pub fn ios_webkit(user_agent: &str, max_touch_points: i32) -> bool {
    ["iPhone", "iPad", "iPod"]
        .iter()
        .any(|d| user_agent.contains(d))
        || (user_agent.contains("Macintosh") && max_touch_points > 1)
}

/// The state machine of one output (one context and worklet; [`Recovery::rebuilt`] starts it
/// again for the next).
#[derive(Clone, Debug)]
pub struct Recovery {
    state: CtxState,
    visible: bool,
    /// The context has run (it was not only waiting for the first gesture).
    ran: bool,
    broken: Option<Broken>,
    /// When a gesture's `resume()` was asked, while it has not brought the context to running.
    resume_at: Option<f64>,
    /// Since when a context that ran has not been running while the page is visible.
    stopped_at: Option<f64>,
    /// The last message of the worklet, or the time from which its silence counts (the context
    /// started running, the page was shown).
    heard: f64,
    /// A hidden page stops the song ([`Action::Leave`]).
    pub stop_on_hidden: bool,
}

impl Recovery {
    /// A new output whose context is in `state` at `now`, the page visible or not.
    pub fn new(state: CtxState, visible: bool, now: f64) -> Recovery {
        let mut r = Recovery {
            state,
            visible,
            ran: false,
            broken: None,
            resume_at: None,
            stopped_at: None,
            heard: now,
            stop_on_hidden: false,
        };
        r.context(state, now);
        r
    }

    /// The context's state now (its `statechange`, or read when convenient).
    pub fn context(&mut self, s: CtxState, now: f64) {
        if s == CtxState::Running && self.state != CtxState::Running {
            self.heard = now;
        }
        self.state = s;
        match s {
            CtxState::Running => {
                self.ran = true;
                self.resume_at = None;
                self.stopped_at = None;
            }
            CtxState::Closed => {
                self.broken.get_or_insert(Broken::Closed);
            }
            CtxState::Suspended | CtxState::Interrupted => {
                if self.ran && self.stopped_at.is_none() {
                    self.stopped_at = Some(now);
                }
            }
        }
    }

    /// The worklet posted a message.
    pub fn heard(&mut self, now: f64) {
        self.heard = now;
    }

    /// The page was shown (`visibilitychange` to visible, `pageshow`, `focus`) or hidden
    /// (`visibilitychange` to hidden, a persisted `pagehide`, `freeze`). Shown again, a context
    /// that ran and no longer runs is resumed; hidden, the song goes on unless
    /// [`Recovery::stop_on_hidden`] (then [`Action::Leave`], and the output is broken).
    pub fn visibility(&mut self, visible: bool, now: f64) -> Action {
        let shown = visible && !self.visible;
        self.visible = visible;
        if !visible {
            self.resume_at = None;
            if self.stop_on_hidden {
                self.broken = Some(Broken::Left);
                return Action::Leave;
            }
            return Action::None;
        }
        if shown {
            // the silence and the stop while hidden do not count: they count from now
            self.heard = now;
            if self.stopped_at.is_some() {
                self.stopped_at = Some(now);
            }
        }
        if self.broken.is_none() && self.ran && self.state != CtxState::Running {
            self.stopped_at.get_or_insert(now);
            Action::Resume
        } else {
            Action::None
        }
    }

    /// The owner could not give the rebuilt output the song: the output is broken, so that the
    /// song shows stopped and the next gesture rebuilds again.
    pub fn restore_failed(&mut self) {
        self.broken.get_or_insert(Broken::Restore);
    }

    /// The page is unloaded (`pagehide` that is not persisted): the song stops, the context is
    /// closed ([`Action::Leave`]) and the output is broken.
    pub fn unload(&mut self) -> Action {
        self.visible = false;
        self.resume_at = None;
        self.broken = Some(Broken::Left);
        Action::Leave
    }

    /// A user gesture (a click, a key, a touch) at `now`.
    pub fn gesture(&mut self, now: f64) -> Action {
        if !self.visible {
            return Action::None;
        }
        self.activation(now)
    }

    /// The system's Play (the Media Session's play action: Control Center, a lock screen, a
    /// headset's button), which may come while the page is hidden: as [`Recovery::gesture`],
    /// whatever the page's visibility.
    pub fn media_play(&mut self, now: f64) -> Action {
        self.activation(now)
    }

    fn activation(&mut self, now: f64) -> Action {
        if self.broken.is_some() || (self.ran && self.state != CtxState::Running) {
            return Action::Rebuild;
        }
        if self.state == CtxState::Running {
            return Action::None;
        }
        self.resume_at.get_or_insert(now);
        Action::Resume
    }

    /// The periodic check ([`TICK_MS`]).
    pub fn tick(&mut self, now: f64) {
        if !self.visible || self.broken.is_some() {
            return;
        }
        if self.state == CtxState::Running && now - self.heard > STALL_MS {
            self.broken = Some(Broken::Silent);
        }
        let late = |at: Option<f64>| at.is_some_and(|t| now - t > RESUME_MS);
        if self.state != CtxState::Running && (late(self.resume_at) || late(self.stopped_at)) {
            self.broken = Some(Broken::NoResume);
        }
    }

    /// The output was rebuilt: a new context in `state`.
    pub fn rebuilt(&mut self, state: CtxState, now: f64) {
        let stop_on_hidden = self.stop_on_hidden;
        *self = Recovery::new(state, self.visible, now);
        self.stop_on_hidden = stop_on_hidden;
    }

    pub fn broken(&self) -> Option<Broken> {
        self.broken
    }

    pub fn state(&self) -> CtxState {
        self.state
    }

    pub fn visible(&self) -> bool {
        self.visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(now: f64) -> Recovery {
        Recovery::new(CtxState::Running, true, now)
    }

    /// Before the first gesture the context is suspended and never ran: a gesture resumes it,
    /// and only a resume that does not take makes it broken.
    #[test]
    fn the_first_gesture_resumes() {
        let mut r = Recovery::new(CtxState::Suspended, true, 0.0);
        r.tick(10_000.0);
        assert_eq!(r.broken(), None, "no gesture yet: nothing to judge");
        assert_eq!(r.gesture(10_000.0), Action::Resume);
        assert_eq!(r.gesture(10_100.0), Action::Resume);
        r.context(CtxState::Running, 10_200.0);
        r.tick(10_300.0);
        assert_eq!(r.broken(), None);
        assert_eq!(r.gesture(10_400.0), Action::None);

        let mut r = Recovery::new(CtxState::Suspended, true, 0.0);
        assert_eq!(r.gesture(100.0), Action::Resume);
        r.tick(100.0 + RESUME_MS - 1.0);
        assert_eq!(r.broken(), None);
        r.tick(100.0 + RESUME_MS + 1.0);
        assert_eq!(r.broken(), Some(Broken::NoResume));
        assert_eq!(r.gesture(2000.0), Action::Rebuild);
    }

    /// A hidden page plays on: nothing stops, nothing is judged; back on the page a running
    /// context and a rendering worklet need nothing.
    #[test]
    fn a_hidden_page_plays_on() {
        let mut r = running(0.0);
        assert_eq!(r.visibility(false, 1000.0), Action::None);
        r.tick(60_000.0);
        assert_eq!(r.broken(), None);
        assert_eq!(r.visibility(true, 60_000.0), Action::None);
        r.heard(60_020.0);
        r.tick(60_500.0);
        assert_eq!(r.broken(), None);
        assert_eq!(r.gesture(60_600.0), Action::None);
    }

    /// iOS: the context is interrupted while hidden. Back on the page it is resumed without a
    /// gesture; when that takes, the song goes on and nothing is rebuilt.
    #[test]
    fn a_resume_on_return_that_takes() {
        let mut r = running(0.0);
        r.visibility(false, 1000.0);
        r.context(CtxState::Interrupted, 1001.0);
        assert_eq!(r.visibility(true, 60_000.0), Action::Resume);
        r.context(CtxState::Running, 60_100.0);
        r.heard(60_120.0);
        r.tick(60_000.0 + RESUME_MS + 1.0);
        assert_eq!(r.broken(), None);
        assert_eq!(r.gesture(61_600.0), Action::None);
    }

    /// The resume on return does not take (iOS without a gesture): broken after
    /// [`RESUME_MS`], counted from the return, so the song shows stopped; the next gesture
    /// rebuilds.
    #[test]
    fn a_resume_on_return_that_does_not_take() {
        let mut r = running(0.0);
        r.visibility(false, 1000.0);
        r.context(CtxState::Interrupted, 1001.0);
        assert_eq!(r.visibility(true, 60_000.0), Action::Resume);
        r.tick(60_000.0 + RESUME_MS - 1.0);
        assert_eq!(r.broken(), None, "the time hidden does not count");
        r.tick(60_000.0 + RESUME_MS + 1.0);
        assert_eq!(r.broken(), Some(Broken::NoResume));
        assert_eq!(r.gesture(62_000.0), Action::Rebuild);
    }

    /// A tap before the resume on return has decided: a rebuild, in the gesture.
    #[test]
    fn a_gesture_before_the_resume_decides_rebuilds() {
        let mut r = running(0.0);
        r.visibility(false, 1000.0);
        r.context(CtxState::Suspended, 1001.0);
        assert_eq!(r.visibility(true, 60_000.0), Action::Resume);
        assert_eq!(r.gesture(60_100.0), Action::Rebuild);
    }

    /// Where a hidden page stops the song, it leaves: broken, and the next gesture rebuilds.
    #[test]
    fn stop_on_hidden_leaves() {
        let mut r = running(0.0);
        r.stop_on_hidden = true;
        assert_eq!(r.visibility(false, 1000.0), Action::Leave);
        assert_eq!(r.broken(), Some(Broken::Left));
        r.context(CtxState::Closed, 1001.0);
        assert_eq!(r.broken(), Some(Broken::Left), "the reason stays");
        assert_eq!(r.visibility(true, 60_000.0), Action::None);
        assert_eq!(r.gesture(60_100.0), Action::Rebuild);
        r.rebuilt(CtxState::Running, 60_200.0);
        assert!(r.stop_on_hidden, "kept across a rebuild");
    }

    /// An unload always leaves.
    #[test]
    fn an_unload_leaves() {
        let mut r = running(0.0);
        assert_eq!(r.unload(), Action::Leave);
        assert_eq!(r.broken(), Some(Broken::Left));
    }

    /// A context suspended or interrupted while the page is visible (a call, Siri, another app's
    /// audio): broken when it does not come back, and the next gesture rebuilds.
    #[test]
    fn a_suspension_while_visible() {
        let mut r = running(0.0);
        r.context(CtxState::Interrupted, 500.0);
        r.tick(500.0 + RESUME_MS - 1.0);
        assert_eq!(r.broken(), None);
        r.tick(500.0 + RESUME_MS + 1.0);
        assert_eq!(r.broken(), Some(Broken::NoResume));
        assert_eq!(r.gesture(10_000.0), Action::Rebuild);

        let mut r = running(0.0);
        r.context(CtxState::Suspended, 500.0);
        assert_eq!(r.gesture(600.0), Action::Rebuild, "a tap rebuilds at once");
    }

    /// The worklet stops posting while its context runs: broken after the stall.
    #[test]
    fn a_silent_worklet_is_broken() {
        let mut r = running(0.0);
        for t in 1..10 {
            r.heard(f64::from(t) * 23.0);
        }
        r.tick(207.0 + STALL_MS - 1.0);
        assert_eq!(r.broken(), None);
        r.tick(207.0 + STALL_MS + 1.0);
        assert_eq!(r.broken(), Some(Broken::Silent));
        assert_eq!(r.gesture(3000.0), Action::Rebuild);
    }

    /// The system's Play rebuilds a broken output even while the page is hidden, where a
    /// gesture cannot come.
    #[test]
    fn a_system_play_while_hidden() {
        let mut r = running(0.0);
        r.visibility(false, 1000.0);
        r.context(CtxState::Closed, 1001.0);
        assert_eq!(r.gesture(2000.0), Action::None);
        assert_eq!(r.media_play(2000.0), Action::Rebuild);
        let mut r = running(0.0);
        r.visibility(false, 1000.0);
        assert_eq!(
            r.media_play(2000.0),
            Action::None,
            "a running context needs nothing"
        );
    }

    /// A rebuilt output that could not be given the song looks healthy (running, posting), but
    /// is broken: the next gesture rebuilds again (task-329's review round 2).
    #[test]
    fn a_failed_restore_rebuilds_again() {
        let mut r = running(0.0);
        r.context(CtxState::Closed, 100.0);
        r.rebuilt(CtxState::Running, 200.0);
        assert_eq!(r.gesture(300.0), Action::None);
        r.restore_failed();
        assert_eq!(r.broken(), Some(Broken::Restore));
        r.heard(400.0);
        r.tick(500.0);
        assert_eq!(r.broken(), Some(Broken::Restore));
        assert_eq!(r.gesture(600.0), Action::Rebuild);
    }

    /// A closed context is broken at once.
    #[test]
    fn a_closed_context() {
        let mut r = running(0.0);
        r.context(CtxState::Closed, 100.0);
        assert_eq!(r.broken(), Some(Broken::Closed));
        assert_eq!(r.gesture(300.0), Action::Rebuild);
    }

    /// After a rebuild the machine starts again: the new context, made in a gesture, may run
    /// at once or wait for the next gesture's resume.
    #[test]
    fn a_rebuild_starts_again() {
        let mut r = running(0.0);
        r.context(CtxState::Closed, 100.0);
        r.rebuilt(CtxState::Suspended, 200.0);
        assert_eq!(r.broken(), None);
        assert_eq!(r.gesture(300.0), Action::Resume);
        r.context(CtxState::Running, 310.0);
        assert_eq!(r.gesture(400.0), Action::None);
    }

    /// The platform of a hidden page that stops.
    #[test]
    fn ios_webkit_by_user_agent() {
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 \
                      (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1";
        let ipados = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                      (KHTML, like Gecko) Version/18.1 Safari/605.1.15";
        let chrome_ios = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) \
                          AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/131.0 Mobile/15E148 \
                          Safari/604.1";
        let android = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like \
                       Gecko) Chrome/131.0 Mobile Safari/537.36";
        let mac_chrome = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/157.0 Safari/537.36";
        assert!(ios_webkit(iphone, 5));
        assert!(ios_webkit(chrome_ios, 5));
        assert!(ios_webkit(ipados, 5), "iPadOS asks for the desktop site");
        assert!(!ios_webkit(ipados, 0), "a Mac's Safari");
        assert!(!ios_webkit(mac_chrome, 0));
        assert!(!ios_webkit(android, 5));
    }

    #[test]
    fn the_state_strings() {
        for s in [
            CtxState::Running,
            CtxState::Suspended,
            CtxState::Interrupted,
            CtxState::Closed,
        ] {
            assert_eq!(CtxState::parse(s.as_str()), s);
        }
        assert_eq!(CtxState::parse("something new"), CtxState::Suspended);
    }
}
