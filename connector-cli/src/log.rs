use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub type DateTime = chrono::DateTime<chrono::Utc>;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Log {
    pub events: Vec<Event>,
}

impl Log {
    pub fn push(&mut self, kind: EventKind) {
        let timestamp = chrono::Utc::now();
        self.events.push(Event { timestamp, kind })
    }

    pub async fn append_to_file<P: AsRef<Path>>(self, path: P) -> anyhow::Result<()> {
        let mut writer = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        for event in self.events {
            let serialized = serde_json::to_string(&event)?;
            writer.write_all(serialized.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: DateTime,
    pub kind: EventKind,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum EventKind {
    Make { command: String },
    InitConfig { new_config: crate::config::Config },
}
