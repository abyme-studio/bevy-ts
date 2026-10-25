//! An app with Bevy's `AudioPlugin` and nothing else that decodes: the decoders a feature turns on
//! are linked through `AudioSource`'s `Decodable`, which `AudioPlugin` registers. Never run.

use bevy::prelude::*;
use bevy_audio::{AudioPlayer, AudioPlugin};

fn main() {
    App::new()
        .add_plugins((MinimalPlugins, AssetPlugin::default(), AudioPlugin::default()))
        .add_systems(Startup, |server: Res<AssetServer>, mut commands: Commands| {
            commands.spawn(AudioPlayer::new(server.load("a.ogg")));
        })
        .run();
}
