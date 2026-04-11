use crate::core::metrics::ResponseMetric;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

pub const DEFAULT_INTERESTING_STATUSES: [u16; 10] =
    [200, 204, 301, 302, 307, 308, 401, 403, 405, 500];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Soft404Fingerprint {
    pub status: u16,
    pub body_size: usize,
    pub body_fingerprint: u64,
}

impl Soft404Fingerprint {
    pub fn from_metric(metric: &ResponseMetric) -> Option<Self> {
        if metric.transport_error || metric.status == 0 {
            return None;
        }

        Some(Self {
            status: metric.status,
            body_size: metric.body_size,
            body_fingerprint: metric.body_fingerprint,
        })
    }
}

#[derive(Clone, Debug)]
pub struct FindingsConfig {
    pub findings_file: PathBuf,
    pub min_body_bytes: usize,
    pub max_body_bytes: Option<usize>,
    pub soft_404_fingerprint: Option<Soft404Fingerprint>,
    interesting_statuses: BTreeSet<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingDecision {
    Record,
    FilteredByStatus,
    FilteredByBodyLength,
    SuppressedSoft404,
}

impl FindingsConfig {
    pub fn new(
        findings_file: PathBuf,
        interesting_statuses: Vec<u16>,
        min_body_bytes: usize,
        max_body_bytes: Option<usize>,
        soft_404_fingerprint: Option<Soft404Fingerprint>,
    ) -> Self {
        Self {
            findings_file,
            min_body_bytes,
            max_body_bytes,
            soft_404_fingerprint,
            interesting_statuses: interesting_statuses.into_iter().collect(),
        }
    }

    pub fn classify(&self, metric: &ResponseMetric) -> FindingDecision {
        if metric.transport_error || !self.interesting_statuses.contains(&metric.status) {
            return FindingDecision::FilteredByStatus;
        }

        if metric.body_size < self.min_body_bytes
            || self
                .max_body_bytes
                .map(|max| metric.body_size > max)
                .unwrap_or(false)
        {
            return FindingDecision::FilteredByBodyLength;
        }

        if self.matches_soft_404(metric) {
            return FindingDecision::SuppressedSoft404;
        }

        FindingDecision::Record
    }

    pub fn render_statuses(&self) -> String {
        self.interesting_statuses
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn matches_soft_404(&self, metric: &ResponseMetric) -> bool {
        self.soft_404_fingerprint
            .as_ref()
            .map(|fingerprint| {
                metric.status == fingerprint.status
                    && metric.body_size == fingerprint.body_size
                    && metric.body_fingerprint == fingerprint.body_fingerprint
            })
            .unwrap_or(false)
    }
}

pub struct FindingWriter {
    writer: BufWriter<File>,
}

impl FindingWriter {
    pub fn new(config: &FindingsConfig) -> Result<Self> {
        if let Some(parent) = config.findings_file.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "Failed to create findings dir: {}",
                        parent.to_string_lossy()
                    )
                })?;
            }
        }

        let file = File::create(&config.findings_file).with_context(|| {
            format!(
                "Failed to create findings file: {}",
                config.findings_file.display()
            )
        })?;
        let mut writer = BufWriter::new(file);

        writeln!(
            writer,
            "# Sigma Morpho findings\n# statuses={}\n# min_body_bytes={}\n# max_body_bytes={}\n# soft_404_filter={}",
            config.render_statuses(),
            config.min_body_bytes,
            config
                .max_body_bytes
                .map(|v| v.to_string())
                .unwrap_or_else(|| "none".to_string()),
            config.soft_404_fingerprint.is_some()
        )?;

        if let Some(fingerprint) = &config.soft_404_fingerprint {
            writeln!(
                writer,
                "# soft_404_baseline status={} bytes={} fingerprint={}",
                fingerprint.status, fingerprint.body_size, fingerprint.body_fingerprint
            )?;
        }

        Ok(Self { writer })
    }

    pub fn record(&mut self, metric: &ResponseMetric) -> Result<()> {
        writeln!(
            self.writer,
            "status={} bytes={} latency_ms={} fingerprint={} path={}",
            metric.status,
            metric.body_size,
            metric.latency_ms,
            metric.body_fingerprint,
            metric.path
        )?;
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{FindingDecision, FindingsConfig, Soft404Fingerprint};
    use crate::core::metrics::ResponseMetric;

    #[test]
    fn suppresses_metrics_matching_soft_404_baseline() {
        let config = FindingsConfig::new(
            "findings.txt".into(),
            vec![200, 404],
            0,
            None,
            Some(Soft404Fingerprint {
                status: 200,
                body_size: 1234,
                body_fingerprint: 99,
            }),
        );
        let metric = ResponseMetric {
            path: "/fake".to_string(),
            status: 200,
            latency_ms: 40,
            body_size: 1234,
            body_fingerprint: 99,
            transport_error: false,
        };

        assert_eq!(config.classify(&metric), FindingDecision::SuppressedSoft404);
    }

    #[test]
    fn filters_by_status_and_body_length() {
        let config = FindingsConfig::new("findings.txt".into(), vec![200], 10, Some(50), None);

        let status_filtered = ResponseMetric {
            path: "/x".to_string(),
            status: 403,
            latency_ms: 10,
            body_size: 20,
            body_fingerprint: 1,
            transport_error: false,
        };
        let body_filtered = ResponseMetric {
            path: "/y".to_string(),
            status: 200,
            latency_ms: 10,
            body_size: 5,
            body_fingerprint: 1,
            transport_error: false,
        };

        assert_eq!(
            config.classify(&status_filtered),
            FindingDecision::FilteredByStatus
        );
        assert_eq!(
            config.classify(&body_filtered),
            FindingDecision::FilteredByBodyLength
        );
    }
}
