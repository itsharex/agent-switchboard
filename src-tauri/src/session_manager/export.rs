use super::{organization, parser::read_messages, resolve_session, SessionMessage, SessionMeta};
use asb_core::contracts::AppKind;
use std::path::Path;

pub fn export_markdown(root: &Path, app: AppKind, session_id: &str) -> Result<String, String> {
    let mut source = resolve_session(app, session_id)?;
    organization::apply(&mut source.meta, &organization::load(root)?);
    let messages = read_messages(&source.path)?;
    Ok(render_markdown(&source.meta, &messages))
}

fn render_markdown(meta: &SessionMeta, messages: &[SessionMessage]) -> String {
    let mut output = format!(
        "# {}\n\n- 来源：{}\n- 会话 ID：{}\n",
        escape(meta.alias.as_deref().unwrap_or(&meta.title)), meta.app.label(), escape(&meta.session_id),
    );
    for (label, value) in [
        ("项目", meta.project_dir.as_deref()),
        ("创建时间", meta.created_at.as_deref()),
        ("最后活动", meta.last_active_at.as_deref()),
    ] {
        if let Some(value) = value { output.push_str(&format!("- {label}：{}\n", escape(value))); }
    }
    for message in messages {
        output.push_str(&format!("\n---\n\n## {}", escape(&message.role)));
        if let Some(at) = &message.at { output.push_str(&format!(" · {}", escape(at))); }
        output.push_str("\n\n");
        output.push_str(&message.content);
        output.push('\n');
    }
    output
}

fn escape(text: &str) -> String {
    text.chars().flat_map(|character| {
        if "\\`*_{}[]<>()#+-.!|".contains(character) { vec!['\\', character] }
        else if character == '\n' || character == '\r' { vec![' '] }
        else { vec![character] }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_complete_markdown_content_and_escapes_metadata() {
        let meta = SessionMeta { app: AppKind::Codex, session_id: "s-1".into(),
            title: "[title]".into(), summary: String::new(), project_dir: None,
            created_at: Some("2026-09-22T00:00:00Z".into()), last_active_at: None,
            resume_command: String::new(), alias: None, pinned: false, tags: Vec::new() };
        let content = "```rust\nlet x = 1;\n```\n\n完整正文";
        let markdown = render_markdown(&meta, &[SessionMessage { id: "id".into(),
            role: "assistant".into(), content: content.into(), at: None }]);
        assert!(markdown.starts_with("# \\[title\\]"));
        assert!(markdown.contains(content));
        assert!(markdown.contains("来源：Codex"));
        assert!(markdown.contains("创建时间：2026"));
    }
}
