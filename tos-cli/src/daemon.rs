use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::cmd;

#[derive(Debug, Clone, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_log_level")]
    #[allow(dead_code)]
    pub log_level: String,
    #[serde(default = "default_sync_interval")]
    pub sync_interval_secs: u64,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub pipeline: Vec<PipelineConfig>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_sync_interval() -> u64 {
    5
}

#[derive(Debug, Clone, Deserialize)]
pub struct PipelineConfig {
    pub name: String,
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default = "default_batch_size")]
    pub batch_size: u64,
    #[serde(default)]
    pub watch: bool,
    #[serde(default)]
    pub interval_secs: Option<u64>,
    #[serde(default)]
    pub disabled: bool,
}

fn default_batch_size() -> u64 {
    500
}

pub fn load_config(path: &Path) -> Result<DaemonConfig> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading topology file {}", path.display()))?;
    let cfg: DaemonConfig = toml::from_str(&text)
        .with_context(|| format!("parsing topology file {}", path.display()))?;
    Ok(cfg)
}

#[derive(Debug, Clone, Default)]
pub struct PipelineStatus {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub last_run_secs_ago: Option<u64>,
    #[allow(dead_code)]
    pub last_records: u64,
    #[allow(dead_code)]
    pub last_batches: u64,
    #[allow(dead_code)]
    pub last_bytes: u64,
    #[allow(dead_code)]
    pub total_runs: u64,
    #[allow(dead_code)]
    pub total_records: u64,
    #[allow(dead_code)]
    pub last_error: Option<String>,
}

pub struct Daemon {
    config: DaemonConfig,
    #[allow(dead_code)]
    handles: Vec<JoinHandle<()>>,
    statuses: Arc<Mutex<BTreeMap<String, PipelineStatus>>>,
    #[allow(dead_code)]
    started_at: std::time::Instant,
}

impl Daemon {
    pub fn new(config: DaemonConfig) -> Self {
        let statuses = Arc::new(Mutex::new(BTreeMap::new()));
        Self {
            config,
            handles: Vec::new(),
            statuses,
            started_at: std::time::Instant::now(),
        }
    }

    #[allow(dead_code)]
    pub async fn statuses(&self) -> BTreeMap<String, PipelineStatus> {
        self.statuses.lock().await.clone()
    }

    pub async fn start(&mut self) -> Result<()> {
        let pipelines = self.config.pipeline.clone();
        for p in pipelines {
            if p.disabled {
                continue;
            }
            if p.to.is_empty() {
                return Err(anyhow!(
                    "pipeline `{}` has empty `to` list",
                    p.name
                ));
            }
            let statuses = self.statuses.clone();
            let interval = p
                .interval_secs
                .unwrap_or(self.config.sync_interval_secs)
                .max(1);
            let handle = tokio::spawn(async move {
                run_pipeline(p, interval, statuses).await;
            });
            self.handles.push(handle);
        }
        Ok(())
    }

    pub fn abort_all(&self) {
        for h in &self.handles {
            h.abort();
        }
    }

    #[allow(dead_code)]
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub fn node_id(&self) -> String {
        self.config
            .node_id
            .clone()
            .unwrap_or_else(generate_node_id)
    }

    pub fn config_ref(&self) -> &DaemonConfig {
        &self.config
    }
}

pub fn generate_node_id_pub() -> String {
    generate_node_id()
}

async fn run_pipeline(
    p: PipelineConfig,
    interval_secs: u64,
    statuses: Arc<Mutex<BTreeMap<String, PipelineStatus>>>,
) {
    let mut status = PipelineStatus {
        name: p.name.clone(),
        ..Default::default()
    };
    let mut last_run = std::time::Instant::now();
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        if p.disabled {
            continue;
        }
        let started = std::time::Instant::now();
        let result = cmd::sync(&p.from, &p.to, p.table.as_deref(), p.watch, interval_secs).await;
        let elapsed = started.elapsed();
        match result {
            Ok(stats) => {
                let total: u64 = stats.iter().map(|s| s.total_records).sum();
                let batches: u64 = stats.iter().map(|s| u64::from(s.total_batches)).sum();
                let bytes: u64 = stats.iter().map(|s| s.bytes_sent).sum();
                status.last_records = total;
                status.last_batches = batches;
                status.last_bytes = bytes;
                status.last_run_secs_ago = Some(last_run.elapsed().as_secs());
                status.total_runs += 1;
                status.total_records += total;
                status.last_error = None;
                tracing::info!(
                    pipeline = %p.name,
                    records = total,
                    batches,
                    bytes,
                    elapsed_ms = elapsed.as_millis() as u64,
                    "pipeline tick ok"
                );
            }
            Err(e) => {
                status.last_error = Some(format!("{e:#}"));
                tracing::warn!(
                    pipeline = %p.name,
                    error = %e,
                    "pipeline tick failed"
                );
            }
        }
        last_run = std::time::Instant::now();
        statuses.lock().await.insert(p.name.clone(), status.clone());
    }
}

fn generate_node_id() -> String {
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "tos-node".to_string());
    let h = simple_hash(&hostname);
    format!("node-{:016x}", h)
}

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn default_topology_path() -> PathBuf {
    if let Some(p) = std::env::var_os("TOS_TOPOLOGY") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("tos").join("topology.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_toml(content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tos-daemon-{}-{}", std::process::id(), simple_hash(content)));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("topology.toml");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        p
    }

    #[test]
    fn load_minimal_config() {
        let p = write_toml(
            r#"
[[pipeline]]
name = "demo"
from = "mock://src?records=3"
to = ["mock://dst"]
"#,
        );
        let cfg = load_config(&p).unwrap();
        assert_eq!(cfg.pipeline.len(), 1);
        assert_eq!(cfg.pipeline[0].name, "demo");
        assert_eq!(cfg.sync_interval_secs, 5);
    }

    #[test]
    fn load_config_with_explicit_interval() {
        let p = write_toml(
            r#"
sync_interval_secs = 10

[[pipeline]]
name = "p1"
from = "mock://a?records=1"
to = ["mock://b"]
interval_secs = 1
batch_size = 100
watch = true
"#,
        );
        let cfg = load_config(&p).unwrap();
        assert_eq!(cfg.sync_interval_secs, 10);
        assert_eq!(cfg.pipeline[0].interval_secs, Some(1));
        assert_eq!(cfg.pipeline[0].batch_size, 100);
        assert!(cfg.pipeline[0].watch);
    }

    #[test]
    fn load_invalid_toml_errors() {
        let p = write_toml("this is not = = valid toml [[[");
        let res = load_config(&p);
        assert!(res.is_err());
    }

    #[test]
    fn load_missing_pipeline_succeeds_empty() {
        let p = write_toml("log_level = \"debug\"\n");
        let res = load_config(&p);
        let cfg = res.expect("config without pipeline is valid (empty)");
        assert!(cfg.pipeline.is_empty());
        assert_eq!(cfg.log_level, "debug");
    }

    #[test]
    fn node_id_format() {
        let id = generate_node_id();
        assert!(id.starts_with("node-"));
        assert_eq!(id.len(), "node-".len() + 16);
    }

    #[test]
    fn node_id_stable_for_same_host() {
        assert_eq!(generate_node_id(), generate_node_id());
    }

    #[test]
    fn simple_hash_stable() {
        assert_eq!(simple_hash("hello"), simple_hash("hello"));
        assert_ne!(simple_hash("hello"), simple_hash("world"));
    }

    #[test]
    fn default_topology_path_returns_path() {
        let p = default_topology_path();
        let s = p.to_string_lossy();
        assert!(!s.is_empty());
    }
}
