use alta_application::reference_items::{CreateReferenceItem, ReferenceItemRepository};
use alta_application::{events::OutboxRepository, idempotency::IdempotencyRepository, jobs::JobLeaseRepository};
use alta_infrastructure::postgres::{self, PgRepositories};
use alta_kernel::TenantId;

async fn pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }
    match postgres::connect(&url).await {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("SKIP pg_gates: connect failed: {e}");
            None
        }
    }
}

fn title(s: &str) -> CreateReferenceItem {
    CreateReferenceItem { title: s.to_string() }
}

#[tokio::test]
async fn tenant_isolation() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    let a = TenantId::new();
    let b = TenantId::new();
    let (item_a, created) = postgres::create_reference_item_atomic(&pool, a, title("iso-a"), None)
        .await
        .expect("create A");
    assert!(created);
    let list_a = postgres::list_reference_items(&pool, a, alta_kernel::PageRequest { limit: 50, cursor: None })
        .await
        .expect("list A");
    assert!(list_a.items.iter().any(|i| i.id == item_a.id));
    let list_b = postgres::list_reference_items(&pool, b, alta_kernel::PageRequest { limit: 50, cursor: None })
        .await
        .expect("list B");
    assert!(list_b.items.is_empty(), "tenant B must not see tenant A rows");
}

#[tokio::test]
async fn idempotency_sequential_replay() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    let t = TenantId::new();
    let key = format!("seq-{}", uuid::Uuid::new_v4());
    let (first, c1) = postgres::create_reference_item_atomic(&pool, t, title("seq"), Some(key.clone()))
        .await
        .expect("first");
    assert!(c1);
    let (second, c2) = postgres::create_reference_item_atomic(&pool, t, title("seq"), Some(key))
        .await
        .expect("replay");
    assert!(!c2);
    assert_eq!(first.id, second.id);
}

#[tokio::test]
async fn idempotency_concurrent_single_winner() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    let t = TenantId::new();
    let key = format!("race-{}", uuid::Uuid::new_v4());
    let mut handles = Vec::new();
    for _ in 0..30 {
        let p = pool.clone();
        let k = key.clone();
        handles.push(tokio::spawn(async move {
            postgres::create_reference_item_atomic(&p, t, title("race"), Some(k)).await
        }));
    }
    let mut ids = Vec::new();
    for h in handles {
        let (item, _) = h.await.expect("task").expect("create");
        ids.push(item.id);
    }
    let first = ids[0];
    assert!(ids.iter().all(|id| *id == first), "concurrent same-key must yield single id, got {:?}", ids);
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM reference_items WHERE tenant_id = $1")
        .bind(t.0)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count.0, 1, "exactly one business row expected");
    let ocount: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM platform_outbox WHERE tenant_id = $1")
        .bind(t.0)
        .fetch_one(&pool)
        .await
        .expect("ocount");
    assert_eq!(ocount.0, 1, "exactly one outbox row expected");
}

#[tokio::test]
async fn outbox_atomicity_and_validation() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    let t = TenantId::new();
    let before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM platform_outbox WHERE tenant_id = $1")
        .bind(t.0)
        .fetch_one(&pool)
        .await
        .expect("before");
    let res = postgres::create_reference_item_atomic(&pool, t, title("   "), None).await;
    assert!(res.is_err(), "empty title must fail");
    let after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM platform_outbox WHERE tenant_id = $1")
        .bind(t.0)
        .fetch_one(&pool)
        .await
        .expect("after");
    assert_eq!(before.0, after.0, "validation failure must not create outbox");
    let (item, _) = postgres::create_reference_item_atomic(&pool, t, title("ok-atomic"), None)
        .await
        .expect("create");
    let matched: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM platform_outbox WHERE aggregate_id = $1 AND tenant_id = $2",
    )
    .bind(item.id)
    .bind(t.0)
    .fetch_one(&pool)
    .await
    .expect("matched");
    assert_eq!(matched.0, 1);
}

#[tokio::test]
async fn worker_claim_is_atomic_and_restart_safe() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    let t = TenantId::new();
    for i in 0..5 {
        postgres::create_reference_item_atomic(&pool, t, title(&format!("w-{i}")), None)
            .await
            .expect("seed");
    }
    let pub1 = alta_infrastructure::workers::OutboxPublisher::new(pool.clone());
    let batch = pub1.publish_batch().await.expect("batch");
    assert!(!batch.is_empty(), "worker must claim seeded rows");
    let batch2 = pub1.publish_batch().await.expect("batch2");
    let pending: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM platform_outbox WHERE tenant_id = $1 AND dispatched_at IS NULL",
    )
    .bind(t.0)
    .fetch_one(&pool)
    .await
    .expect("pending");
    assert_eq!(pending.0, 0, "all seeded rows must be dispatched, second batch must not re-claim same tenant rows exclusively but global pending may remain from other tests; tenant pending must be 0");
    let _ = batch2;
}

