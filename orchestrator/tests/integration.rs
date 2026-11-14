//! Integration tests: full HTTP stack via tower::oneshot.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use orchestrator::capabilities::CapabilitySet;
use orchestrator::context::{AppContext, AppContextConfig};
use orchestrator::events::EventBus;
use orchestrator::module_registry::ModuleRegistry;
use orchestrator::telemetry;
use orchestrator::telemetry::ApiState;
use orchestrator::tenant::TenantRegistry;
use orchestrator::wasm::build_engine;
use shared_types::{Capability, ModuleQuota};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

async fn state() -> (ApiState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = AppContextConfig {
        db_path: tmp.path().join("t.db"),
        http_timeout: Duration::from_secs(5),
        max_db_connections: 1,
        max_sockets: 16,
        max_concurrent_wasm: 2,
        wasm_acquire_timeout: Duration::from_secs(2),
        wasm_memory_limit_mb: 128,
    };
    let caps = CapabilitySet::from_capabilities(&[
        Capability::Log,
        Capability::HttpGet {
            // Expanded to include "localhost" so per_module_capability_*
            // tests can request a restricted module capability set
            // ("localhost" only) that is still a subset of the ceiling.
            // Without this, the M4 capability-ceiling enforcement
            // (resolve_capabilities intersects header with
            // ctx.capabilities) rejects the upload with 400 before the
            // runtime deny test ever executes. See commit 9c5f3d5.
            allowed_hosts: vec!["example.com".into(), "localhost".into()],
        },
    ]);
    let bus = EventBus::new(16);
    let ctx = Arc::new(AppContext::build(cfg, caps, bus).await.unwrap());
    let reg = Arc::new(ModuleRegistry::new(
        build_engine().unwrap(),
        vec![],
        ModuleQuota::default(),
        128,
        Duration::from_secs(2),
    ));
    let tenants = Arc::new(
        TenantRegistry::from_config(Default::default(), "default".into(), Duration::from_secs(2))
            .unwrap(),
    );
    let prom = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder()
        .handle();
    let s = ApiState {
        prom,
        ctx,
        registry: reg,
        started_at: std::time::Instant::now(),
        modules_dir: tmp.path().to_path_buf(),
        tenants,
        build_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        cargo_home: std::path::PathBuf::from("/nonexistent"),
        require_auth: false,
        enable_build: false,
        enable_build_network: false,
        data_dir: tmp.path().to_path_buf(),
        wizard: orchestrator::wizard::WizardState::new(tmp.path().to_path_buf()),
    };
    (s, tmp)
}

async fn state_with_auth() -> (ApiState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = AppContextConfig {
        db_path: tmp.path().join("t.db"),
        http_timeout: Duration::from_secs(5),
        max_db_connections: 1,
        max_sockets: 16,
        max_concurrent_wasm: 2,
        wasm_acquire_timeout: Duration::from_secs(2),
        wasm_memory_limit_mb: 128,
    };
    let caps = CapabilitySet::from_capabilities(&[Capability::Log]);
    let bus = EventBus::new(16);
    let ctx = Arc::new(AppContext::build(cfg, caps, bus).await.unwrap());
    let reg = Arc::new(ModuleRegistry::new(
        build_engine().unwrap(),
        vec![],
        ModuleQuota::default(),
        128,
        Duration::from_secs(2),
    ));
    let mut tmap = std::collections::HashMap::new();
    tmap.insert(
        "default".to_string(),
        orchestrator::config::TenantConfig {
            token_hash: None,
            token_plain: Some("test-token".to_string()),
            max_modules: None,
            max_concurrent_wasm: None,
            rate_limit_per_sec: None,
        },
    );
    let tenants = Arc::new(
        TenantRegistry::from_config(tmap, "default".into(), Duration::from_secs(2)).unwrap(),
    );
    let prom = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder()
        .handle();
    let s = ApiState {
        prom,
        ctx,
        registry: reg,
        started_at: std::time::Instant::now(),
        modules_dir: tmp.path().to_path_buf(),
        tenants,
        build_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        cargo_home: std::path::PathBuf::from("/nonexistent"),
        require_auth: true,
        enable_build: false,
        enable_build_network: false,
        data_dir: tmp.path().to_path_buf(),
        wizard: orchestrator::wizard::WizardState::new(tmp.path().to_path_buf()),
    };
    (s, tmp)
}

