use crate::gateway::GatewayInner;
use std::sync::{atomic::Ordering, Arc};

const MAX_CONNECTIONS: usize = 32;

pub(super) struct ConnectionGuard(Arc<GatewayInner>);
impl ConnectionGuard {
    pub(super) fn acquire(inner: Arc<GatewayInner>) -> Option<Self> {
        inner
            .websocket_connections
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_CONNECTIONS).then_some(count + 1)
            })
            .ok()?;
        Some(Self(inner))
    }
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.websocket_connections.fetch_sub(1, Ordering::AcqRel);
    }
}
