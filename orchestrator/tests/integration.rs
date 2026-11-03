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
        Capability::HttpGet { allowed_hosts: vec!["example.com".into()] },
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
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
    assert_eq!(resp.status(), StatusCode::CREATED, "upload should return 201");

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

    // 3. Audit log should reference it
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/audit/logs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(
        body_str.contains("test-mod"),
        "audit log should contain module id. Got: {body_str}"
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
