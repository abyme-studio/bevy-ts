//! `bevy-ts`: the prebuilt engine binary. A project engine's generated `main` makes the same
//! call with its identity and the project's plugins.

fn main() {
    bevy_ts_cli::run(bevy_ts_cli::Prebuilt);
}
