//! Read-only session aggregates: message summary, discussion, inbox hints.
use super::model::{
    SessionDiscussionCounts, SessionDiscussionSummary, SessionInboxHint, SessionInboxOpenCounts,
    SessionMessage, SessionMessageError, SessionMessageKind, SessionMessagePriority,
    SessionMessageStatus, SessionMessagesSummary, SessionRecord, MAX_MESSAGE_CHARS,
    MAX_MESSAGE_RESOLUTION_CHARS, MAX_MESSAGE_SUMMARY_CHARS, MAX_MESSAGE_TAGS,
    MAX_MESSAGE_TAG_CHARS, SUMMARY_MESSAGE_GROUP_LIMIT,
};
use super::util::bound_chars;

pub(super) fn build_messages_summary(record: &SessionRecord) -> SessionMessagesSummary {
    let total = record.messages.len();
    let open = record
        .messages
        .iter()
        .filter(|message| message.status == SessionMessageStatus::Open)
        .count();
    SessionMessagesSummary {
        total,
        open,
        resolved: total.saturating_sub(open),
        pending_guidance: count_open_kind(record, SessionMessageKind::Guidance),
        open_questions: count_open_kind(record, SessionMessageKind::Question),
        open_risks: count_open_kind(record, SessionMessageKind::Risk),
        open_todos: count_open_kind(record, SessionMessageKind::Todo),
        recent_progress: take_recent_kind(
            record,
            SessionMessageKind::Progress,
            None,
            SUMMARY_MESSAGE_GROUP_LIMIT,
        ),
        guidance: count_kind(record, SessionMessageKind::Guidance),
        progress: count_kind(record, SessionMessageKind::Progress),
        risk: count_kind(record, SessionMessageKind::Risk),
        todo: count_kind(record, SessionMessageKind::Todo),
        question: count_kind(record, SessionMessageKind::Question),
        decision: count_kind(record, SessionMessageKind::Decision),
    }
}

pub(super) fn build_discussion_counts(record: &SessionRecord) -> SessionDiscussionCounts {
    let total = record.messages.len();
    let open = record
        .messages
        .iter()
        .filter(|message| message.status == SessionMessageStatus::Open)
        .count();
    SessionDiscussionCounts {
        total,
        open,
        resolved: total.saturating_sub(open),
        guidance: count_kind(record, SessionMessageKind::Guidance),
        progress: count_kind(record, SessionMessageKind::Progress),
        risk: count_kind(record, SessionMessageKind::Risk),
        todo: count_kind(record, SessionMessageKind::Todo),
        question: count_kind(record, SessionMessageKind::Question),
        decision: count_kind(record, SessionMessageKind::Decision),
    }
}

pub(super) fn build_discussion_summary(
    record: &SessionRecord,
    limit: usize,
) -> SessionDiscussionSummary {
    SessionDiscussionSummary {
        counts: build_discussion_counts(record),
        open_guidance: take_recent_kind(
            record,
            SessionMessageKind::Guidance,
            Some(SessionMessageStatus::Open),
            limit.min(SUMMARY_MESSAGE_GROUP_LIMIT),
        ),
        open_questions: take_recent_kind(
            record,
            SessionMessageKind::Question,
            Some(SessionMessageStatus::Open),
            limit.min(SUMMARY_MESSAGE_GROUP_LIMIT),
        ),
        open_risks: take_recent_kind(
            record,
            SessionMessageKind::Risk,
            Some(SessionMessageStatus::Open),
            limit.min(SUMMARY_MESSAGE_GROUP_LIMIT),
        ),
        open_todos: take_recent_kind(
            record,
            SessionMessageKind::Todo,
            Some(SessionMessageStatus::Open),
            limit.min(SUMMARY_MESSAGE_GROUP_LIMIT),
        ),
        recent_progress: take_recent_kind(record, SessionMessageKind::Progress, None, limit),
        recent_decisions: take_recent_kind(record, SessionMessageKind::Decision, None, limit),
    }
}

