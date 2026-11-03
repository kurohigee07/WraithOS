//! Web dashboard (HTML + HTMX) served from memory.
//! Fragments return HTML <tr> rows consumed by HTMX polling.

use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::Router;

use crate::module::OrchestratorModule;
use crate::telemetry::ApiState;

const DASHBOARD: &str = include_str!("dashboard.html");

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/ui", get(dashboard))
        .route("/ui/fragments/stats", get(stats_fragment))
        .route("/ui/fragments/modules", get(modules_fragment))
        .route("/ui/fragments/logs", get(logs_fragment))
        .route("/ui/fragments/metrics", get(metrics_fragment))
}

async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD)
}

async fn stats_fragment(State(s): State<ApiState>) -> Html<String> {
    let uptime = fmt_uptime(s.started_at.elapsed().as_secs());
    let modules = s.registry.all().len();
    let total_exec: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_logs")
        .fetch_one(&s.ctx.db)
        .await
        .unwrap_or(0);
    let recent_err: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_logs WHERE status='error' AND executed_at > datetime('now','-1 hour')"
    ).fetch_one(&s.ctx.db).await.unwrap_or(0);

    let err_class = if recent_err > 0 { "err" } else { "ok" };
    Html(format!(
        concat!(
            "<div class=\"stat\"><div class=\"label\">Uptime</div><div class=\"value\">{}</div></div>",
            "<div class=\"stat\"><div class=\"label\">Modules</div><div class=\"value\">{}</div></div>",
            "<div class=\"stat\"><div class=\"label\">Total Executions</div><div class=\"value\">{}</div></div>",
            "<div class=\"stat\"><div class=\"label\">Errors (1h)</div><div class=\"value {}\">{}</div></div>"
        ),
        uptime, modules, total_exec, err_class, recent_err,
    ))
}

fn fmt_uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{}d {}h", d, h)
    } else if h > 0 {
        format!("{}h {}m", h, m)
    } else {
        format!("{}m {}s", m, secs % 60)
    }
}

async fn modules_fragment(State(s): State<ApiState>) -> Html<String> {
    let mut html = String::new();
    for (tenant, id, m) in s.registry.all() {
        let manifest = m.manifest();
        let q = &manifest.quota;
        let quota_str = format!(
            "mem:{} conc:{} rate:{}/s",
            q.max_memory_mb, q.max_concurrent, q.rate_limit_per_sec
        );
        html.push_str(&format!(
            concat!(
                "<tr>",
                "<td>{tenant}</td>",
                "<td><strong>{id}</strong></td>",
                "<td>{ver}</td>",
                "<td style=\"color:var(--muted);font-size:11px\">{quota}</td>",
                "<td><button class=\"btn\" onclick=\"execModule('{id}')\">Execute</button></td>",
                "</tr>"
            ),
            tenant = tenant,
            id = id,
            ver = manifest.version,
            quota = quota_str,
        ));
    }
    if html.is_empty() {
        html.push_str("<tr><td colspan=\"5\" style=\"text-align:center;color:var(--muted);padding:24px\">No modules loaded</td></tr>");
    }
    Html(html)
}

async fn logs_fragment(State(s): State<ApiState>) -> Html<String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        tenant: String,
        module_id: String,
        status: String,
        duration_ms: i64,
        executed_at: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, tenant, module_id, status, duration_ms, executed_at FROM execution_logs ORDER BY id DESC LIMIT 20"
    ).fetch_all(&s.ctx.db).await.unwrap_or_default();

    let mut html = String::new();
    for r in rows {
        let class = if r.status == "ok" { "ok" } else { "error" };
        html.push_str(&format!(
            concat!(
                "<tr>",
                "<td>{id}</td>",
                "<td>{tenant}</td>",
                "<td>{module_id}</td>",
                "<td><span class=\"status {class}\">{status}</span></td>",
                "<td>{dur}ms</td>",
                "<td style=\"color:var(--muted);font-size:11px\">{when}</td>",
                "</tr>"
            ),
            id = r.id,
            tenant = r.tenant,
            module_id = r.module_id,
            class = class,
            status = r.status,
            dur = r.duration_ms,
            when = r.executed_at,
        ));
    }
    if html.is_empty() {
        html.push_str("<tr><td colspan=\"6\" style=\"text-align:center;color:var(--muted);padding:24px\">No executions yet</td></tr>");
    }
    Html(html)
}

async fn metrics_fragment(State(s): State<ApiState>) -> Html<String> {
    let text = s.prom.render();
    let exec_total = sum_metric(&text, "wasm_executions_total");
    let exec_err = sum_metric_label(&text, "wasm_executions_total", "result=\"error\"");
    let rhai_total = sum_metric(&text, "rhai_events_processed_total");
    let pool_active = sum_metric(&text, "wasm_pool_in_flight");
    let sockets = sum_metric(&text, "socket_registry_active");
    let err_class = if exec_err > 0.0 { "err" } else { "ok" };

    Html(format!(
        concat!(
            "<div class=\"stats\">",
            "<div class=\"stat\"><div class=\"label\">Wasm Executions</div><div class=\"value\">{}</div></div>",
            "<div class=\"stat\"><div class=\"label\">Wasm Errors</div><div class=\"value {}\">{}</div></div>",
            "<div class=\"stat\"><div class=\"label\">Rhai Events</div><div class=\"value\">{}</div></div>",
            "<div class=\"stat\"><div class=\"label\">Pool In-Flight</div><div class=\"value\">{}</div></div>",
            "<div class=\"stat\"><div class=\"label\">Active Sockets</div><div class=\"value\">{}</div></div>",
            "</div>",
            "<p style=\"color:var(--muted);margin-top:24px\">Raw Prometheus endpoint: <a href=\"/metrics\" style=\"color:var(--accent)\">/metrics</a></p>"
        ),
        exec_total as u64, err_class, exec_err as u64,
        rhai_total as u64, pool_active as u64, sockets as u64,
    ))
}

fn sum_metric(text: &str, name: &str) -> f64 {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter(|l| l.starts_with(name))
        .filter_map(|l| l.rsplit_once(' ').and_then(|(_, v)| v.parse::<f64>().ok()))
        .sum()
}

fn sum_metric_label(text: &str, name: &str, label_substr: &str) -> f64 {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter(|l| l.starts_with(name))
        .filter(|l| l.contains(label_substr))
        .filter_map(|l| l.rsplit_once(' ').and_then(|(_, v)| v.parse::<f64>().ok()))
        .sum()
}
