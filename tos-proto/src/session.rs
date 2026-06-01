use std::sync::Arc;

use tos_crypto::Identity;

use crate::messages::StreamEnd;

#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    pub total_records: u64,
    pub total_batches: u32,
    pub duration_ms: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

pub struct Session {
    pub identity: Arc<Identity>,
    pub session_id: [u8; 32],
    pub batch_size: u32,
    pub stats: SessionStats,
}

impl Session {
    pub fn new(identity: Arc<Identity>, batch_size: u32) -> Self {
        let session_id = *identity.node_id().as_bytes();
        Self {
            identity,
            session_id,
            batch_size,
            stats: SessionStats::default(),
        }
    }

    pub fn end_stream(&self, total_records: u64, duration_ms: u64) -> StreamEnd {
        StreamEnd {
            session_id: self.session_id,
            total_records,
            duration_ms,
        }
    }

    pub fn record_batch_sent(&mut self, bytes: u64) {
        self.stats.total_batches += 1;
        self.stats.bytes_sent += bytes;
    }

    pub fn record_batch_received(&mut self, bytes: u64) {
        self.stats.bytes_received += bytes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn make_id() -> Arc<Identity> {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        Arc::new(Identity::from_signing_key(key))
    }

    #[test]
    fn session_stats_default() {
        let s = Session::new(make_id(), 100);
        assert_eq!(s.stats.total_records, 0);
        assert_eq!(s.stats.total_batches, 0);
    }

    #[test]
    fn session_tracks_batches() {
        let mut s = Session::new(make_id(), 100);
        s.record_batch_sent(1024);
        s.record_batch_sent(2048);
        s.record_batch_received(512);
        assert_eq!(s.stats.total_batches, 2);
        assert_eq!(s.stats.bytes_sent, 3072);
        assert_eq!(s.stats.bytes_received, 512);
    }

    #[test]
    fn end_stream_message_has_session_id() {
        let s = Session::new(make_id(), 100);
        let end = s.end_stream(100, 500);
        assert_eq!(end.session_id, s.session_id);
        assert_eq!(end.total_records, 100);
        assert_eq!(end.duration_ms, 500);
    }

    #[test]
    fn session_id_equals_node_id() {
        let id = make_id();
        let s = Session::new(id.clone(), 100);
        assert_eq!(s.session_id, *id.node_id().as_bytes());
    }
}
