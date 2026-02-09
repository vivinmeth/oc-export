use std::collections::HashMap;

use crate::loader::StorageData;
use crate::types::*;

/// Build fully resolved projects from raw storage data.
pub fn resolve(
    data: &StorageData,
    project_filter: Option<&str>,
    session_filter: Option<&str>,
    since_ms: Option<u64>,
) -> Vec<ResolvedProject> {
    let mut result = Vec::new();

    for project in &data.projects {
        // Apply project filter (match on worktree path or project id)
        if let Some(filter) = project_filter {
            let matches = project.worktree.contains(filter)
                || project.id.starts_with(filter)
                || project.display_name().eq_ignore_ascii_case(filter);
            if !matches {
                continue;
            }
        }

        let session_ids = match data.sessions_by_project.get(&project.id) {
            Some(ids) => ids,
            None => continue,
        };

        // Collect all sessions for this project
        let mut all_sessions: Vec<&Session> = session_ids
            .iter()
            .filter_map(|id| data.sessions.get(id))
            .collect();
        all_sessions.sort_by_key(|s| s.time.created.unwrap_or(0));

        // Build a set of sub-agent session IDs (those with a parentID)
        let sub_agent_ids: std::collections::HashSet<&str> = all_sessions
            .iter()
            .filter(|s| s.parent_id.is_some())
            .map(|s| s.id.as_str())
            .collect();

        // Map parent_session_id -> Vec<child Session>
        let mut children_by_parent: HashMap<&str, Vec<&Session>> = HashMap::new();
        for s in &all_sessions {
            if let Some(ref pid) = s.parent_id {
                children_by_parent.entry(pid.as_str()).or_default().push(s);
            }
        }

        let mut resolved_sessions = Vec::new();

        for session in &all_sessions {
            // Skip sub-agent sessions at the top level (they'll be inlined)
            if sub_agent_ids.contains(session.id.as_str()) {
                continue;
            }

            // Apply session filter
            if let Some(filter) = session_filter {
                if session.id != filter {
                    continue;
                }
            }

            // Apply date filter
            if let Some(since) = since_ms {
                if session.time.created.unwrap_or(0) < since {
                    continue;
                }
            }

            let resolved = resolve_session(session, data, &children_by_parent);
            resolved_sessions.push(resolved);
        }

        if !resolved_sessions.is_empty() {
            result.push(ResolvedProject {
                project: project.clone(),
                sessions: resolved_sessions,
            });
        }
    }

    result
}

fn resolve_session(
    session: &Session,
    data: &StorageData,
    children_by_parent: &HashMap<&str, Vec<&Session>>,
) -> ResolvedSession {
    let messages = data
        .messages_by_session
        .get(&session.id)
        .cloned()
        .unwrap_or_default();

    let child_sessions = children_by_parent
        .get(session.id.as_str())
        .cloned()
        .unwrap_or_default();

    // Build the conversation flow, inlining sub-agent sessions
    let conversation = build_conversation(&messages, &child_sessions, data, children_by_parent);

    // Collect diffs and todos
    let diffs = data
        .diffs_by_session
        .get(&session.id)
        .cloned()
        .unwrap_or_default();
    let todos = data
        .todos_by_session
        .get(&session.id)
        .cloned()
        .unwrap_or_default();

    // Sum up tokens across all assistant messages
    let token_totals = sum_tokens(&messages);

    ResolvedSession {
        session: session.clone(),
        messages: conversation,
        diffs,
        todos,
        token_totals,
    }
}

fn build_conversation(
    messages: &[Message],
    child_sessions: &[&Session],
    data: &StorageData,
    children_by_parent: &HashMap<&str, Vec<&Session>>,
) -> Vec<ResolvedConversationItem> {
    let mut items = Vec::new();

    // Index child sessions by their creation time so we can interleave them
    let mut child_by_time: Vec<(&Session, u64)> = child_sessions
        .iter()
        .map(|s| (*s, s.time.created.unwrap_or(0)))
        .collect();
    child_by_time.sort_by_key(|(_, t)| *t);

    let mut child_idx = 0;

    for msg in messages {
        let msg_time = msg.time.created.unwrap_or(0);

        // Insert any sub-agent sessions that started before this message
        while child_idx < child_by_time.len() && child_by_time[child_idx].1 <= msg_time {
            let child_session = child_by_time[child_idx].0;
            let child_resolved = resolve_session(child_session, data, children_by_parent);
            items.push(ResolvedConversationItem::SubAgent {
                session: child_resolved.session.clone(),
                messages: child_resolved.messages,
            });
            child_idx += 1;
        }

        // Resolve parts for this message
        let parts = data
            .parts_by_message
            .get(&msg.id)
            .cloned()
            .unwrap_or_default();

        items.push(ResolvedConversationItem::Message(ResolvedMessage {
            message: msg.clone(),
            parts,
        }));
    }

    // Append any remaining child sessions
    while child_idx < child_by_time.len() {
        let child_session = child_by_time[child_idx].0;
        let child_resolved = resolve_session(child_session, data, children_by_parent);
        items.push(ResolvedConversationItem::SubAgent {
            session: child_resolved.session.clone(),
            messages: child_resolved.messages,
        });
        child_idx += 1;
    }

    items
}

fn sum_tokens(messages: &[Message]) -> Tokens {
    let mut total = Tokens {
        input: Some(0),
        output: Some(0),
        reasoning: Some(0),
        cache: TokenCache {
            read: Some(0),
            write: Some(0),
        },
    };
    for m in messages {
        if let Some(ref t) = m.tokens {
            *total.input.as_mut().unwrap() += t.input.unwrap_or(0);
            *total.output.as_mut().unwrap() += t.output.unwrap_or(0);
            *total.reasoning.as_mut().unwrap() += t.reasoning.unwrap_or(0);
            *total.cache.read.as_mut().unwrap() += t.cache.read.unwrap_or(0);
            *total.cache.write.as_mut().unwrap() += t.cache.write.unwrap_or(0);
        }
    }
    total
}
