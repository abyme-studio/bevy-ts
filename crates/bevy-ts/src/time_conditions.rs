//! Bevy's time run conditions (`bevy::time::common_conditions`), exposed through the condition
//! seam under `bevy:` names, as `sets` exposes the `bevy:` anchors. Each factory calls Bevy's own
//! function: no port. `bevy-ts` wraps each one typed (`onTimer(2)` and the rest, in `core.js`).

use crate::conditions::expose;
use bevy::ecs::schedule::BoxedCondition;
use bevy::ecs::system::ReadOnlySystem;
use bevy::prelude::*;
use bevy::time::common_conditions as bevy_time;
use std::time::Duration;

/// A Bevy condition factory that takes a `Duration`.
type Timed<S> = fn(Duration) -> S;

/// The `bevy:` name, the TS function that wraps it, and Bevy's function.
#[track_caller]
fn timed<S, M>(app: &mut App, name: &str, ts_name: &'static str, make: Timed<S>)
where
    S: IntoSystem<(), bool, M> + 'static,
    S::System: ReadOnlySystem,
{
    expose(app, name, move |args| Ok(Box::new(IntoSystem::into_system(make(seconds(ts_name, args)?))) as BoxedCondition));
}

/// `args.secs` as a `Duration`: a finite number `>= 0`. JS's `NaN` and `Infinity` arrive as `null`.
fn seconds(ts_name: &str, args: &serde_json::Value) -> Result<Duration, String> {
    let secs = &args["secs"];
    match secs.as_f64() {
        Some(s) if s.is_finite() && s >= 0.0 => Ok(Duration::from_secs_f64(s)),
        _ => Err(format!("{ts_name} takes seconds, a finite number >= 0; got {secs}")),
    }
}

/// Registers the seven conditions. Called from the plugin's `build`.
#[track_caller]
pub fn expose_all(app: &mut App) {
    timed(app, "bevy:on-timer", "onTimer", bevy_time::on_timer);
    timed(app, "bevy:on-real-timer", "onRealTimer", bevy_time::on_real_timer);
    timed(app, "bevy:once-after-delay", "onceAfterDelay", bevy_time::once_after_delay);
    timed(app, "bevy:once-after-real-delay", "onceAfterRealDelay", bevy_time::once_after_real_delay);
    timed(app, "bevy:repeating-after-delay", "repeatingAfterDelay", bevy_time::repeating_after_delay);
    timed(app, "bevy:repeating-after-real-delay", "repeatingAfterRealDelay", bevy_time::repeating_after_real_delay);
    expose(app, "bevy:paused", |_| Ok(Box::new(IntoSystem::into_system(bevy_time::paused)) as BoxedCondition));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditions::TsConditions;

    #[test]
    fn seven_conditions_and_a_checked_duration() {
        let mut app = App::new();
        expose_all(&mut app);
        let c = app.world().resource::<TsConditions>();
        assert!(c.check().is_ok());
        assert_eq!(c.names().filter(|n| n.starts_with("bevy:")).count(), 7);
        let on_timer = c.get("bevy:on-timer").unwrap().factory.clone();
        assert!(on_timer(&serde_json::json!({ "secs": 0.5 })).is_ok());
        for bad in [serde_json::json!({ "secs": -1 }), serde_json::json!({ "secs": null }), serde_json::json!({ "secs": "2" }), serde_json::json!(null)] {
            let e = on_timer(&bad).err().unwrap();
            assert!(e.starts_with("onTimer takes seconds, a finite number >= 0; got "), "{e}");
        }
        assert!(c.get("bevy:paused").unwrap().factory.as_ref()(&serde_json::Value::Null).is_ok());
    }
}
