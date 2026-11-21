//! Antigravity CLI (`agy`) specific metrics. All return `None` for Claude Code
//! payloads so they can sit safely in a shared `config.toml`.

use super::{Ctx, Metric, MetricMeta, Options, Reading};
use crate::style::{Category, IconKey, Severity};
use crate::widgets::WidgetKind;

fn meta(
    id: &'static str,
    category: Category,
    name: &'static str,
    summary: &'static str,
    icon: IconKey,
    templates: [&'static str; 3],
    fields: &'static [&'static str],
) -> MetricMeta {
    MetricMeta {
        id,
        category,
        name,
        summary,
        icon,
        widget: WidgetKind::Text,
        templates,
        fields,
        levels: &[],
        min_cc: None,
        needs_mod: false,
        options: &[],
        stateful: false,
        priority: 3,
    }
}

pub(super) fn metrics() -> Vec<Metric> {
    vec![
        Metric {
            meta: MetricMeta {
                priority: 4,
                ..meta(
                    "agy.state",
                    Category::Identity,
                    "Antigravity state",
                    "Current Antigravity agent state (idle, working, tool, review, confirm, error)",
                    IconKey::Thinking,
                    ["{label}", "{label}", "{short}"],
                    &["label", "short", "raw"],
                )
            },
            compute: agent_state,
        },
        Metric {
            meta: MetricMeta {
                priority: 3,
                ..meta(
                    "agy.mode",
                    Category::Identity,
                    "Antigravity mode",
                    "Active Shift+Tab cycle mode (Plan, Edits, Auto)",
                    IconKey::OutputStyle,
                    ["{label}", "{label}", "{short}"],
                    &["label", "short", "raw"],
                )
            },
            compute: cycle_mode,
        },
        Metric {
            meta: MetricMeta {
                priority: 2,
                ..meta(
                    "agy.sandbox",
                    Category::System,
                    "Antigravity sandbox",
                    "Shown when the Antigravity terminal sandbox is enabled",
                    IconKey::Tools,
                    ["{label}", "{label}", "{short}"],
                    &["label", "short"],
                )
            },
            compute: sandbox,
        },
        Metric {
            meta: MetricMeta {
                priority: 2,
                ..meta(
                    "agy.subagents",
                    Category::Session,
                    "Antigravity subagents",
                    "Running and total subagents in the active Antigravity session",
                    IconKey::Agent,
                    ["{count} sub", "{count}", "{running}"],
                    &["count", "running", "total"],
                )
            },
            compute: subagents,
        },
        Metric {
            meta: MetricMeta {
                priority: 2,
                ..meta(
                    "agy.tasks",
                    Category::Session,
                    "Antigravity tasks",
                    "Running background tasks and artifacts in Antigravity",
                    IconKey::Tools,
                    ["[{tasks} task][ · {artifacts} art]", "[{tasks}t][ {artifacts}a]", "{total}"],
                    &["tasks", "artifacts", "total"],
                )
            },
            compute: tasks,
        },
    ]
}

fn agent_state(ctx: &Ctx, _: &Options) -> Option<Reading> {
    if ctx.input.tool_confirmation_pending == Some(true) {
        return Some(
            Reading::default()
                .set("label", "confirm")
                .set("short", "!")
                .set("raw", "tool_confirmation_pending")
                .severity(Severity::Warn),
        );
    }
    let raw = ctx.input.agent_state.as_deref().filter(|s| !s.is_empty())?;
    let (label, short, severity) = match raw {
        "idle" => ("idle", "ok", Severity::Good),
        "working" => ("working", "run", Severity::Normal),
        "tool_use" => ("tool", "tool", Severity::Warn),
        "reviewing" => ("review", "rev", Severity::Warn),
        "authenticating" => ("auth", "auth", Severity::Warn),
        "initializing" => ("init", "init", Severity::Normal),
        "error" => ("error", "err", Severity::Crit),
        other => (other, other, Severity::Normal),
    };
    Some(
        Reading::default()
            .set("label", label)
            .set("short", short)
            .set("raw", raw)
            .severity(severity),
    )
}

