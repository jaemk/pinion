use crate::models::{BaseUser, Phone, User, VerificationCode};
use crate::{AppError, Result, CONFIG};
use async_graphql::{
    Context, EmptySubscription, ErrorExtensions, FieldResult, Guard, Object, ResultExt,
};
use chrono::Utc;
use serde::Serialize;
use sqlx::PgPool;

struct LoginGuard;

impl LoginGuard {
    fn new() -> Self {
        Self {}
    }
}

/// Used to wrap entrypoints that require a logged and verified in user
#[async_trait::async_trait]
impl Guard for LoginGuard {
    async fn check(&self, ctx: &Context<'_>) -> FieldResult<()> {
        let u = ctx.data_opt::<User>();
        if u.is_none() {
            return Err(AppError::Unauthorized("Unauthorized".into()).extend());
        }
        let u = u.unwrap();
        if u.phone_verified.is_none() {
            return Err(AppError::Unverified("Unverified".into()).extend());
        }
        Ok(())
    }
}

struct LoginNeedsVerificationGuard;

impl LoginNeedsVerificationGuard {
    fn new() -> Self {
        Self {}
    }
}

/// Used to wrap entrypoints that require a user, but the user might not
/// yet be verified
#[async_trait::async_trait]
impl Guard for LoginNeedsVerificationGuard {
    async fn check(&self, ctx: &Context<'_>) -> FieldResult<()> {
        let u = ctx.data_opt::<User>();
        if u.is_none() {
            return Err(AppError::Unauthorized("Unauthorized".into()).extend());
        }
        // it's ok if the phone isn't verified
        Ok(())
    }
}

fn format_set_cookie(token: &str) -> String {
    format!(
        "{name}={token}; Domain={domain}; {secure} HttpOnly; Max-Age={max_age}; SameSite=Lax; Path=/",
        name = &CONFIG.cookie_name,
        token = token,
        domain = &CONFIG.get_real_domain(),
        secure = if CONFIG.secure_cookie { "Secure;" } else { "" },
        max_age = &CONFIG.auth_expiration_seconds,
    )
}

async fn login_ctx(ctx: &Context<'_>, user: &User) -> Result<()> {
    let pool = ctx.data_unchecked::<PgPool>();
    let token = hex::encode(crate::crypto::rand_bytes(32)?);
    let token_hash = crate::crypto::hmac_sign(&token);
    let expires = Utc::now()
        .checked_add_signed(chrono::Duration::seconds(
            CONFIG.auth_expiration_seconds as i64,
        ))
        .ok_or_else(|| AppError::from("error calculating auth expiration"))?;
    sqlx::query(
        r##"
        insert into pin.auth_tokens
            (user_id, hash, expires) values ($1, $2, $3)
    "##,
    )
    .bind(user.id)
    .bind(token_hash)
    .bind(expires)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("error {:?}", e);
        AppError::from(e)
    })?;
    let cookie_str = format_set_cookie(&token);
    ctx.insert_http_header("set-cookie", cookie_str);
    Ok(())
}