fn sample_wasm() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../modules/default/sample.wasm"
    ))
    .unwrap()
}

#[tokio::test]
async fn healthz_returns_ok() {
    let (s, _tmp) = state().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn upload_execute_audit_roundtrip() {
    let (s, _tmp) = state().await;
    let app = telemetry::build_router(s);

    // 1. Upload sample module
    let wasm = sample_wasm();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules")
                .header("x-module-id", "test-mod")
                .body(Body::from(wasm))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "upload should return 201"
    );

    // 2. Execute it
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules/test-mod/execute")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "execute should return 200");

    // 3. Audit log should reference it.
    //
    // v0.48.0 batches audit writes: an execute response returns as
    // soon as the AuditEntry lands on the bounded channel, and the
    // writer coalesces rows into one INSERT per 50 ms or 100 rows.
    // The row therefore appears up to ~50 ms after the response.
    // Poll for up to 2 s before failing; anything longer is a real
    // regression, not the batching window.
    let mut body_str = String::new();
    let mut found = false;
    for _ in 0..40 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/audit/logs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        body_str = String::from_utf8(body.to_vec()).unwrap();
        if body_str.contains("test-mod") {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        found,
        "audit log should contain module id within 2 s. Got: {body_str}"
    );
}

#[tokio::test]
async fn unknown_module_returns_404() {
    let (s, _tmp) = state().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules/does-not-exist/execute")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn auth_required_401_without_header() {
    let (s, _tmp) = state_with_auth().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/modules")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_accepts_valid_bearer() {
    let (s, _tmp) = state_with_auth().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/modules")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn auth_healthz_public() {
    let (s, _tmp) = state_with_auth().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn build_disabled_by_default() {
    let (s, _tmp) = state().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules/build")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED,
        "build route should be unavailable, got {status}"
    );
}

#[tokio::test]
async fn per_module_capability_denies_wrong_host() {
    let (s, _tmp) = state().await;
    let app = telemetry::build_router(s);

    // Restrict http to "localhost" only. sample.wasm tries example.com -> denied.
    let caps_json = r#"[{"type":"http-get","allowed_hosts":["localhost"]}]"#;
    let wasm = sample_wasm();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules")
                .header("x-module-id", "restricted-mod")
                .header("x-module-capabilities", caps_json)
                .body(Body::from(wasm))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules/restricted-mod/execute")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(
        body_str.contains("\"http\":{")
            && body_str.contains("\"ok\":false")
            && body_str.contains("capability denied")
            && body_str.contains("example.com"),
        "expected capability denial for example.com. Body: {body_str}"
    );
}

#[tokio::test]
#[ignore = "requires network: guest calls GET https://example.com and TCP :80; offline CI would return http/tcp errors and the success assertion would fail"]
async fn per_module_capability_allows_matching_host() {
    let (s, _tmp) = state().await;
    let app = telemetry::build_router(s);

    let caps_json = r#"[{"type":"http-get","allowed_hosts":["example.com"]},{"type":"tcp-connect","allowed_ports":[80]}]"#;
    let wasm = sample_wasm();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules")
                .header("x-module-id", "allowed-mod")
                .header("x-module-capabilities", caps_json)
                .body(Body::from(wasm))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules/allowed-mod/execute")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(
        body_str.contains("\"http\":{\"len\":") || body_str.contains("\"ok\":true"),
        "expected http success. Body: {body_str}"
    );
}

