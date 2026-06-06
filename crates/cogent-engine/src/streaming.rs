//! Streaming support for the Cogent Engine

use crate::types::{CheckEvent, EngineResult};
use cogent_protocol::types::*;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;
use tokio::sync::mpsc;

/// Stream of check events
pub type CheckEventStream = Pin<Box<dyn Stream<Item = CheckEvent> + Send>>;

/// Create a streaming channel for check events
pub fn create_check_stream() -> (mpsc::Sender<CheckEvent>, CheckEventStream) {
    let (tx, rx) = mpsc::channel(100);
    let stream = Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx));
    (tx, stream)
}

/// Convert a findings stream into CheckEvents
pub fn findings_to_events(check_id: String, findings: Vec<Finding>) -> Vec<CheckEvent> {
    let mut events = Vec::new();

    for finding in findings {
        events.push(CheckEvent::Finding(finding));
    }

    events.push(CheckEvent::End(FindingsEndParams {
        check_id,
        total_findings: events.len() - 1, // minus the End event
    }));

    events
}

/// Send finding events through a channel
pub async fn send_findings(
    tx: &mpsc::Sender<CheckEvent>,
    check_id: String,
    findings: Vec<Finding>,
) -> EngineResult<()> {
    for finding in findings {
        tx.send(CheckEvent::Finding(finding)).await?;
    }
    tx.send(CheckEvent::End(FindingsEndParams {
        check_id,
        total_findings: findings.len(),
    }))
    .await?;
    Ok(())
}

/// Send progress event
pub async fn send_progress(
    tx: &mpsc::Sender<CheckEvent>,
    check_id: String,
    rule: String,
    stage: String,
    files_processed: usize,
    total_files: usize,
    message: Option<String>,
) -> EngineResult<()> {
    tx.send(CheckEvent::Progress(ProgressParams {
        check_id,
        rule,
        stage,
        files_processed,
        total_files,
        message,
    }))
    .await?;
    Ok(())
}

/// Send rule complete event
pub async fn send_rule_complete(
    tx: &mpsc::Sender<CheckEvent>,
    check_id: String,
    rule: String,
    passed: bool,
    score: Option<f64>,
    threshold: Option<f64>,
    duration_ms: u64,
) -> EngineResult<()> {
    tx.send(CheckEvent::RuleComplete(RuleCompleteParams {
        check_id,
        rule,
        passed,
        score,
        threshold,
        duration_ms,
    }))
    .await?;
    Ok(())
}

/// Streaming adapter for the protocol server
pub struct StreamingAdapter {
    check_id: String,
    tx: mpsc::Sender<CheckEvent>,
}

impl StreamingAdapter {
    pub fn new(tx: mpsc::Sender<CheckEvent>) -> Self {
        Self {
            check_id: uuid::Uuid::new_v4().to_string(),
            tx,
        }
    }

    pub fn with_check_id(tx: mpsc::Sender<CheckEvent>, check_id: String) -> Self {
        Self { check_id, tx }
    }

    pub async fn send_finding(&self, finding: Finding) -> EngineResult<()> {
        self.tx.send(CheckEvent::Finding(finding)).await?;
        Ok(())
    }

    pub async fn send_progress(
        &self,
        rule: String,
        stage: String,
        files_processed: usize,
        total_files: usize,
        message: Option<String>,
    ) -> EngineResult<()> {
        send_progress(
            &self.tx,
            self.check_id.clone(),
            rule,
            stage,
            files_processed,
            total_files,
            message,
        )
        .await
    }

    pub async fn send_rule_complete(
        &self,
        rule: String,
        passed: bool,
        score: Option<f64>,
        threshold: Option<f64>,
        duration_ms: u64,
    ) -> EngineResult<()> {
        send_rule_complete(
            &self.tx,
            self.check_id.clone(),
            rule,
            passed,
            score,
            threshold,
            duration_ms,
        )
        .await
    }

    pub async fn send_end(&self, total_findings: usize) -> EngineResult<()> {
        self.tx
            .send(CheckEvent::End(FindingsEndParams {
                check_id: self.check_id.clone(),
                total_findings,
            }))
            .await?;
        Ok(())
    }

    pub fn check_id(&self) -> &str {
        &self.check_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_streaming_events() {
        let (tx, mut stream) = create_check_stream();
        let adapter = StreamingAdapter::new(tx);

        let finding = Finding {
            finding_id: "test:file.rs:1".into(),
            rule_id: "test".into(),
            rule_pack: None,
            severity: Severity::High,
            category: Category::Security,
            file: "file.rs".into(),
            line: Some(1),
            column: Some(1),
            end_line: None,
            end_column: None,
            message: "Test finding".into(),
            code_snippet: None,
            suggested_fix: None,
            compliance_controls: vec![],
            tags: vec![],
            metadata: None,
        };

        adapter.send_finding(finding.clone()).await.unwrap();
        adapter.send_end(1).await.unwrap();
        drop(adapter);

        let mut count = 0;
        while let Some(event) = stream.next().await {
            count += 1;
            match event {
                CheckEvent::Finding(f) => assert_eq!(f.finding_id, "test:file.rs:1"),
                CheckEvent::End(e) => assert_eq!(e.total_findings, 1),
                _ => panic!("unexpected event"),
            }
        }
        assert_eq!(count, 2); // 1 finding + 1 end
    }
}