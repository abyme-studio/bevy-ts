//! The V8 inspector over CDP, for `bevy-ts dev --inspect`.

use crate::V8Backend;

pub(crate) struct InspectorServer {
    address: std::net::SocketAddr,
}

impl InspectorServer {
    pub(crate) fn address(&self) -> std::net::SocketAddr {
        self.address
    }
}

impl V8Backend {
    pub(crate) fn start_inspector(&mut self) {
        if let Some(address) = self.options.inspect {
            self.inspector = Some(InspectorServer { address });
        }
    }

    pub(crate) fn stop_inspector(&mut self) {
        self.inspector = None;
    }

    pub(crate) fn pump_inspector(&mut self) {}
}
