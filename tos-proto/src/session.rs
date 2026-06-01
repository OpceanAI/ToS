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
    pub session_id: [u8; 32],
    pub stats: SessionStats,
}

impl Session {
    pub fn new(session_id: [u8; 32]) -> Self {
        Self {
            session_id,
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

    #[test]
    fn session_stats_default() {
        let s = Session::new([0u8; 32]);
        assert_eq!(s.stats.total_records, 0);
        assert_eq!(s.stats.total_batches, 0);
    }

    #[test]
    fn session_tracks_batches() {
        let mut s = Session::new([0u8; 32]);
        s.record_batch_sent(1024);
        s.record_batch_sent(2048);
        s.record_batch_received(512);
        assert_eq!(s.stats.total_batches, 2);
        assert_eq!(s.stats.bytes_sent, 3072);
        assert_eq!(s.stats.bytes_received, 512);
    }

    #[test]
    fn end_stream_message_has_session_id() {
        let mut sid = [0u8; 32];
        sid[0] = 0xab;
        let s = Session::new(sid);
        let end = s.end_stream(100, 500);
        assert_eq!(end.session_id, sid);
        assert_eq!(end.total_records, 100);
        assert_eq!(end.duration_ms, 500);
    }
}
