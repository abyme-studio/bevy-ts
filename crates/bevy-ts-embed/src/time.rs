//! `bevy-ts/time`, a built-in module every engine ships: the clocks TS writes, under Bevy's
//! spellings. `ResMut<Time<Virtual>>` is `VirtualTime` and `ResMut<Time<Fixed>>` is `FixedTime`,
//! exported params over Bevy's own resources (docs/time.md). Each method calls Bevy's; what Bevy
//! asserts is checked first and thrown with Bevy's message, so a bad value leaves the clock as it
//! was. The getters are the clock's stage-1 reads, live.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::time::{Fixed, Virtual};
use bevy_ts_native::prelude::*;
use std::time::Duration;

pub const MODULE: &str = "bevy-ts/time";
/// The module exports nothing to import: scripts take the params as `ResMut<Time<Virtual>>` and
/// `ResMut<Time<Fixed>>`. Their declarations come from `#[export]`.
const JS: &str = "// `bevy-ts/time`: VirtualTime and FixedTime, taken as ResMut<Time<Virtual>> and ResMut<Time<Fixed>>.\n";

/// Registers the module and its two params.
pub struct ScriptTimePlugin;

impl Plugin for ScriptTimePlugin {
    fn build(&self, app: &mut App) {
        app.ts_param::<VirtualClock<'static>>().ts_param::<FixedClock<'static>>().ts_module(MODULE, JS, "");
    }
}

/// `value` seconds as a `Duration`: a finite number `>= 0` that fits one.
fn secs(value: f64) -> Result<Duration, String> {
    Duration::try_from_secs_f64(value).map_err(|_| format!("takes seconds, a finite number >= 0; got {}", number(value)))
}

/// A number as JS prints it, near enough: `-1`, `0.5`, `1e300`, `NaN`.
fn number(v: f64) -> String {
    if v.is_finite() && v.abs() >= 1e21 { format!("{v:e}") } else { format!("{v}") }
}

/// The seconds `set_timestep_seconds(seconds)` sets, checked as Bevy asserts, and against
/// `Duration`'s range, where Bevy would panic too.
fn timestep_seconds(seconds: f64) -> Result<Duration, String> {
    if !seconds.is_sign_positive() {
        return Err("seconds less than or equal to zero".into());
    }
    if !seconds.is_finite() {
        return Err("seconds is infinite".into());
    }
    let d = Duration::try_from_secs_f64(seconds).map_err(|_| format!("seconds is too large for a timestep; got {}", number(seconds)))?;
    if d.is_zero() {
        return Err("attempted to set fixed timestep to zero".into());
    }
    Ok(d)
}

/// `Time<Virtual>`'s writes: `ResMut<Time<Virtual>>` in TS.
#[derive(SystemParam)]
pub struct VirtualClock<'w> {
    time: ResMut<'w, Time<Virtual>>,
}

#[export(ts = "VirtualTime", module = "bevy-ts/time", stands_for = "ResMut<Time<Virtual>>")]
impl VirtualClock<'_> {
    pub fn pause(&mut self) {
        self.time.pause()
    }

    pub fn unpause(&mut self) {
        self.time.unpause()
    }

    pub fn toggle(&mut self) {
        self.time.toggle()
    }

    pub fn set_relative_speed(&mut self, ratio: f32) -> Result<(), String> {
        relative_speed(ratio as f64)?;
        self.time.set_relative_speed(ratio);
        Ok(())
    }

    pub fn set_relative_speed_f64(&mut self, ratio: f64) -> Result<(), String> {
        relative_speed(ratio)?;
        self.time.set_relative_speed_f64(ratio);
        Ok(())
    }

    pub fn set_max_delta(&mut self, seconds: f64) -> Result<(), String> {
        let d = secs(seconds)?;
        if d.is_zero() {
            return Err("tried to set max delta to zero".into());
        }
        self.time.set_max_delta(d);
        Ok(())
    }

    pub fn set_wrap_period(&mut self, seconds: f64) -> Result<(), String> {
        let d = wrap_period(seconds)?;
        self.time.set_wrap_period(d);
        Ok(())
    }

    #[getter]
    pub fn delta_secs(&self) -> f32 {
        self.time.delta_secs()
    }
    #[getter]
    pub fn elapsed_secs(&self) -> f32 {
        self.time.elapsed_secs()
    }
    #[getter]
    pub fn elapsed_secs_wrapped(&self) -> f32 {
        self.time.elapsed_secs_wrapped()
    }
    #[getter]
    pub fn delta_secs_f64(&self) -> f64 {
        self.time.delta_secs_f64()
    }
    #[getter]
    pub fn elapsed_secs_f64(&self) -> f64 {
        self.time.elapsed_secs_f64()
    }
    #[getter]
    pub fn elapsed_secs_wrapped_f64(&self) -> f64 {
        self.time.elapsed_secs_wrapped_f64()
    }
    #[getter]
    pub fn wrap_period(&self) -> f64 {
        self.time.wrap_period().as_secs_f64()
    }
    #[getter]
    pub fn is_paused(&self) -> bool {
        self.time.is_paused()
    }
    #[getter]
    pub fn was_paused(&self) -> bool {
        self.time.was_paused()
    }
    #[getter]
    pub fn relative_speed(&self) -> f32 {
        self.time.relative_speed()
    }
    #[getter]
    pub fn relative_speed_f64(&self) -> f64 {
        self.time.relative_speed_f64()
    }
    #[getter]
    pub fn effective_speed(&self) -> f32 {
        self.time.effective_speed()
    }
    #[getter]
    pub fn effective_speed_f64(&self) -> f64 {
        self.time.effective_speed_f64()
    }
    #[getter]
    pub fn max_delta(&self) -> f64 {
        self.time.max_delta().as_secs_f64()
    }
}