fn cycle_mode(ctx: &Ctx, _: &Options) -> Option<Reading> {
    let raw = ctx.input.cycle_mode.as_deref().filter(|s| !s.is_empty())?;
    let (label, short) = match raw {
        "plan" => ("Plan", "P"),
        "accept-edits" => ("Edits", "E"),
        "auto" => ("Auto", "A"),
        other => (other, other),
    };
    Some(
        Reading::default()
            .set("label", label)
            .set("short", short)
            .set("raw", raw),
    )
}

fn sandbox(ctx: &Ctx, _: &Options) -> Option<Reading> {
    if ctx.input.sandbox.as_ref().and_then(|s| s.enabled) != Some(true) {
        return None;
    }
    Some(Reading::default().set("label", "sandbox").set("short", "sbx"))
}

fn subagents(ctx: &Ctx, _: &Options) -> Option<Reading> {
    let total = ctx.input.subagents.len();
    if total == 0 {
        return None;
    }
    let running = ctx
        .input
        .subagents
        .iter()
        .filter(|s| s.status.as_deref() == Some("running"))
        .count();
    let count = if running == total {
        total.to_string()
    } else {
        format!("{running}/{total}")
    };
    let mut r = Reading::default()
        .set("count", count)
        .set("running", running.to_string())
        .set("total", total.to_string());
    if running > 0 {
        r = r.severity(Severity::Good);
    }
    Some(r)
}

fn tasks(ctx: &Ctx, _: &Options) -> Option<Reading> {
    let tasks = ctx.input.task_count.unwrap_or(0);
    let artifacts = ctx.input.artifact_count.unwrap_or(0);
    let total = tasks + artifacts;
    if total == 0 {
        return None;
    }
    let mut r = Reading::default().set("total", total.to_string());
    if tasks > 0 {
        r = r.set("tasks", tasks.to_string());
    }
    if artifacts > 0 {
        r = r.set("artifacts", artifacts.to_string());
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;
    use crate::metrics::{find, read};
    use crate::models::Registry;
    use crate::style::GlyphTier;
    use chrono::FixedOffset;

    fn render_metric(json: &str, id: &str) -> Option<String> {
        let input = Input::parse_at(json, 1_000_000).unwrap();
        let reg = Registry::default();
        let ctx = Ctx::new(
            &input,
            &reg,
            1_000_000,
            FixedOffset::east_opt(0).unwrap(),
            GlyphTier::Unicode,
        );
        let m = find(id)?;
        let r = read(m, &ctx, &Options::new())?;
        Some(
            crate::widgets::template::render(m.meta.templates[0], &r)
                .iter()
                .map(|p| p.text.as_str())
                .collect(),
        )
    }

    #[test]
    fn hidden_on_claude_code_payloads() {
        let cc = r#"{"version": "2.1.285", "model": {"id": "claude-opus-5-5"}}"#;
        for id in [
            "agy.state",
            "agy.mode",
            "agy.sandbox",
            "agy.subagents",
            "agy.tasks",
        ] {
            assert_eq!(render_metric(cc, id), None, "{id} should be hidden");
        }
    }

    #[test]
    fn renders_antigravity_metrics() {
        let agy = r#"{
            "product": "antigravity",
            "version": "1.5.14",
            "agent_state": "working",
            "cycle_mode": "plan",
            "sandbox": {"enabled": true},
            "subagents": [
                {"name": "a1", "role": "research", "status": "running"},
                {"name": "a2", "role": "review", "status": "completed"}
            ],
            "task_count": 1,
            "artifact_count": 2
        }"#;
        assert_eq!(render_metric(agy, "agy.state").as_deref(), Some("working"));
        assert_eq!(render_metric(agy, "agy.mode").as_deref(), Some("Plan"));
        assert_eq!(render_metric(agy, "agy.sandbox").as_deref(), Some("sandbox"));
        assert_eq!(
            render_metric(agy, "agy.subagents").as_deref(),
            Some("1/2 sub")
        );
        assert_eq!(
            render_metric(agy, "agy.tasks").as_deref(),
            Some("1 task · 2 art")
        );
    }

    #[test]
    fn confirmation_pending_overrides_agent_state() {
        let agy = r#"{"product": "antigravity", "agent_state": "tool_use", "tool_confirmation_pending": true}"#;
        assert_eq!(render_metric(agy, "agy.state").as_deref(), Some("confirm"));
    }
}
