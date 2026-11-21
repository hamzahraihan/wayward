pub mod claude_code;
pub mod level;
pub mod mock;
pub mod opencode;
pub mod registry;

#[cfg(test)]
pub(crate) mod test_support {
    use std::future::Future;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::SystemTime;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    pub(crate) struct TempPath(PathBuf);

    impl TempPath {
        pub(crate) fn new(name: &str) -> Self {
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("clock is after epoch")
                .as_nanos();
            Self(std::env::temp_dir().join(format!(
                "wayward-ai-{name}-{}-{id}-{nanos}",
                std::process::id()
            )))
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempPath {
        fn drop(&mut self) {
            if self.0.is_dir() {
                let _ = std::fs::remove_dir_all(&self.0);
            } else {
                let _ = std::fs::remove_file(&self.0);
            }
        }
    }

    pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(output) => output,
            std::task::Poll::Pending => panic!("provider future should be ready immediately"),
        }
    }
}

use std::time::SystemTime;

use async_trait::async_trait;

#[async_trait]
pub trait UsageProvider: Send {
    fn id(&self) -> &'static str;
    async fn fetch(&mut self) -> anyhow::Result<Usage>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub plan: Option<String>,
    pub meters: Vec<Meter>,
    pub spend: Option<Spend>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Meter {
    pub label: String,
    pub used: f64,
    pub limit: f64,
    pub reset_at: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spend {
    pub used: f64,
    pub limit: f64,
    pub currency: String,
}
