use super::{app::QueueItem, sounds::Cue};
use crate::engine::JobStatus;

#[derive(Default)]
pub(super) struct QueueSummary {
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cancelled: usize,
    pub total: usize,
    pub reboot: bool,
}

impl QueueSummary {
    pub fn from_items(items: &[QueueItem]) -> Self {
        let mut summary = Self {
            total: items.len(),
            ..Self::default()
        };
        for item in items {
            match item.status {
                JobStatus::Done { reboot_required } => {
                    summary.done += 1;
                    summary.reboot |= reboot_required;
                }
                JobStatus::Failed(_) => summary.failed += 1,
                JobStatus::Skipped(_) => summary.skipped += 1,
                JobStatus::Cancelled => summary.cancelled += 1,
                _ => {}
            }
        }
        summary
    }

    pub fn completed(&self) -> usize {
        self.done + self.failed + self.skipped + self.cancelled
    }

    pub fn title(&self) -> &'static str {
        if self.completed() < self.total {
            "Выполняем задачи"
        } else if self.failed + self.skipped > 0 {
            "Нужны действия"
        } else if self.cancelled > 0 {
            "Очередь остановлена"
        } else {
            "Все задачи выполнены"
        }
    }

    pub fn cue(&self) -> Option<Cue> {
        if self.total == 0 || self.completed() != self.total {
            None
        } else if self.failed + self.skipped > 0 {
            Some(Cue::Error)
        } else if self.cancelled > 0 {
            None
        } else {
            Some(Cue::Success)
        }
    }
}
