use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use serde_json::Value;

use crate::{Usage, UsageProvider};

fn collect_jsonl(dir: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry.with_context(|| format!("reading entry in {}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .with_context(|| format!("inspecting {}", path.display()))?;
        if kind.is_dir() {
            collect_jsonl(&path, files)?;
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") {
            files.push(path);
        }
    }
    Ok(())
}

fn token_count(usage: &Value, field: &str) -> anyhow::Result<u128> {
    usage
        .get(field)
        .and_then(Value::as_u64)
        .map(u128::from)
        .ok_or_else(|| anyhow!("missing or invalid token field: {field}"))
}

pub struct ClaudeCodeProvider {
    root: PathBuf,
    limit: f64,
    reset_after_secs: u64,
    reset_anchor_secs: Option<u64>,
}

impl ClaudeCodeProvider {
    pub fn new(
        root: PathBuf,
        limit: f64,
        reset_after_secs: u64,
        reset_anchor_secs: Option<u64>,
    ) -> Self {
        Self {
            root,
            limit,
            reset_after_secs,
            reset_anchor_secs,
        }
    }
}

#[async_trait]
impl UsageProvider for ClaudeCodeProvider {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    async fn fetch(&mut self) -> anyhow::Result<Usage> {
        let projects = self.root.join("projects");
        let mut files = Vec::new();
        collect_jsonl(&projects, &mut files)?;
        files.sort();

        let now = SystemTime::now();
        let cutoff = now
            .checked_sub(Duration::from_secs(self.reset_after_secs))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let mut total = 0u128;

        for path in files {
            let content =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            for (index, line) in content.lines().enumerate() {
                let record: Value = serde_json::from_str(line)
                    .with_context(|| format!("parsing line {} of {}", index + 1, path.display()))?;
                if record.get("type").and_then(Value::as_str) != Some("assistant") {
                    continue;
                }

                let raw_timestamp = record
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("assistant record has no timestamp"))?;
                let parsed = raw_timestamp.parse::<jiff::Timestamp>().with_context(|| {
                    format!(
                        "parsing timestamp on line {} of {}",
                        index + 1,
                        path.display()
                    )
                })?;
                let recorded_at = SystemTime::try_from(parsed).with_context(|| {
                    format!(
                        "converting timestamp on line {} of {}",
                        index + 1,
                        path.display()
                    )
                })?;
                if recorded_at < cutoff {
                    continue;
                }

                let usage = record
                    .get("message")
                    .and_then(|message| message.get("usage"))
                    .ok_or_else(|| anyhow!("assistant record has no usage"))?;
                total += token_count(usage, "input_tokens")?;
                total += token_count(usage, "output_tokens")?;
                total += token_count(usage, "cache_creation_input_tokens")?;
                total += token_count(usage, "cache_read_input_tokens")?;
            }
        }

        Ok(Usage {
            plan: None,
            meters: vec![crate::window::window_meter(
                self.reset_anchor_secs,
                self.reset_after_secs,
                self.limit,
                total as f64,
            )],
            spend: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::test_support::{block_on, TempPath};

    fn write_jsonl(root: &std::path::Path, name: &str, lines: &[String]) {
        let dir = root.join("projects").join("nested");
        std::fs::create_dir_all(&dir).expect("create projects dir");
        std::fs::write(dir.join(name), lines.join("\n") + "\n").expect("write jsonl");
    }

    fn now_timestamp() -> String {
        let timestamp =
            jiff::Timestamp::try_from(SystemTime::now()).expect("convert now to jiff timestamp");
        timestamp.to_string()
    }

    fn assistant_line(
        timestamp: &str,
        input: u64,
        output: u64,
        cache_write: u64,
        cache_read: u64,
    ) -> String {
        format!(
            r#"{{"parentUuid":null,"isSidechain":false,"userType":"external","cwd":"C:\\rust-projects\\orion","sessionId":"s-1","version":"1.0.127","gitBranch":"main","type":"assistant","uuid":"u-1","timestamp":"{timestamp}","message":{{"id":"m-1","container":null,"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","stop_sequence":"","type":"message","usage":{{"input_tokens":{input},"output_tokens":{output},"cache_creation_input_tokens":{cache_write},"cache_read_input_tokens":{cache_read},"server_tool_use":{{"web_search_requests":0}},"service_tier":null,"cache_creation":{{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":0}}}},"content":[]}},"isApiErrorMessage":true}}"#
        )
    }

    #[test]
    fn assistant_token_counts_sum_into_one_window_meter() {
        let dir = TempPath::new("claude-happy");
        let timestamp = now_timestamp();
        let lines = vec![
            format!(
                r#"{{"type":"user","timestamp":"{timestamp}","message":{{"role":"user","content":"hi"}}}}"#
            ),
            assistant_line(&timestamp, 11, 22, 33, 44),
        ];
        write_jsonl(dir.path(), "conversation.jsonl", &lines);

        let before = SystemTime::now();
        let mut provider = ClaudeCodeProvider::new(dir.path().to_path_buf(), 500.0, 3_600, None);
        let usage = block_on(provider.fetch()).expect("fetch should succeed");

        assert!(usage.plan.is_none());
        assert!(usage.spend.is_none());
        assert_eq!(usage.meters.len(), 1);
        let meter = &usage.meters[0];
        assert_eq!(meter.label, "usage window");
        assert_eq!(meter.used, 110.0);
        assert_eq!(meter.limit, 500.0);
        assert!(meter.reset_at.expect("meter has reset time") > before);
    }

    #[test]
    fn id_is_claude_code() {
        assert_eq!(
            ClaudeCodeProvider::new(PathBuf::new(), 1.0, 1, None).id(),
            "claude-code"
        );
    }

    #[test]
    fn missing_root_returns_error() {
        let dir = TempPath::new("claude-missing");
        let mut provider = ClaudeCodeProvider::new(dir.path().to_path_buf(), 500.0, 3_600, None);

        assert!(block_on(provider.fetch()).is_err());
    }

    #[test]
    fn malformed_json_line_returns_error() {
        let dir = TempPath::new("claude-malformed");
        let timestamp = now_timestamp();
        write_jsonl(
            dir.path(),
            "conversation.jsonl",
            &[
                assistant_line(&timestamp, 1, 2, 3, 4),
                "{not json".to_owned(),
            ],
        );
        let mut provider = ClaudeCodeProvider::new(dir.path().to_path_buf(), 500.0, 3_600, None);

        assert!(block_on(provider.fetch()).is_err());
    }

    #[test]
    fn unparseable_timestamp_returns_error() {
        let dir = TempPath::new("claude-bad-timestamp");
        write_jsonl(
            dir.path(),
            "conversation.jsonl",
            &[assistant_line("not-a-time", 1, 2, 3, 4)],
        );
        let mut provider = ClaudeCodeProvider::new(dir.path().to_path_buf(), 500.0, 3_600, None);

        assert!(block_on(provider.fetch()).is_err());
    }

    #[test]
    fn records_outside_the_window_are_ignored() {
        let dir = TempPath::new("claude-window");
        let fresh = now_timestamp();
        let old = jiff::Timestamp::try_from(SystemTime::now() - Duration::from_secs(2 * 86_400))
            .expect("convert old time to jiff timestamp")
            .to_string();
        write_jsonl(
            dir.path(),
            "conversation.jsonl",
            &[
                assistant_line(&fresh, 4, 3, 2, 1),
                assistant_line(&old, 1_000, 1_000, 1_000, 1_000),
            ],
        );
        let mut provider = ClaudeCodeProvider::new(dir.path().to_path_buf(), 500.0, 3_600, None);

        let usage = block_on(provider.fetch()).expect("fetch should succeed");
        assert_eq!(usage.meters[0].used, 10.0);
    }
}
