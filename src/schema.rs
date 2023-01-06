use crate::crypto::{b64_encode, encrypt};
use crate::models::{
    BaseUser, ChallengePhone, LoginSuccess, Phone, Pinion, User, VerificationCode,
};
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

fn generate_clear_token() -> String {
    let clear_token = hex::encode(crate::crypto::rand_bytes(31).unwrap_or_else(|_| vec![0; 31]));
    format!("xxxx{clear_token}")
}

fn format_set_auth_cookie(token: &str) -> String {
    format!(
        "{name}={token}; Domain={domain}; {secure} HttpOnly; Max-Age={max_age}; SameSite=Lax; Path=/",
        name = CONFIG.auth_cookie_name,
        token = token,
        domain = &CONFIG.get_real_domain(),
        secure = if CONFIG.secure_cookie { "Secure;" } else { "" },
        max_age = &CONFIG.auth_expiration_seconds,
    )
}

fn format_set_challenge_phone_cookie(token: &str) -> String {
    format!(
        "{name}={token}; Domain={domain}; {secure} HttpOnly; Max-Age={max_age}; SameSite=Lax; Path=/",
        name = CONFIG.cookie_challenge_phone_name,
        token = token,
        domain = &CONFIG.get_real_domain(),
        secure = if CONFIG.secure_cookie { "Secure;" } else { "" },
        max_age = &CONFIG.challenge_phone_expiration_seconds,
    )
}

async fn challenge_phone_ctx(ctx: &Context<'_>, phone_number: &str) -> Result<()> {
    let phone_enc = encrypt(phone_number)?;
    let phone_json = serde_json::to_string(&phone_enc)?;
    let s = b64_encode(&phone_json);
    let cookie_str = format_set_challenge_phone_cookie(&s);
    ctx.append_http_header("set-cookie", cookie_str);
    Ok(())
}

async fn login_ctx(ctx: &Context<'_>, user: &User) -> Result<String> {
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
    let cookie_str = format_set_auth_cookie(&token);
    ctx.append_http_header("set-cookie", cookie_str);

    let clear_cookie = format_set_challenge_phone_cookie(&generate_clear_token());
    ctx.append_http_header("set-cookie", clear_cookie);
    Ok(token)
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
        where user_id = $1 and deleted is false
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
            .checked_sub_signed(chrono::Duration::seconds(
                CONFIG.challenge_phone_expiration_seconds as i64,
            ))
            .expect("error calculating challenge phone offset")
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

async fn create_user(
    tr: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    handle: String,
    phone_number: &str,
    name: Option<String>,
) -> FieldResult<User> {
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
    let user = User::fetch_user(&mut *tr, user.id)
        .await
        .extend_err(|e, ex| {
            tracing::error!("error {:?}", e);
            ex.set("key", "DATABASE_ERROR")
        })?;

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

        let user = create_user(&mut tr, handle, &phone_number, name).await?;
        tr.commit().await.map_err(AppError::from).extend()?;
        send_verification_code(ctx, &user).await.extend()?;
        login_ctx(ctx, &user).await.extend()?;
        Ok(user)
    }

    #[graphql(guard = "LoginGuard::new()")]
    async fn set_handle(&self, ctx: &Context<'_>, handle: String) -> FieldResult<User> {
        let user = ctx.data_unchecked::<User>();
        let pool = ctx.data_unchecked::<PgPool>();
        let mut tr = pool.begin().await?;

        let user: Option<BaseUser> = sqlx::query_as(
            r##"
            update pin.users
                set handle = $1
                where id = $2
                    and deleted is false
                returning *
            "##,
        )
        .bind(&handle)
        .bind(user.id)
        .fetch_optional(&mut *tr)
        .await
        .map_err(AppError::from)
        .extend_err(|e, ex| {
            if let Some((_code, _constraint)) = e.unique_constraint_error() {
                tracing::info!("handle {} is unavailable", &handle);
                ex.set("key", "UNAVAILABLE_HANDLE")
            } else {
                tracing::error!("error {:?}", e);
                ex.set("key", "DATABASE_ERROR");
            }
        })?;

        let user = match user {
            None => {
                return Err(AppError::BadRequest("bad request".into())
                    .extend()
                    .extend_with(|_e, ex| ex.set("key", "UNKNOWN_USER")))
            }
            Some(user) => User::fetch_user(&mut tr, user.id).await?,
        };
        tr.commit().await.map_err(AppError::from).extend()?;
        Ok(user)
    }

    async fn login_phone_confirm(
        &self,
        ctx: &Context<'_>,
        phone_number: Option<String>,
        code: String,
    ) -> FieldResult<LoginSuccess> {
        let pool = ctx.data_unchecked::<PgPool>();
        let mut tr = pool
            .begin()
            .await
            .map_err(AppError::from)
            .extend_err(|_e, ex| ex.set("key", "DATABASE_ERROR"))?;

        let phone_number = phone_number.or_else(|| {
            tracing::info!("no phone number specified, using challenge-ctx cookie number");
            ctx.data_opt::<ChallengePhone>().map(|p| p.number.clone())
        });
        let phone_number = match phone_number {
            None => {
                tracing::error!("no phone number provided or found while verifying code");
                return Err(AppError::from("no phone number provided or found"))
                    .extend_err(|_e, ex| ex.set("key", "MISSING_PHONE_NUMBER"));
            }
            Some(p) => p,
        };
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
        let token = login_ctx(ctx, &user).await.extend()?;
        Ok(LoginSuccess {
            auth_token: token,
            user,
        })
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
        let user = match user {
            Some(user) => user,
            None => {
                create_user(
                    &mut tr,
                    uuid::Uuid::new_v4().as_hyphenated().to_string(),
                    &phone_number,
                    None,
                )
                .await?
            }
        };
        tr.commit().await.map_err(AppError::from).extend()?;
        send_verification_code(ctx, &user).await.extend()?;
        challenge_phone_ctx(ctx, &phone_number).await.extend()?;
        Ok(true)
    }

    async fn logout(&self, ctx: &Context<'_>) -> bool {
        let cookie_str = format_set_auth_cookie(&generate_clear_token());
        ctx.append_http_header("set-cookie", cookie_str);
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

    #[graphql(guard = "LoginGuard::new()")]
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

    #[graphql(guard = "LoginGuard::new()")]
    async fn opine(
        &self,
        ctx: &Context<'_>,
        question_id: String,
        multi_selection_id: String,
    ) -> FieldResult<Pinion> {
        let user = ctx.data_unchecked::<User>();
        let pool = ctx.data_unchecked::<PgPool>();
        let mut tr = pool.begin().await.map_err(AppError::from).extend()?;
        let pinion: Pinion = sqlx::query_as(
            r##"
            insert into pin.pinions
                (user_id, question_id, multi_selection)
                values ($1, $2, $3)
                returning *
        "##,
        )
        .bind(user.id)
        .bind(question_id.parse::<i64>()?)
        .bind(multi_selection_id.parse::<i64>()?)
        .fetch_one(&mut *tr)
        .await
        .map_err(AppError::from)
        .extend_err(|e, ex| {
            if let Some((_code, _constraint)) = e.unique_constraint_error() {
                tracing::info!("{} submitted multiple pinions in one day", &user.handle);
                ex.set("key", "MULTIPLE_DAILY_RESPONSES");
            } else {
                tracing::error!("error saving pinion {:?}", e);
                ex.set("key", "DATABASE_ERROR");
            }
        })?;
        tr.commit().await.map_err(AppError::from).extend()?;
        Ok(pinion)
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
