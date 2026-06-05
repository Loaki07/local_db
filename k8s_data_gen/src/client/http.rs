use std::{fmt, fs::File, io::BufReader};

use chrono::Utc;
use reqwest::Client;

use crate::anomaly::AnomalyState;
use crate::config::{password, username};

pub async fn post_live<T: serde::Serialize>(
    client: &Client,
    url: &str,
    records: &[T],
    anomaly_state: &Option<AnomalyState>,
) {
    let anomaly_active = anomaly_state
        .as_ref()
        .map(|a| a.is_active())
        .unwrap_or(false);
    let anomaly_remaining = anomaly_state
        .as_ref()
        .map(|a| a.remaining_secs)
        .unwrap_or(0);

    match client
        .post(url)
        .basic_auth(username(), Some(password()))
        .json(records)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let suffix = if anomaly_active {
                format!(" [ANOMALY ACTIVE: {}s remaining]", anomaly_remaining)
            } else {
                String::new()
            };
            println!(
                "[{}] ✓ {} records{}",
                Utc::now().format("%Y-%m-%d %H:%M:%S"),
                records.len(),
                suffix
            );
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!(
                "[{}] ✗ HTTP {}: {}",
                Utc::now().format("%Y-%m-%d %H:%M:%S"),
                status,
                body
            );
        }
        Err(e) => eprintln!("[{}] ✗ {}", Utc::now().format("%Y-%m-%d %H:%M:%S"), e),
    }
}

pub async fn post_otlp(
    client: &Client,
    url: &str,
    stream_name: Option<&str>,
    body: &serde_json::Value,
    record_count: usize,
    anomaly_state: &Option<AnomalyState>,
) {
    let anomaly_active = anomaly_state
        .as_ref()
        .map(|a| a.is_active())
        .unwrap_or(false);
    let anomaly_remaining = anomaly_state
        .as_ref()
        .map(|a| a.remaining_secs)
        .unwrap_or(0);

    let builder = client
        .post(url)
        .basic_auth(username(), Some(password()))
        .json(body);
    let builder = match stream_name {
        Some(name) => builder.header("stream-name", name),
        None => builder,
    };

    match builder.send().await {
        Ok(resp) if resp.status().is_success() => {
            let suffix = if anomaly_active {
                format!(" [ANOMALY ACTIVE: {}s remaining]", anomaly_remaining)
            } else {
                String::new()
            };
            println!(
                "[{}] ✓ {} records{}",
                Utc::now().format("%Y-%m-%d %H:%M:%S"),
                record_count,
                suffix
            );
        }
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            eprintln!(
                "[{}] ✗ HTTP {}: {}",
                Utc::now().format("%Y-%m-%d %H:%M:%S"),
                status,
                text
            );
        }
        Err(e) => eprintln!("[{}] ✗ {}", Utc::now().format("%Y-%m-%d %H:%M:%S"), e),
    }
}

/// Stream a JSON array file element-by-element in a blocking thread, sending
/// batches through `tx`. Only O(batch_size) records are in memory at once.
pub fn stream_json_array<T>(
    file_path: String,
    batch_size: usize,
    tx: tokio::sync::mpsc::Sender<Vec<T>>,
) -> tokio::task::JoinHandle<Result<(), String>>
where
    T: serde::de::DeserializeOwned + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        use serde::de::{Deserializer as _, SeqAccess, Visitor};

        struct Batcher<T> {
            tx: tokio::sync::mpsc::Sender<Vec<T>>,
            buf: Vec<T>,
            cap: usize,
        }

        impl<'de, T: serde::de::Deserialize<'de>> Visitor<'de> for Batcher<T> {
            type Value = ();
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "JSON array")
            }
            fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<(), A::Error> {
                while let Some(v) = seq.next_element::<T>()? {
                    self.buf.push(v);
                    if self.buf.len() >= self.cap {
                        let batch = std::mem::replace(&mut self.buf, Vec::with_capacity(self.cap));
                        self.tx
                            .blocking_send(batch)
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                    }
                }
                if !self.buf.is_empty() {
                    self.tx
                        .blocking_send(std::mem::take(&mut self.buf))
                        .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                }
                Ok(())
            }
        }

        let reader = BufReader::new(File::open(&file_path).map_err(|e| e.to_string())?);
        let mut de = serde_json::Deserializer::from_reader(reader);
        de.deserialize_seq(Batcher::<T> {
            tx,
            buf: Vec::with_capacity(batch_size),
            cap: batch_size,
        })
        .map_err(|e| e.to_string())
    })
}