#[tokio::test]
async fn ports_are_implemented() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    let repos = PgRepositories::new(pool.clone());
    let t = TenantId::new();
    let (_item, _) = ReferenceItemRepository::create(&repos, t, title("via-port"), None)
        .await
        .expect("port create");
    let _ = ReferenceItemRepository::list(&repos, t, alta_kernel::PageRequest { limit: 10, cursor: None })
        .await
        .expect("port list");
    let _ = OutboxRepository::count_pending(&repos).await.expect("count_pending");
    let _ = IdempotencyRepository::probe(&repos).await.expect("probe");
    let _ = JobLeaseRepository::pending_count(&repos).await.expect("pending_count");
}

#[tokio::test]
async fn idempotency_key_length_enforced() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    let t = TenantId::new();
    let long = "k".repeat(129);
    let res = postgres::create_reference_item_atomic(&pool, t, title("long-key"), Some(long)).await;
    assert!(res.is_err(), "key >128 must be rejected");
}

#[tokio::test]
async fn pagination_cursor_paginates() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    let t = TenantId::new();
    for i in 0..5 {
        postgres::create_reference_item_atomic(&pool, t, title(&format!("page-{i}")), None)
            .await
            .expect("seed");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let p1 = postgres::list_reference_items(&pool, t, alta_kernel::PageRequest { limit: 2, cursor: None })
        .await
        .expect("p1");
    assert_eq!(p1.items.len(), 2);
    let c1 = p1.next_cursor.clone().expect("p1 cursor");
    let p2 = postgres::list_reference_items(&pool, t, alta_kernel::PageRequest { limit: 2, cursor: Some(c1) })
        .await
        .expect("p2");
    assert_eq!(p2.items.len(), 2);
    let c2 = p2.next_cursor.clone().expect("p2 cursor");
    let p3 = postgres::list_reference_items(&pool, t, alta_kernel::PageRequest { limit: 2, cursor: Some(c2) })
        .await
        .expect("p3");
    assert_eq!(p3.items.len(), 1);
    assert!(p3.next_cursor.is_none(), "last page must have no cursor");
    let mut ids: Vec<uuid::Uuid> = Vec::new();
    ids.extend(p1.items.iter().map(|i| i.id));
    ids.extend(p2.items.iter().map(|i| i.id));
    ids.extend(p3.items.iter().map(|i| i.id));
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 5, "cursor pagination must return all 5 exactly once");
    assert!(alta_kernel::pagination::decode_cursor(&p1.next_cursor.unwrap()).is_some());
}

#[tokio::test]
async fn idempotency_body_mismatch_conflicts() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    let t = TenantId::new();
    let key = format!("mismatch-{}", uuid::Uuid::new_v4());
    let (first, c1) = postgres::create_reference_item_atomic(&pool, t, title("alpha"), Some(key.clone()))
        .await
        .expect("first");
    assert!(c1);
    let err = postgres::create_reference_item_atomic(&pool, t, title("beta"), Some(key.clone()))
        .await
        .expect_err("different payload same key must conflict");
    assert_eq!(err.code(), alta_kernel::ErrorCode::IdempotencyConflict);
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM reference_items WHERE tenant_id = $1")
        .bind(t.0)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count.0, 1);
    let ocount: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM platform_outbox WHERE tenant_id = $1")
        .bind(t.0)
        .fetch_one(&pool)
        .await
        .expect("ocount");
    assert_eq!(ocount.0, 1);
    let (replay, c2) = postgres::create_reference_item_atomic(&pool, t, title("alpha"), Some(key))
        .await
        .expect("same payload replays");
    assert!(!c2);
    assert_eq!(first.id, replay.id);
}

#[tokio::test]
async fn job_lease_single_winner() {
    let Some(pool) = pool().await else { eprintln!("SKIP no DATABASE_URL"); return };
    sqlx::query("DELETE FROM platform_job_executions WHERE status IN ('pending','leased')")
        .execute(&pool)
        .await
        .expect("clean jobs");
    let job_type = format!("test-job-{}", uuid::Uuid::new_v4());
    let row: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO platform_job_executions (job_type, payload) VALUES ($1, '{}'::jsonb) RETURNING job_id",
    )
    .bind(&job_type)
    .fetch_one(&pool)
    .await
    .expect("insert job");
    let jid = row.0;
    let mut handles = Vec::new();
    for _ in 0..5 {
        let pc = pool.clone();
        handles.push(tokio::spawn(async move {
            alta_infrastructure::workers::claim_job(&pc, 60).await
        }));
    }
    let mut wins = 0;
    let mut won_id = None;
    for h in handles {
        if let Some(rec) = h.await.expect("task").expect("claim") {
            wins += 1;
            won_id = Some(rec.job_id);
        }
    }
    assert_eq!(wins, 1, "exactly one worker must win the lease");
    assert_eq!(won_id.unwrap(), jid);
    alta_infrastructure::workers::complete_job(&pool, jid).await.expect("complete");
    let status: (String,) = sqlx::query_as("SELECT status FROM platform_job_executions WHERE job_id = $1")
        .bind(jid)
        .fetch_one(&pool)
        .await
        .expect("status");
    assert_eq!(status.0, "done");
}
