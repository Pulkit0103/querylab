//! Service-level tests against a real metadata database.
//! Run with: `cargo test -- --include-ignored` (needs DATABASE_URL with CREATEDB).

mod common;

use querylab_api::{
    database::users,
    errors::AppError,
    models::connection::{CreateConnectionRequest, SslMode, UpdateConnectionRequest},
    services::connections::ConnectionService,
};
use sqlx::PgPool;
use uuid::Uuid;

async fn create_user(pool: &PgPool, email: &str) -> Uuid {
    let id = Uuid::new_v4();
    users::insert(pool, id, email, "not-a-real-hash", None)
        .await
        .unwrap();
    id
}

fn create_req(name: &str, password: &str) -> CreateConnectionRequest {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "host": "db.example.com",
        "database": "app",
        "username": "reader",
        "password": password,
        "ssl_mode": "require",
    }))
    .unwrap()
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn credentials_are_encrypted_at_rest(pool: PgPool) {
    let state = common::app_with_pool(pool.clone()).1;
    let svc = ConnectionService::new(&state);
    let user = create_user(&pool, "a@example.com").await;

    let created = svc
        .create(user, &create_req("Prod", "super-secret-password"))
        .await
        .unwrap();
    assert_eq!(created.ssl_mode, SslMode::Require);
    assert!(created.read_only);

    // The raw column must not contain the password, and the API model has no
    // field that could carry it.
    let raw: Vec<u8> =
        sqlx::query_scalar("SELECT encrypted_credentials FROM database_connections WHERE id = $1")
            .bind(created.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!raw
        .windows(b"super-secret".len())
        .any(|w| w == b"super-secret"));
    let json = serde_json::to_string(&created).unwrap();
    assert!(!json.contains("super-secret"));

    let record = svc.get_record(user, created.id).await.unwrap();
    assert_eq!(svc.decrypt_secret(&record).unwrap().password, "super-secret-password");
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn ciphertext_is_bound_to_its_row(pool: PgPool) {
    let state = common::app_with_pool(pool.clone()).1;
    let svc = ConnectionService::new(&state);
    let user = create_user(&pool, "a@example.com").await;
    let a = svc.create(user, &create_req("A", "pw-a")).await.unwrap();
    let b = svc.create(user, &create_req("B", "pw-b")).await.unwrap();

    // Copy A's ciphertext onto B: decryption must fail authentication.
    sqlx::query(
        "UPDATE database_connections SET encrypted_credentials = \
         (SELECT encrypted_credentials FROM database_connections WHERE id = $1) WHERE id = $2",
    )
    .bind(a.id)
    .bind(b.id)
    .execute(&pool)
    .await
    .unwrap();
    let record = svc.get_record(user, b.id).await.unwrap();
    assert!(matches!(svc.decrypt_secret(&record), Err(AppError::Internal(_))));
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn connections_are_isolated_per_user(pool: PgPool) {
    let state = common::app_with_pool(pool.clone()).1;
    let svc = ConnectionService::new(&state);
    let alice = create_user(&pool, "alice@example.com").await;
    let bob = create_user(&pool, "bob@example.com").await;

    let conn = svc.create(alice, &create_req("Prod", "pw")).await.unwrap();

    assert_eq!(svc.list(alice).await.unwrap().len(), 1);
    assert!(svc.list(bob).await.unwrap().is_empty());
    assert!(matches!(
        svc.get(bob, conn.id).await,
        Err(AppError::NotFound("connection"))
    ));
    assert!(matches!(
        svc.delete(bob, conn.id).await,
        Err(AppError::NotFound("connection"))
    ));
    assert!(matches!(
        svc.update(bob, conn.id, &UpdateConnectionRequest::default()).await,
        Err(AppError::NotFound("connection"))
    ));
    // Alice still has it.
    assert!(svc.get(alice, conn.id).await.is_ok());

    // Same name is allowed for different users, but not twice for one user.
    assert!(svc.create(bob, &create_req("Prod", "pw")).await.is_ok());
    assert!(matches!(
        svc.create(alice, &create_req("Prod", "pw")).await,
        Err(AppError::Conflict(_))
    ));
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn update_keeps_password_unless_provided(pool: PgPool) {
    let state = common::app_with_pool(pool.clone()).1;
    let svc = ConnectionService::new(&state);
    let user = create_user(&pool, "a@example.com").await;
    let conn = svc.create(user, &create_req("Prod", "first")).await.unwrap();

    let renamed: UpdateConnectionRequest =
        serde_json::from_value(serde_json::json!({ "name": "Renamed", "read_only": false })).unwrap();
    let updated = svc.update(user, conn.id, &renamed).await.unwrap();
    assert_eq!(updated.name, "Renamed");
    assert!(!updated.read_only);
    let record = svc.get_record(user, conn.id).await.unwrap();
    assert_eq!(svc.decrypt_secret(&record).unwrap().password, "first");

    let new_pw: UpdateConnectionRequest =
        serde_json::from_value(serde_json::json!({ "password": "second" })).unwrap();
    svc.update(user, conn.id, &new_pw).await.unwrap();
    let record = svc.get_record(user, conn.id).await.unwrap();
    assert_eq!(svc.decrypt_secret(&record).unwrap().password, "second");

    svc.delete(user, conn.id).await.unwrap();
    assert!(svc.list(user).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn saved_queries_cannot_reference_another_users_connection(pool: PgPool) {
    let state = common::app_with_pool(pool.clone()).1;
    let svc = ConnectionService::new(&state);
    let alice = create_user(&pool, "alice@example.com").await;
    let bob = create_user(&pool, "bob@example.com").await;
    let conn = svc.create(alice, &create_req("Prod", "pw")).await.unwrap();

    // The composite foreign key rejects cross-user references at the DB level.
    let err = sqlx::query(
        "INSERT INTO saved_queries (id, user_id, connection_id, name, sql_content) VALUES ($1, $2, $3, 'x', 'select 1')",
    )
    .bind(Uuid::new_v4())
    .bind(bob)
    .bind(conn.id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(matches!(AppError::from(err), AppError::BadRequest(_)));
}
