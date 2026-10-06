use function_providers::ChatMessage;

#[derive(Debug, Clone, Copy)]
pub struct ContextBudget {
    pub context_limit: usize,
    pub output_reserve: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ContextCompactionReport {
    pub before_tokens: usize,
    pub after_tokens: usize,
    pub removed_messages: usize,
    pub removed_images: usize,
    pub truncated_outputs: usize,
}

fn estimate_message_tokens(message: &ChatMessage) -> usize {
    let text_tokens = (message.content.chars().count() / 4).max(1);
    let image_tokens = message
        .images
        .as_ref()
        .map(|images| images.len() * 4096)
        .unwrap_or_default();
    text_tokens + image_tokens + 4
}

pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

pub fn messages_fit(messages: &[ChatMessage], budget: ContextBudget) -> bool {
    estimate_tokens(messages) <= budget.context_limit.saturating_sub(budget.output_reserve)
}

pub fn compact_messages(
    messages: &[ChatMessage],
    budget: ContextBudget,
) -> (Vec<ChatMessage>, ContextCompactionReport) {
    let mut compacted = messages.to_vec();
    let before_tokens = estimate_tokens(&compacted);
    let target = budget.context_limit.saturating_sub(budget.output_reserve);
    let mut report = ContextCompactionReport {
        before_tokens,
        ..Default::default()
    };

    if before_tokens <= target {
        report.after_tokens = before_tokens;
        return (compacted, report);
    }

    // Screenshots are the least durable part of old context. Keep only the
    // newest two image-bearing messages so the current visual task survives.
    let mut image_messages = compacted
        .iter()
        .enumerate()
        .filter(|(_, message)| {
            message
                .images
                .as_ref()
                .is_some_and(|images| !images.is_empty())
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    while image_messages.len() > 2 {
        if let Some(index) = image_messages.first().copied() {
            if let Some(message) = compacted.get_mut(index) {
                report.removed_images += message
                    .images
                    .take()
                    .map(|images| images.len())
                    .unwrap_or(0);
            }
        }
        image_messages.remove(0);
    }

    // Tool observations can be enormous JSON blobs. Preserve their role and
    // the beginning/end of the result, which usually contain the useful data.
    for message in compacted.iter_mut().skip(1) {
        if message.tool_call_id.is_some() && message.content.chars().count() > 2400 {
            let chars = message.content.chars().collect::<Vec<_>>();
            let prefix = chars.iter().take(1200).collect::<String>();
            let suffix = chars.iter().skip(chars.len() - 1200).collect::<String>();
            message.content = format!(
                "{}\n[tool output compacted: {} chars omitted]\n{}",
                prefix,
                chars.len() - 2400,
                suffix
            );
            report.truncated_outputs += 1;
        }
    }

    // Drop oldest non-system messages while keeping a recent tail and the
    // latest message, which is the current task or observation.
    while !messages_fit(&compacted, budget) && compacted.len() > 3 {
        compacted.remove(1);
        report.removed_messages += 1;
    }

    // If one current tool result is still too large, bounded truncation is
    // preferable to dispatching an invalid request.
    while !messages_fit(&compacted, budget) {
        let candidate = compacted
            .iter()
            .enumerate()
            .skip(1)
            .max_by_key(|(_, message)| estimate_message_tokens(message))
            .map(|(index, _)| index);
        let Some(index) = candidate else { break };
        let message = &mut compacted[index];
        if message.content.len() <= 256 {
            break;
        }
        let shortened = message.content.chars().take(256).collect::<String>();
        message.content = format!("{}\n[context truncated]", shortened);
        report.truncated_outputs += 1;
    }

    report.after_tokens = estimate_tokens(&compacted);
    (compacted, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use function_providers::MessageRole;

    fn message(role: MessageRole, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
            images: None,
            tool_call_id: None,
            tool_calls: None,
            thought_signature: None,
        }
    }

    #[test]
    fn compaction_preserves_system_and_current_task() {
        let mut messages = vec![message(MessageRole::System, "system instructions")];
        messages.extend(
            (0..8).map(|index| message(MessageRole::Tool, &"old output ".repeat(600 + index))),
        );
        messages.push(message(MessageRole::User, "current task"));
        let (compacted, report) = compact_messages(
            &messages,
            ContextBudget {
                context_limit: 700,
                output_reserve: 100,
            },
        );
        assert_eq!(compacted.first().unwrap().content, "system instructions");
        assert_eq!(compacted.last().unwrap().content, "current task");
        assert!(report.removed_messages > 0 || report.truncated_outputs > 0);
        assert!(messages_fit(
            &compacted,
            ContextBudget {
                context_limit: 700,
                output_reserve: 100
            }
        ));
    }

    #[test]
    fn compaction_handles_unicode_tool_output() {
        let messages = vec![
            message(MessageRole::System, "keep this"),
            ChatMessage {
                role: MessageRole::Tool,
                content: "界".repeat(3000),
                images: None,
                tool_call_id: Some("tool-1".to_string()),
                tool_calls: None,
                thought_signature: None,
            },
            message(MessageRole::User, "current task"),
        ];
        let (compacted, report) = compact_messages(
            &messages,
            ContextBudget {
                context_limit: 700,
                output_reserve: 100,
            },
        );
        assert!(report.truncated_outputs > 0);
        assert!(messages_fit(
            &compacted,
            ContextBudget {
                context_limit: 700,
                output_reserve: 100,
            }
        ));
    }
}