#[tokio::test]
async fn no_caps_header_falls_back_to_global() {
    let (s, _tmp) = state().await;
    let app = telemetry::build_router(s);
    let wasm = sample_wasm();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules")
                .header("x-module-id", "global-caps-mod")
                .body(Body::from(wasm))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/modules/global-caps-mod/execute")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── M4: multi-tenant middleware isolation ─────────────────────────────
//
// auth_middleware used to hardcode `default_name()` when verifying
// Bearer tokens, so any /t/{tenant}/... route would reject legitimate
// tenant tokens before the handler ran. Now it extracts the tenant
// from the path (extract_tenant_from_path) and verifies against that
// tenant's token. These tests pin the fix.

async fn state_with_multi_tenant() -> (ApiState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = AppContextConfig {
        db_path: tmp.path().join("t.db"),
        http_timeout: Duration::from_secs(5),
        max_db_connections: 1,
        max_sockets: 16,
        max_concurrent_wasm: 2,
        wasm_acquire_timeout: Duration::from_secs(2),
        wasm_memory_limit_mb: 128,
    };
    let caps = CapabilitySet::from_capabilities(&[Capability::Log]);
    let bus = EventBus::new(16);
    let ctx = Arc::new(AppContext::build(cfg, caps, bus).await.unwrap());
    let reg = Arc::new(ModuleRegistry::new(
        build_engine().unwrap(),
        vec![],
        ModuleQuota::default(),
        128,
        Duration::from_secs(2),
    ));
    let mut tmap = std::collections::HashMap::new();
    for (name, token) in [
        ("default", "default-token"),
        ("alice", "alice-token"),
        ("bob", "bob-token"),
    ] {
        tmap.insert(
            name.to_string(),
            orchestrator::config::TenantConfig {
                token_hash: None,
                token_plain: Some(token.to_string()),
                max_modules: None,
                max_concurrent_wasm: None,
                rate_limit_per_sec: None,
            },
        );
    }
    let tenants = Arc::new(
        TenantRegistry::from_config(tmap, "default".into(), Duration::from_secs(2)).unwrap(),
    );
    let prom = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder()
        .handle();
    let s = ApiState {
        prom,
        ctx,
        registry: reg,
        started_at: std::time::Instant::now(),
        modules_dir: tmp.path().to_path_buf(),
        tenants,
        build_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        cargo_home: std::path::PathBuf::from("/nonexistent"),
        require_auth: true,
        enable_build: false,
        enable_build_network: false,
        data_dir: tmp.path().to_path_buf(),
        wizard: orchestrator::wizard::WizardState::new(tmp.path().to_path_buf()),
    };
    (s, tmp)
}

#[tokio::test]
async fn m4_tenant_alice_200_with_alice_token() {
    let (s, _tmp) = state_with_multi_tenant().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/t/alice/modules")
                .header("authorization", "Bearer alice-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn m4_tenant_alice_401_with_default_token() {
    let (s, _tmp) = state_with_multi_tenant().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/t/alice/modules")
                .header("authorization", "Bearer default-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn m4_tenant_alice_401_with_bob_token() {
    let (s, _tmp) = state_with_multi_tenant().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/t/alice/modules")
                .header("authorization", "Bearer bob-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn m4_tenant_unknown_path_401() {
    let (s, _tmp) = state_with_multi_tenant().await;
    let app = telemetry::build_router(s);

    // Unknown tenant in path: verify() returns UnknownTenant -> 401.
    // Security-wise we deliberately return 401 (not 404) so callers
    // cannot enumerate which tenant names exist.
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/t/charlie/modules")
                .header("authorization", "Bearer alice-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn m4_healthz_public_in_multi_tenant() {
    let (s, _tmp) = state_with_multi_tenant().await;
    let app = telemetry::build_router(s);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn m4_default_route_still_uses_default_token() {
    let (s, _tmp) = state_with_multi_tenant().await;
    let app = telemetry::build_router(s);

    // /modules (no /t/ prefix) must still verify against default tenant
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/modules")
                .header("authorization", "Bearer alice-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let resp = telemetry::build_router(state_with_multi_tenant().await.0)
        .oneshot(
            Request::builder()
                .uri("/modules")
                .header("authorization", "Bearer default-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
