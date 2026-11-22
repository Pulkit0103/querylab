//! Phase 10a: password change and reset, sessions, two-factor
//! authentication, the audit log and metrics.

mod common;

use axum::Router;
use common::{api, register, send, send_full, session_token};
use querylab_api::utils::totp;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const PASSWORD: &str = "correct horse battery";

async fn call(
    app: &Router,
    token: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let (status, json) = send(app, api(method, path, body, Some(token))).await;
    (status.as_u16(), json)
}

async fn login(app: &Router, email: &str, password: &str) -> (u16, Value, Option<String>) {
    let (status, headers, body) = send_full(
        app,
        api(
            "POST",
            "/api/auth/login",
            Some(json!({ "email": email, "password": password })),
            None,
        ),
    )
    .await;
    (status.as_u16(), body, session_token(&headers))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// The code for an absolute time step.
fn code(secret_b32: &str, step: u64) -> String {
    let secret = totp::base32_decode(secret_b32).unwrap();
    format!("{:06}", totp::code_at(&secret, step))
}

async fn actions(app: &Router, token: &str) -> Vec<String> {
    let (status, page) = call(app, token, "GET", "/api/audit-events?limit=200", None).await;
    assert_eq!(status, 200, "{page}");
    page["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap().to_string())
        .collect()
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn password_change_and_sessions(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let first = register(&app, "pw@example.com").await;
    let (status, _, second) = login(&app, "pw@example.com", PASSWORD).await;
    assert_eq!(status, 200);
    let second = second.unwrap();

    let (status, sessions) = call(&app, &first, "GET", "/api/auth/sessions", None).await;
    assert_eq!(status, 200);
    let list = sessions.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list.iter().filter(|s| s["current"] == true).count(), 1);

    // Wrong current password.
    let (status, err) = call(
        &app,
        &first,
        "POST",
        "/api/auth/password",
        Some(json!({
            "current_password": "not the password", "new_password": "a brand new password",
        })),
    )
    .await;
    assert_eq!(status, 422, "{err}");
    // New password too short or unchanged.
    let (status, _) = call(
        &app,
        &first,
        "POST",
        "/api/auth/password",
        Some(json!({
            "current_password": PASSWORD, "new_password": "short",
        })),
    )
    .await;
    assert_eq!(status, 422);

    let (status, body) = call(
        &app,
        &first,
        "POST",
        "/api/auth/password",
        Some(json!({
            "current_password": PASSWORD, "new_password": "a brand new password",
        })),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["other_sessions_revoked"], 1);
    // The other session is gone; this one stays.
    assert_eq!(
        call(&app, &second, "GET", "/api/auth/me", None).await.0,
        401
    );
    assert_eq!(call(&app, &first, "GET", "/api/auth/me", None).await.0, 200);
    assert_eq!(login(&app, "pw@example.com", PASSWORD).await.0, 401);
    let (status, _, third) = login(&app, "pw@example.com", "a brand new password").await;
    assert_eq!(status, 200);

    // Revoke one session by id; other users' sessions are not visible.
    let (_, sessions) = call(&app, &first, "GET", "/api/auth/sessions", None).await;
    let other = sessions
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["current"] == false)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mallory = register(&app, "mallory@example.com").await;
    assert_eq!(
        call(
            &app,
            &mallory,
            "DELETE",
            &format!("/api/auth/sessions/{other}"),
            None
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            &app,
            &first,
            "DELETE",
            &format!("/api/auth/sessions/{other}"),
            None
        )
        .await
        .0,
        204
    );
    assert_eq!(
        call(&app, &third.unwrap(), "GET", "/api/auth/me", None)
            .await
            .0,
        401
    );

    let events = actions(&app, &first).await;
    for expected in [
        "auth.registered",
        "auth.login",
        "auth.password_change_failed",
        "auth.password_changed",
        "auth.session_revoked",
    ] {
        assert!(
            events.iter().any(|a| a == expected),
            "{expected} missing from {events:?}"
        );
    }
    // Mallory sees only her own events.
    assert!(actions(&app, &mallory)
        .await
        .iter()
        .all(|a| a == "auth.registered"));
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn two_factor_authentication(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "otp@example.com").await;

    // Confirming before enrolment fails.
    let (status, _) = call(
        &app,
        &token,
        "POST",
        "/api/auth/2fa/verify",
        Some(json!({ "code": "123456" })),
    )
    .await;
    assert_eq!(status, 409);

    let (status, enrollment) = call(&app, &token, "POST", "/api/auth/2fa/enroll", None).await;
    assert_eq!(status, 200, "{enrollment}");
    let secret = enrollment["secret"].as_str().unwrap().to_string();
    assert!(enrollment["otpauth_uri"]
        .as_str()
        .unwrap()
        .starts_with("otpauth://totp/QueryLab:otp%40example.com?secret="));
    assert!(enrollment["qr_svg"].as_str().unwrap().contains("<svg"));

    let (status, _) = call(
        &app,
        &token,
        "POST",
        "/api/auth/2fa/verify",
        Some(json!({ "code": "000000" })),
    )
    .await;
    assert_eq!(status, 422);
    // Codes are computed from one captured step, so crossing a 30 s boundary
    // mid-test stays inside the accepted window.
    let step = totp::step_at(now());
    let (status, codes) = call(
        &app,
        &token,
        "POST",
        "/api/auth/2fa/verify",
        Some(json!({ "code": code(&secret, step) })),
    )
    .await;
    assert_eq!(status, 200, "{codes}");
    let recovery: Vec<String> = codes["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect();
    assert_eq!(recovery.len(), 10);
    let (_, me) = call(&app, &token, "GET", "/api/auth/me", None).await;
    assert_eq!(me["two_factor_enabled"], true);
    // Enrolling again is refused while enabled.
    assert_eq!(
        call(&app, &token, "POST", "/api/auth/2fa/enroll", None)
            .await
            .0,
        409
    );

    // A correct password now yields a challenge, not a session.
    let (status, body, session) = login(&app, "otp@example.com", PASSWORD).await;
    assert_eq!(status, 200);
    assert_eq!(body["two_factor_required"], true);
    assert!(session.is_none());
    let challenge = body["challenge"].as_str().unwrap().to_string();
    let second = |code: String| {
        api(
            "POST",
            "/api/auth/login/2fa",
            Some(json!({ "challenge": challenge, "code": code })),
            None,
        )
    };
    let (status, _) = send(&app, second("999999".into())).await;
    assert_eq!(status, 422);
    // The step used to enable 2FA cannot be replayed; the next one works.
    let (status, _, _) = send_full(&app, second(code(&secret, step))).await;
    assert_eq!(status, 422);
    let (status, headers, user) = send_full(&app, second(code(&secret, step + 1))).await;
    assert_eq!(status, 200, "{user}");
    assert!(session_token(&headers).is_some());
    // A finished challenge cannot be reused.
    assert_eq!(send(&app, second(code(&secret, step + 1))).await.0, 401);

    // Recovery codes work once.
    let (_, body, _) = login(&app, "otp@example.com", PASSWORD).await;
    let challenge = body["challenge"].as_str().unwrap().to_string();
    let with = |c: &str| {
        api(
            "POST",
            "/api/auth/login/2fa",
            Some(json!({ "challenge": challenge, "code": c })),
            None,
        )
    };
    assert_eq!(send(&app, with(&recovery[0].to_uppercase())).await.0, 200);
    let (_, body, _) = login(&app, "otp@example.com", PASSWORD).await;
    let challenge = body["challenge"].as_str().unwrap().to_string();
    let again = api(
        "POST",
        "/api/auth/login/2fa",
        Some(json!({ "challenge": challenge, "code": recovery[0] })),
        None,
    );
    assert_eq!(send(&app, again).await.0, 422);
    let (_, status_body) = call(&app, &token, "GET", "/api/auth/security", None).await;
    assert_eq!(status_body["recovery_codes_left"], 9);

    // Attempts per challenge are limited.
    for _ in 0..5 {
        let bad = api(
            "POST",
            "/api/auth/login/2fa",
            Some(json!({ "challenge": challenge, "code": "111111" })),
            None,
        );
        send(&app, bad).await;
    }
    let last = api(
        "POST",
        "/api/auth/login/2fa",
        Some(json!({ "challenge": challenge, "code": recovery[1] })),
        None,
    );
    assert_eq!(send(&app, last).await.0, 401);

    // Disabling needs the password and a code.
    let (status, _) = call(
        &app,
        &token,
        "POST",
        "/api/auth/2fa/disable",
        Some(json!({ "password": "wrong password!", "code": recovery[2] })),
    )
    .await;
    assert_eq!(status, 422);
    let (status, _) = call(
        &app,
        &token,
        "POST",
        "/api/auth/2fa/disable",
        Some(json!({ "password": PASSWORD, "code": recovery[2] })),
    )
    .await;
    assert_eq!(status, 204);
    let (status, body, session) = login(&app, "otp@example.com", PASSWORD).await;
    assert_eq!(status, 200);
    assert!(session.is_some(), "{body}");

    let events = actions(&app, &token).await;
    for expected in [
        "auth.2fa_enabled",
        "auth.recovery_code_used",
        "auth.2fa_disabled",
        "auth.login_failed",
    ] {
        assert!(
            events.iter().any(|a| a == expected),
            "{expected} missing from {events:?}"
        );
    }
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn password_reset(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool.clone());
    let token = register(&app, "reset@example.com").await;
    let (_, options) = send(&app, api("GET", "/api/auth/options", None, None)).await;
    assert_eq!(options["password_reset"], true);

    // Same answer whether or not the account exists.
    for email in ["reset@example.com", "nobody@example.com"] {
        let (status, _) = send(
            &app,
            api(
                "POST",
                "/api/auth/password-reset",
                Some(json!({ "email": email })),
                None,
            ),
        )
        .await;
        assert_eq!(status, 202);
    }
    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM password_resets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(links, 1);

    // The emailed token is random; plant a known one for the test.
    let reset_token = "test-reset-token-0123456789";
    let hash: Vec<u8> = Sha256::digest(reset_token.as_bytes()).to_vec();
    sqlx::query(
        "INSERT INTO password_resets (id, user_id, token_hash, expires_at) \
         SELECT gen_random_uuid(), id, $1, now() + interval '1 hour' FROM users WHERE email = 'reset@example.com'",
    )
    .bind(&hash)
    .execute(&pool)
    .await
    .unwrap();

    let check = |t: &str| {
        api(
            "GET",
            &format!("/api/auth/password-reset?token={t}"),
            None,
            None,
        )
    };
    assert_eq!(send(&app, check(reset_token)).await.1["valid"], true);
    assert_eq!(send(&app, check("nope")).await.1["valid"], false);

    let confirm = |t: &str| {
        api(
            "POST",
            "/api/auth/password-reset/confirm",
            Some(json!({ "token": t, "new_password": "reset password 123" })),
            None,
        )
    };
    assert_eq!(send(&app, confirm(reset_token)).await.0, 204);
    // Single use; every session was signed out.
    assert_eq!(send(&app, confirm(reset_token)).await.0, 422);
    assert_eq!(call(&app, &token, "GET", "/api/auth/me", None).await.0, 401);
    assert_eq!(
        login(&app, "reset@example.com", "reset password 123")
            .await
            .0,
        200
    );
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn audit_log_is_append_only(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool.clone());
    let token = register(&app, "audit@example.com").await;
    call(&app, &token, "POST", "/api/auth/logout", None).await;
    let (_, _, token) = login(&app, "audit@example.com", PASSWORD).await;
    let token = token.unwrap();

    let (status, page) = call(&app, &token, "GET", "/api/audit-events?action=auth.", None).await;
    assert_eq!(status, 200);
    let events = page["events"].as_array().unwrap();
    assert!(events.len() >= 3, "{page}");
    assert!(events
        .iter()
        .all(|e| e["action"].as_str().unwrap().starts_with("auth.")));
    assert!(events[0]["request_id"].is_string(), "{}", events[0]);
    assert_eq!(events[0]["actor_email"], "audit@example.com");

    let (status, headers, _) = send_full(
        &app,
        api("GET", "/api/audit-events/export", None, Some(&token)),
    )
    .await;
    assert_eq!(status.as_u16(), 200);
    assert!(headers["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/csv"));

    // The application role cannot change or remove events.
    for sql in [
        "UPDATE audit_events SET action = 'tampered'",
        "DELETE FROM audit_events",
        "TRUNCATE audit_events",
    ] {
        let err = sqlx::query(sql).execute(&pool).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("append-only") || msg.contains("permission denied"),
            "{sql}: {msg}"
        );
    }
    // The purge only removes events past the retention period.
    let removed = querylab_api::database::audit::purge(&pool, 30)
        .await
        .unwrap();
    assert_eq!(removed, 0);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(left >= 3);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn metrics_require_a_token(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool.clone());
    assert_eq!(
        send(&app, api("GET", "/metrics", None, None))
            .await
            .0
            .as_u16(),
        404
    );

    let (app, _) = common::app_with_config(pool, |c| {
        c.metrics_token = Some("metrics-token-0123456789".into())
    });
    send(&app, api("GET", "/api/version", None, None)).await;
    assert_eq!(
        send(&app, api("GET", "/metrics", None, None))
            .await
            .0
            .as_u16(),
        401
    );
    let res = {
        use tower::ServiceExt;
        app.clone()
            .oneshot(
                axum::http::Request::get("/metrics")
                    .header("authorization", "Bearer metrics-token-0123456789")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    };
    assert_eq!(res.status().as_u16(), 200);
    let body = http_body_util::BodyExt::collect(res.into_body())
        .await
        .unwrap()
        .to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("querylab_http_requests_total{method=\"GET\",status=\"2xx\"}"),
        "{text}"
    );
    assert!(text.contains("querylab_executions_running 0"));
    assert!(text.contains("querylab_db_pool_connections"));
}
