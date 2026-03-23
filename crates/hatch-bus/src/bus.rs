use std::sync::Arc;

use hatch_core::{HatchError, Result};
use tokio::sync::broadcast;
use tracing::trace;

use crate::HatchMessage;

/// In-memory publish/subscribe bus for [`HatchMessage`] events.
#[derive(Clone)]
pub struct MessageBus {
    sender: Arc<broadcast::Sender<HatchMessage>>,
    capacity: usize,
}

impl MessageBus {
    /// Creates a bus with the given channel capacity (bounded lag for slow subscribers).
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self {
            sender: Arc::new(sender),
            capacity: capacity.max(1),
        }
    }

    /// Publishes a message to all active subscribers.
    pub fn publish(&self, msg: HatchMessage) -> Result<()> {
        trace!(target: "hatch_bus", "publish message");
        self.sender
            .send(msg)
            .map_err(|e| HatchError::Bus(e.to_string()))?;
        Ok(())
    }

    /// Subscribes to subsequent messages (missed if lag exceeds capacity).
    pub fn subscribe(&self) -> broadcast::Receiver<HatchMessage> {
        self.sender.subscribe()
    }

    /// Returns the configured capacity hint.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use hatch_core::{AgentId, AgentOutput, RunId, TaskId};

    use super::MessageBus;
    use crate::HatchMessage;

    #[tokio::test]
    async fn publish_delivers_to_subscriber() {
        let bus = MessageBus::new(16);
        let mut rx = bus.subscribe();
        let run = RunId::new_v4();
        let aid = AgentId::new_v4();
        let tid = TaskId::new_v4();
        let msg = HatchMessage::AgentDone(AgentOutput {
            agent_id: aid,
            task_id: tid,
            content: "hello".into(),
            artifacts: vec![],
        });
        bus.publish(msg.clone()).expect("publish");
        let got = rx.recv().await.expect("recv");
        match got {
            HatchMessage::AgentDone(o) => {
                assert_eq!(o.content, "hello");
                assert_eq!(o.agent_id, aid);
            }
            _ => panic!("unexpected message"),
        }
    }
}