async fn send_verification_code(ctx: &Context<'_>, user: &User) -> Result<String> {
    let pool = ctx.data_unchecked::<PgPool>();

    #[derive(Clone, sqlx::FromRow)]
    struct Count {
        last_minute_count: i64,
    }
    let c: Count = sqlx::query_as(
        r##"
        select count(*) as last_minute_count
        from pin.verification_codes
        where user_id = $1
            and created > now() - interval '60 seconds'
        "##,
    )
    .bind(user.id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("error {:?}", e);
        AppError::from(e)
    })?;
    if let Some(sent) = user.phone_verification_sent {
        if sent
            > Utc::now()
                .checked_sub_signed(chrono::Duration::seconds(5))
                .expect("error calculating 5 seconds ago")
            || c.last_minute_count > 5
        {
            return Err(AppError::BadRequest(
                "too many authorization attempts".into(),
            ));
        }
    }
    let code = crate::crypto::rand_bytes(6)
        .expect("error generating code bytes")
        .iter()
        .map(|n| (n % 10).to_string())
        .collect::<String>();
    let salt = crate::crypto::new_pw_salt().expect("error generating salt");
    let hash = crate::crypto::derive_password_hash(code.as_bytes(), salt.as_ref());
    let salt = hex::encode(salt);
    let hash = hex::encode(hash);
    let pool = ctx.data_unchecked::<PgPool>();

    sqlx::query(
        r##"
            insert into pin.verification_codes (user_id, salt, hash)
                values ($1, $2, $3)
        "##,
    )
    .bind(user.id)
    .bind(salt)
    .bind(hash)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("error {:?}", e);
        AppError::from(e)
    })?;

    sqlx::query(
        r##"
            update pin.phones
                set modified = now(),
                verification_sent = now(),
                verification_attempts = verification_attempts + 1
            where user_id = $1
        "##,
    )
    .bind(user.id)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("error {:?}", e);
        AppError::from(e)
    })?;

    #[derive(Serialize)]
    struct Msg {
        #[serde(rename = "To")]
        to: String,
        #[serde(rename = "MessagingServiceSid")]
        msg_sid: String,
        #[serde(rename = "Body")]
        body: String,
    }
    let msg = Msg {
        to: user.phone_number.clone(),
        msg_sid: CONFIG.twilio_messaging_service_sid.clone(),
        body: format!("Your Pinion code is {}", code),
    };
    let url = format!(
        "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
        CONFIG.twilio_account
    );
    if CONFIG.allowed_phone_numbers.is_none()
        || (CONFIG.allowed_phone_numbers.is_some()
            && CONFIG
                .allowed_phone_numbers
                .as_ref()
                .unwrap()
                .contains(&user.phone_number))
    {
        tracing::info!("sending code to {}", &user.phone_number);
        let _resp: serde_json::Value = reqwest::Client::new()
            .post(&url)
            .basic_auth(&CONFIG.twilio_sid, Some(&CONFIG.twilio_secret))
            .form(&msg)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("{:?}", e);
                e
            })?
            .json()
            .await
            .map_err(|e| {
                tracing::error!("{:?}", e);
                e
            })?;
    }
    tracing::debug!("verification code: {}", code);
    Ok(code)
}