/// Bevy's `set_relative_speed_f64` asserts.
fn relative_speed(ratio: f64) -> Result<(), String> {
    if !ratio.is_finite() {
        return Err("tried to go infinitely fast".into());
    }
    if ratio < 0.0 {
        return Err("tried to go back in time".into());
    }
    Ok(())
}

/// Bevy's `set_wrap_period` asserts.
fn wrap_period(seconds: f64) -> Result<Duration, String> {
    let d = secs(seconds)?;
    if d.is_zero() {
        return Err("division by zero".into());
    }
    Ok(d)
}

/// `Time<Fixed>`'s writes: `ResMut<Time<Fixed>>` in TS.
#[derive(SystemParam)]
pub struct FixedClock<'w> {
    time: ResMut<'w, Time<Fixed>>,
}

#[export(ts = "FixedTime", module = "bevy-ts/time", stands_for = "ResMut<Time<Fixed>>")]
impl FixedClock<'_> {
    pub fn set_timestep(&mut self, seconds: f64) -> Result<(), String> {
        let d = secs(seconds)?;
        if d.is_zero() {
            return Err("attempted to set fixed timestep to zero".into());
        }
        self.time.set_timestep(d);
        Ok(())
    }

    pub fn set_timestep_seconds(&mut self, seconds: f64) -> Result<(), String> {
        timestep_seconds(seconds)?;
        self.time.set_timestep_seconds(seconds);
        Ok(())
    }

    pub fn set_timestep_hz(&mut self, hz: f64) -> Result<(), String> {
        if !hz.is_sign_positive() {
            return Err("Hz less than or equal to zero".into());
        }
        if !hz.is_finite() {
            return Err("Hz is infinite".into());
        }
        timestep_seconds(1.0 / hz)?;
        self.time.set_timestep_hz(hz);
        Ok(())
    }

    pub fn discard_overstep(&mut self, seconds: f64) -> Result<(), String> {
        let d = secs(seconds)?;
        self.time.discard_overstep(d);
        Ok(())
    }

    pub fn set_wrap_period(&mut self, seconds: f64) -> Result<(), String> {
        let d = wrap_period(seconds)?;
        self.time.set_wrap_period(d);
        Ok(())
    }

    #[getter]
    pub fn delta_secs(&self) -> f32 {
        self.time.delta_secs()
    }
    #[getter]
    pub fn elapsed_secs(&self) -> f32 {
        self.time.elapsed_secs()
    }
    #[getter]
    pub fn elapsed_secs_wrapped(&self) -> f32 {
        self.time.elapsed_secs_wrapped()
    }
    #[getter]
    pub fn delta_secs_f64(&self) -> f64 {
        self.time.delta_secs_f64()
    }
    #[getter]
    pub fn elapsed_secs_f64(&self) -> f64 {
        self.time.elapsed_secs_f64()
    }
    #[getter]
    pub fn elapsed_secs_wrapped_f64(&self) -> f64 {
        self.time.elapsed_secs_wrapped_f64()
    }
    #[getter]
    pub fn wrap_period(&self) -> f64 {
        self.time.wrap_period().as_secs_f64()
    }
    #[getter]
    pub fn timestep(&self) -> f64 {
        self.time.timestep().as_secs_f64()
    }
    #[getter]
    pub fn overstep(&self) -> f64 {
        self.time.overstep().as_secs_f64()
    }
    #[getter]
    pub fn overstep_fraction(&self) -> f32 {
        self.time.overstep_fraction()
    }
    #[getter]
    pub fn overstep_fraction_f64(&self) -> f64 {
        self.time.overstep_fraction_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every input Bevy asserts on is refused before the call, with Bevy's message.
    #[test]
    fn checks_match_bevys_asserts() {
        assert_eq!(timestep_seconds(-1.0).unwrap_err(), "seconds less than or equal to zero");
        assert_eq!(timestep_seconds(f64::INFINITY).unwrap_err(), "seconds is infinite");
        assert_eq!(timestep_seconds(0.0).unwrap_err(), "attempted to set fixed timestep to zero");
        assert_eq!(timestep_seconds(1e300).unwrap_err(), "seconds is too large for a timestep; got 1e300");
        assert_eq!(timestep_seconds(0.5), Ok(Duration::from_millis(500)));
        assert_eq!(relative_speed(-0.5).unwrap_err(), "tried to go back in time");
        assert_eq!(relative_speed(f64::NAN).unwrap_err(), "tried to go infinitely fast");
        assert_eq!(wrap_period(0.0).unwrap_err(), "division by zero");
        assert!(secs(-1.0).unwrap_err().starts_with("takes seconds"));
    }
}