pub(super) fn build_inbox_hint(record: &SessionRecord) -> Option<SessionInboxHint> {
    let mut counts = SessionInboxOpenCounts::default();
    let mut highest_priority = None;

    for message in record
        .messages
        .iter()
        .filter(|message| message.status == SessionMessageStatus::Open)
    {
        match message.kind {
            SessionMessageKind::Guidance => counts.guidance += 1,
            SessionMessageKind::Question => counts.question += 1,
            SessionMessageKind::Todo => counts.todo += 1,
            SessionMessageKind::Risk => counts.risk += 1,
            _ => continue,
        }
        if highest_priority
            .is_none_or(|priority| priority_rank(message.priority) > priority_rank(priority))
        {
            highest_priority = Some(message.priority);
        }
    }

    highest_priority.map(|priority| SessionInboxHint {
        has_open_messages: true,
        open_counts: counts,
        highest_priority: priority,
        suggested_next_tool: "session_discussion_summary",
    })
}

pub(super) fn priority_rank(priority: SessionMessagePriority) -> u8 {
    match priority {
        SessionMessagePriority::Low => 0,
        SessionMessagePriority::Normal => 1,
        SessionMessagePriority::High => 2,
    }
}

pub(super) fn count_kind(record: &SessionRecord, kind: SessionMessageKind) -> usize {
    record
        .messages
        .iter()
        .filter(|message| message.kind == kind)
        .count()
}

pub(super) fn count_open_kind(record: &SessionRecord, kind: SessionMessageKind) -> usize {
    record
        .messages
        .iter()
        .filter(|message| message.kind == kind && message.status == SessionMessageStatus::Open)
        .count()
}

pub(super) fn take_recent_kind(
    record: &SessionRecord,
    kind: SessionMessageKind,
    status: Option<SessionMessageStatus>,
    limit: usize,
) -> Vec<SessionMessage> {
    record
        .messages
        .iter()
        .rev()
        .filter(|message| message.kind == kind)
        .filter(|message| status.is_none_or(|status| message.status == status))
        .take(limit)
        .cloned()
        .map(bound_message_for_summary)
        .collect()
}

pub(super) fn bound_message_for_summary(mut message: SessionMessage) -> SessionMessage {
    message.message = bound_chars(&message.message, MAX_MESSAGE_SUMMARY_CHARS);
    if let Some(resolution) = message.resolution.as_mut() {
        *resolution = bound_chars(resolution, MAX_MESSAGE_SUMMARY_CHARS);
    }
    message
}

pub(super) fn validate_message_text(value: String) -> Result<String, SessionMessageError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(SessionMessageError::InvalidInput(
            "message must not be empty".to_string(),
        ));
    }
    if value.chars().count() > MAX_MESSAGE_CHARS {
        return Err(SessionMessageError::InvalidInput(format!(
            "message exceeds {MAX_MESSAGE_CHARS} chars"
        )));
    }
    Ok(value)
}

pub(super) fn validate_resolution_text(value: String) -> Result<String, SessionMessageError> {
    let value = value.trim().to_string();
    if value.chars().count() > MAX_MESSAGE_RESOLUTION_CHARS {
        return Err(SessionMessageError::InvalidInput(format!(
            "resolution exceeds {MAX_MESSAGE_RESOLUTION_CHARS} chars"
        )));
    }
    Ok(value)
}

pub(super) fn validate_message_tags(
    values: Vec<String>,
) -> Result<Vec<String>, SessionMessageError> {
    if values.len() > MAX_MESSAGE_TAGS {
        return Err(SessionMessageError::InvalidInput(format!(
            "tags exceed {MAX_MESSAGE_TAGS} items"
        )));
    }
    let mut tags = Vec::new();
    for value in values {
        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > MAX_MESSAGE_TAG_CHARS {
            return Err(SessionMessageError::InvalidInput(format!(
                "tag exceeds {MAX_MESSAGE_TAG_CHARS} chars"
            )));
        }
        if !tags.iter().any(|tag| tag == &value) {
            tags.push(value);
        }
    }
    Ok(tags)
}