async fn _verify_code_for_user(
    tr: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: &User,
    code: &str,
) -> Result<User> {
    let latest_code: Option<VerificationCode> = sqlx::query_as(
        r##"
        select * from pin.verification_codes
        where user_id = $1
        order by created desc
        limit 1
        "##,
    )
    .bind(user.id)
    .fetch_optional(&mut *tr)
    .await
    .map_err(AppError::from)?;
    if latest_code.is_none() {
        return Err(AppError::BadRequest("invalid code".into()));
    }
    let latest_code = latest_code.unwrap();
    if latest_code.created
        < Utc::now()
            .checked_sub_signed(chrono::Duration::seconds(120))
            .expect("error calculating 2 minutes ago")
    {
        return Err(AppError::BadRequest("invalid code".into()));
    }
    let saved_hash = hex::decode(&latest_code.hash)?;
    let this_hash = crate::crypto::derive_password_hash(
        code.as_bytes(),
        hex::decode(&latest_code.salt)?.as_ref(),
    );
    if ring::constant_time::verify_slices_are_equal(&saved_hash, &this_hash).is_err() {
        return Err(AppError::BadRequest("invalid code".into()));
    }

    sqlx::query(
        r##"update pin.verification_codes set deleted = true, modified = now() where id = $1"##,
    )
    .bind(latest_code.id)
    .execute(&mut *tr)
    .await
    .map_err(AppError::from)?;

    // Note: This will fail if someone has already verified this number. This is because we
    //       only enforce unique _verified_ numbers so that someone can't squat your number
    //       without being able to verify it. The potential downside is that if you legitimately
    //       enter the wrong (or someone elses) number at signup, then you won't realize until now.
    //       Need to add another mutation to let you change your phone number (delete and recreate)
    sqlx::query(r##"update pin.phones set verified = now(), modified = now() where user_id = $1"##)
        .bind(latest_code.user_id)
        .execute(&mut *tr)
        .await
        .map_err(AppError::from)?;

    let user = User::fetch_user(tr, user.id).await?;

    Ok(user)
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn sign_up(
        &self,
        ctx: &Context<'_>,
        handle: String,
        phone_number: String,
        name: Option<String>,
    ) -> FieldResult<User> {
        let pool = ctx.data_unchecked::<PgPool>();
        let mut tr = pool
            .begin()
            .await
            .map_err(AppError::from)
            .extend_err(|_e, ex| ex.set("key", "DATABASE_ERROR"))?;
        let user: Option<BaseUser> = sqlx::query_as(
            r##"
            insert into pin.users (handle)
                values ($1)
            on conflict (handle)
                where deleted is false
                do nothing
            returning *
            "##,
        )
        .bind(handle)
        .fetch_optional(&mut *tr)
        .await
        .map_err(AppError::from)
        .extend_err(|e, ex| {
            tracing::error!("error {:?}", e);
            ex.set("key", "DATABASE_ERROR");
        })?;

        if user.is_none() {
            return Err(AppError::BadRequest("bad request".into())
                .extend()
                .extend_with(|_e, ex| ex.set("key", "UNAVAILABLE_HANDLE")));
        }
        let user = user.unwrap();

        if let Some(name) = name {
            sqlx::query(
                r##"
                insert into pin.profiles (user_id, name)
                    values ($1, $2)
                "##,
            )
            .bind(user.id)
            .bind(name)
            .execute(&mut *tr)
            .await
            .map_err(AppError::from)
            .extend_err(|e, ex| {
                tracing::error!("error {:?}", e);
                ex.set("key", "DATABASE_ERROR");
            })?;
        }

        // try to clean it up, also truncate the size in case
        // people are being assholes
        let phone_number = phone_number.trim().chars().take(20).collect::<String>();

        let existing_phone: Option<Phone> = sqlx::query_as(
            r##"
            select * from pin.phones
            where number = $1
                and deleted is false
                and verified is not null
        "##,
        )
        .bind(&phone_number)
        .fetch_optional(&mut *tr)
        .await
        .map_err(AppError::from)
        .extend_err(|e, ex| {
            tracing::error!("error {:?}", e);
            ex.set("key", "DATABASE_ERROR")
        })?;
        if existing_phone.is_some() {
            return Err(AppError::BadRequest("bad request".into())
                .extend()
                .extend_with(|_e, ex| ex.set("key", "UNAVAILABLE_PHONE")));
        }

        let phone: Option<Phone> = sqlx::query_as(
            r##"
            insert into pin.phones (user_id, number)
                values ($1, $2)
            on conflict (number)
                where deleted is false and verified is not null
                do nothing
            returning *
        "##,
        )
        .bind(user.id)
        .bind(phone_number)
        .fetch_optional(&mut *tr)
        .await
        .map_err(AppError::from)
        .extend_err(|e, ex| {
            tracing::error!("error {:?}", e);
            ex.set("key", "DATABASE_ERROR")
        })?;

        if phone.is_none() {
            return Err(AppError::BadRequest("bad request".into())
                .extend()
                .extend_with(|_e, ex| ex.set("key", "UNAVAILABLE_PHONE")));
        }
        let user = User::fetch_user(&mut tr, user.id)
            .await
            .extend_err(|e, ex| {
                tracing::error!("error {:?}", e);
                ex.set("key", "DATABASE_ERROR")
            })?;

        tr.commit().await.map_err(AppError::from).extend()?;
        send_verification_code(ctx, &user).await.extend()?;
        login_ctx(ctx, &user).await.extend()?;
        Ok(user)
    }

    async fn login_phone_confirm(
        &self,
        ctx: &Context<'_>,
        phone_number: String,
        code: String,
    ) -> FieldResult<User> {
        let pool = ctx.data_unchecked::<PgPool>();
        let mut tr = pool
            .begin()
            .await
            .map_err(AppError::from)
            .extend_err(|_e, ex| ex.set("key", "DATABASE_ERROR"))?;

        let user = User::fetch_user_by_number(&mut tr, &phone_number)
            .await
            .extend_err(|e, ex| {
                tracing::error!("error {:?}", e);
                ex.set("key", "DATABASE_ERROR");
            })?;
        if user.is_none() {
            return Err(AppError::BadRequest("bad request".into())
                .extend()
                .extend_with(|_e, ex| ex.set("key", "INVALID_CODE")));
        }
        let user = user.unwrap();
        let user = _verify_code_for_user(&mut tr, &user, &code)
            .await
            .extend_err(|e, ex| {
                tracing::error!("error {:?}", e);
                ex.set("key", "INVALID_CODE")
            })?;

        tr.commit().await.map_err(AppError::from).extend()?;
        login_ctx(ctx, &user).await.extend()?;
        Ok(user)
    }

    async fn login_phone(&self, ctx: &Context<'_>, phone_number: String) -> FieldResult<bool> {
        let pool = ctx.data_unchecked::<PgPool>();
        let mut tr = pool.begin().await?;
        let user = User::fetch_user_by_number(&mut tr, &phone_number)
            .await
            .extend_err(|e, ex| {
                tracing::error!("error {:?}", e);
                ex.set("key", "DATABASE_ERROR")
            })?;
        tr.commit().await.map_err(AppError::from).extend()?;
        if user.is_some() {
            send_verification_code(ctx, &user.unwrap()).await.extend()?;
        }
        Ok(true)
    }

    async fn logout(&self, ctx: &Context<'_>) -> bool {
        let token = hex::encode(crate::crypto::rand_bytes(31).unwrap_or_else(|_| vec![0; 31]));
        let token = format!("xx{token}");
        let cookie_str = format_set_cookie(&token);
        ctx.insert_http_header("set-cookie", cookie_str);
        true
    }

    #[graphql(guard = "LoginNeedsVerificationGuard::new()")]
    async fn send_verification_code(&self, ctx: &Context<'_>) -> FieldResult<User> {
        let user = ctx.data_unchecked::<User>();
        send_verification_code(ctx, user).await.extend()?;
        Ok(user.clone())
    }

    #[graphql(guard = "LoginNeedsVerificationGuard::new()")]
    async fn verify_number(&self, ctx: &Context<'_>, code: String) -> FieldResult<User> {
        let user = ctx.data_unchecked::<User>();
        let pool = ctx.data_unchecked::<PgPool>();
        let mut tr = pool.begin().await.map_err(AppError::from).extend()?;
        let user = _verify_code_for_user(&mut tr, user, &code)
            .await
            .extend_err(|e, ex| {
                tracing::error!("error {:?}", e);
                ex.set("key", "INVALID_CODE")
            })?;
        tr.commit().await.map_err(AppError::from).extend()?;
        Ok(user)
    }

    #[graphql(guard = "LoginNeedsVerificationGuard::new()")]
    async fn delete_account(&self, ctx: &Context<'_>, code: String) -> FieldResult<bool> {
        let user = ctx.data_unchecked::<User>();
        let pool = ctx.data_unchecked::<PgPool>();
        let mut tr = pool.begin().await.map_err(AppError::from).extend()?;
        let user = _verify_code_for_user(&mut tr, user, &code)
            .await
            .extend_err(|e, ex| {
                tracing::error!("error {:?}", e);
                ex.set("key", "INVALID_CODE")
            })?;
        tr.commit().await.map_err(AppError::from).extend()?;

        let mut tr = pool.begin().await.map_err(AppError::from).extend()?;
        sqlx::query(
            r##"
            update pin.phones
                set modified = now(),
                deleted = true
            where user_id = $1
        "##,
        )
        .bind(user.id)
        .execute(&mut *tr)
        .await
        .map_err(AppError::from)
        .extend_err(|e, ex| {
            tracing::error!("error {:?}", e);
            ex.set("key", "DATABASE_ERROR")
        })?;
        sqlx::query(
            r##"
            update pin.users
                set modified = now(),
                deleted = true
            where id = $1
        "##,
        )
        .bind(user.id)
        .execute(&mut *tr)
        .await
        .map_err(AppError::from)
        .extend_err(|e, ex| {
            tracing::error!("error {:?}", e);
            ex.set("key", "DATABASE_ERROR")
        })?;
        tr.commit().await.map_err(AppError::from).extend()?;
        let r = self.logout(ctx).await?;
        Ok(r)
    }
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    #[graphql(guard = "LoginGuard::new()")]
    async fn user(&self, ctx: &Context<'_>) -> Option<User> {
        let u = ctx.data_opt::<User>();
        u.cloned()
    }
}

pub type Schema = async_graphql::Schema<QueryRoot, MutationRoot, EmptySubscription>;
