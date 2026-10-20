//! The viewport runner: no winit, no window. While a viewer is attached and visible, one update
//! per `tick` from the leading viewer's display link (the game's while it is visible, else the
//! first visible preview's), paced at the attach's `hz` before the first tick or
//! when ticks stop for 100 ms. Hidden or detached, play keeps updating at 60 Hz without rendering
//! and edit updates only when something arrives (input, a channel message) or every 100 ms to
//! serve protocol requests and reloads.

use crate::shared::{Shared, SharedRes, TICK_TIMEOUT};
use bevy::app::{AppExit, PluginsState};
use bevy::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Hidden play mode's rate.
const HIDDEN_HZ: f64 = 60.0;
/// How often an idle edit mode wakes to serve protocol requests.
const IDLE_POLL: Duration = Duration::from_millis(100);

/// Why the runner updates now.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Wake {
    Tick,
    Timer,
    Message,
}

fn wait(shared: &Shared, next_timer: Instant) -> Wake {
    let mut w = shared.hub.wake.lock().unwrap();
    loop {
        if w.tick {
            w.tick = false;
            w.other = false;
            return Wake::Tick;
        }
        let now = Instant::now();
        let rendering = shared.any_rendering();
        let ticking = w.last_tick.is_some_and(|t| now.duration_since(t) < TICK_TIMEOUT) && rendering;
        if w.other && !ticking {
            w.other = false;
            return Wake::Message;
        }
        // While ticks come, the timer only takes over once they stop.
        let deadline = if ticking { w.last_tick.unwrap() + TICK_TIMEOUT } else { next_timer };
        if now >= deadline {
            if !ticking {
                return Wake::Timer;
            }
            continue;
        }
        w = shared.hub.wake_cv.wait_timeout(w, deadline - now).unwrap().0;
    }
}

/// Runs the app until it exits. Set with `App::set_runner`.
pub fn runner(mut app: App) -> AppExit {
    if app.plugins_state() != PluginsState::Cleaned {
        while app.plugins_state() == PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
    }
    let shared: Arc<Shared> = app.world().resource::<SharedRes>().0.clone();
    let mut last = Instant::now();
    loop {
        let period = if shared.any_rendering() {
            Duration::from_secs_f64(1.0 / shared.pace_hz())
        } else if shared.edit() {
            IDLE_POLL
        } else {
            Duration::from_secs_f64(1.0 / HIDDEN_HZ)
        };
        let why = wait(&shared, last + period);
        last = Instant::now();
        shared.hub.wake.lock().unwrap().updating = true;
        let started = Instant::now();
        crate::trace::mark("update", why as u64, 0);
        app.update();
        crate::trace::mark("updated", 0, 0);
        shared.stats.add_update(started.elapsed().as_nanos() as u64);
        shared.hub.wake.lock().unwrap().updating = false;
        if let Some(exit) = app.should_exit() {
            return exit;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waiting_runner_wakes_on_the_timer_with_no_viewer() {
        let shared = Shared::default();
        let t0 = Instant::now();
        assert_eq!(wait(&shared, t0 + Duration::from_millis(20)), Wake::Timer);
        assert!(t0.elapsed() >= Duration::from_millis(20));
    }

    #[test]
    fn a_message_wakes_an_idle_runner() {
        let shared = Arc::new(Shared::default());
        let s = shared.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            s.poke();
        });
        let t0 = Instant::now();
        assert_eq!(wait(&shared, t0 + Duration::from_secs(5)), Wake::Message);
        assert!(t0.elapsed() < Duration::from_secs(1));
    }
}
