use async_graphql::{dataloader::HashMapCache, EmptySubscription};
use async_graphql_warp::GraphQLResponse;
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, PgPool};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;
use warp::{hyper::Method, Filter};

mod config;
mod crypto;
mod error;
mod loaders;
mod models;
mod schema;

use crate::crypto::b64_decode;
use crate::error::LogError;
use crate::loaders::QOD_QUERY;
use crate::models::{ChallengePhone, Question, QuestionOptionCount};
use error::{AppError, Result};
use loaders::PgLoader;
use models::User;
use schema::{MutationRoot, QueryRoot, Schema};

lazy_static::lazy_static! {
    pub static ref CONFIG: config::Config = config::Config::load();
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error running server: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    dotenv::dotenv().ok();

    let addr = CONFIG.get_host_port();
    let filter = tracing_subscriber::filter::EnvFilter::new(&CONFIG.log_level);
    if CONFIG.log_json {
        tracing_subscriber::fmt()
            .json()
            .with_current_span(false)
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    let mut pg_opt: PgConnectOptions = CONFIG.database_url.parse()?;
    pg_opt
        .log_statements(log::LevelFilter::Debug)
        .log_slow_statements(log::LevelFilter::Warn, std::time::Duration::new(5, 0));
    let pool = sqlx::PgPool::connect_with(pg_opt).await?;

    let status = warp::path("status").and(warp::get()).map(move || {
        #[derive(serde::Serialize)]
        struct Status<'a> {
            version: &'a str,
            ok: &'a str,
        }
        serde_json::to_string(&Status {
            version: &CONFIG.version,
            ok: "ok",
        })
        .expect("error serializing status")
    });

    let favicon = warp::path("favicon.ico")
        .and(warp::get())
        .and(warp::fs::file("static/think.jpg"));

    let index = warp::any().and(warp::path::end()).map(|| "hello");

    let schema = async_graphql::Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(pool.clone())
        .finish();

    let move_pool = pool.clone();
    let graphql_post = warp::path!("api" / "graphql")
        .and(warp::path::end())
        .and(warp::post())
        .map(move || move_pool.clone())
        .and(warp::filters::cookie::optional(&CONFIG.auth_cookie_name))
        .and(warp::filters::header::optional(&CONFIG.auth_header_name))
        .and(warp::filters::cookie::optional(
            &CONFIG.cookie_challenge_phone_name,
        ))
        .and(async_graphql_warp::graphql(schema.clone()))
        .and_then(
            |pool: PgPool,
             auth_cookie: Option<String>,
             auth_header: Option<String>,
             challenge_phone_cookie: Option<String>,
             (schema, mut request): (Schema, async_graphql::Request)| async move {
                if let Some(auth) = auth_cookie.or(auth_header) {
                    let hash = crypto::hmac_sign(&auth);
                    let u: Result<User> = sqlx::query_as(
                        r##"
                        select
                            u.*, p.number as phone_number, p.verified as phone_verified,
                            p.verification_sent as phone_verification_sent,
                            p.verification_attempts as phone_verification_attempts,
                            pr.name
                        from pin.users u
                            inner join pin.auth_tokens at on u.id = at.user_id
                            inner join pin.phones p on u.id = p.user_id
                            left outer join pin.profiles pr on u.id = pr.user_id
                        where at.hash = $1
                            and at.deleted is false
                            and at.expires > now()
                            and u.deleted is false
                            and (pr.deleted is false or pr.deleted is null)
                            "##,
                    )
                    .bind(hash)
                    .fetch_one(&pool)
                    .await
                    .map_err(|e| {
                        if matches!(e, sqlx::Error::RowNotFound) {
                            tracing::info!("no user logged in");
                        } else {
                            tracing::error!("error {:?}", e);
                        }
                        AppError::from(e)
                    });
                    if let Ok(u) = u {
                        tracing::info!(user = %u.handle, user_id = %u.id, "found user for request");
                        request.data.insert(u);
                    }
                }
                let loader = async_graphql::dataloader::DataLoader::with_cache(
                    PgLoader::new(pool),
                    tokio::spawn,
                    HashMapCache::default(),
                );
                request.data.insert(loader);

                if let Some(challenge_cookie) = challenge_phone_cookie {
                    if !challenge_cookie.starts_with("xxxx") {
                        b64_decode(&challenge_cookie)
                            .map_err(|e| {
                                tracing::error!(
                                    "error base64 decoding challenge_phone_cookie {:?}",
                                    e
                                );
                                e
                            })
                            .and_then(|s| Ok(serde_json::from_slice(&s)?))
                            .map_err(|e| {
                                tracing::error!(
                                    "error decoding challenge_phone_cookie, expected json {:?}",
                                    e
                                );
                                e
                            })
                            .and_then(|enc| crypto::decrypt(&enc))
                            .map_err(|e| {
                                tracing::error!("error decrypting challenge_phone_cookie {:?}", e);
                                e
                            })
                            .map(|number| {
                                request.data.insert(ChallengePhone { number });
                            })
                            .ok();
                    }
                }

                let resp = schema.execute(request).await;
                Ok::<_, Infallible>(GraphQLResponse::from(resp))
            },
        );

    let graphiql = warp::path!("_" / "graphiql")
        .and(warp::path::end())
        .and(warp::get())
        .map(|| {
            warp::http::Response::builder()
                .header("content-type", "text/html")
                .body(
                    async_graphql::http::GraphiQLSource::build()
                        .endpoint("/api/graphql")
                        .finish(),
                )
        });

    let index_options = warp::path::end().and(warp::options()).map(warp::reply);

    let graphql_options = warp::path!("api" / "graphql")
        .and(warp::path::end())
        .and(warp::options())
        .map(warp::reply);

    let cors = warp::cors()
        .allow_methods(&[Method::GET, Method::POST])
        .allow_headers(["cookie", "content-type"])
        .allow_origins([
            "http://127.0.0.1:3000",
            "http://localhost:3000",
            "http://localhost:3003",
            "https://api.getpinion.com",
            "https://getpinion.com",
        ]);
    let routes = index
        .or(index_options)
        .or(graphql_post)
        .or(graphiql)
        .or(graphql_options)
        .or(favicon)
        .or(status)
        .with(cors)
        .with(warp::trace::request());

    async fn tally_question_of_the_day(pool: PgPool) {
        // todo: instead of just sleep, select from either a timer
        //       or a channel so that other code can trigger a refresh
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let question: Result<Question> = sqlx::query_as(QOD_QUERY)
                .fetch_one(&pool)
                .await
                .map_err(AppError::from)
                .log_error_msg(|| "background: error loading question of the day");
            if question.is_err() {
                continue;
            }
            let question = question.unwrap();

            let tr = pool
                .begin()
                .await
                .map_err(AppError::from)
                .log_error_msg(|| "background: error starting transaction {:?}");
            if tr.is_err() {
                continue;
            }
            let mut tr = tr.unwrap();
            let options = Question::get_options(question.id, &mut tr)
                .await
                .log_error_msg(|| "background: error getting question options");
            if options.is_err() {
                continue;
            }
            let options = options.unwrap();
            let option_counts = QuestionOptionCount::get_option_counts(question.id, &mut tr)
                .await
                .log_error_msg(|| "background: error getting option counts");
            if option_counts.is_err() {
                continue;
            }
            let option_counts = option_counts.unwrap();
            let option_counts = option_counts
                .into_iter()
                .map(|count| (count.multi_selection, count.count))
                .collect::<HashMap<i64, i64>>();
            let option_counts = options
                .into_iter()
                .map(|opt| (opt.id, option_counts.get(&opt.id).unwrap_or(&0)))
                .collect::<Vec<_>>();
            for (multi_selection, count) in option_counts {
                sqlx::query(
                    r##"
                    insert into pin.question_multi_option_tallies
                    (question_id, multi_selection, count)
                    values
                    ($1, $2, $3)
                    on conflict (question_id, multi_selection) where deleted is false
                    do update set
                        count = $3
                    "##,
                )
                .bind(question.id)
                .bind(multi_selection)
                .bind(count)
                .execute(&mut *tr)
                .await
                .map_err(AppError::from)
                .log_error_msg(|| "background: error upserting multi selection count")
                .ok();
            }
            tr.commit()
                .await
                .map_err(AppError::from)
                .log_error_msg(|| "background: error commiting changes")
                .ok();
        }
    }

    tokio::spawn(tally_question_of_the_day(pool.clone()));

    async fn set_question_of_the_day(pool: PgPool) {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let question: Result<Question> = sqlx::query_as(QOD_QUERY)
                .fetch_one(&pool)
                .await
                .map_err(|e| {
                    tracing::error!("background: error loading question of the day {:?}", e);
                    AppError::from(e)
                });
            if question.is_err() {
                continue;
            }
            let question = question.unwrap();
            if matches!(question.used, None) {
                let tr = pool.begin().await.map_err(|e| {
                    tracing::error!("background: error starting transaction {:?}", e);
                    AppError::from(e)
                });
                if tr.is_err() {
                    continue;
                }
                let mut tr = tr.unwrap();
                let question = Question::mark_used(question.id, &mut tr)
                    .await
                    .map_err(|e| {
                        tracing::error!("background: error marking question used {:?}", e);
                        e
                    });
                if question.is_err() {
                    continue;
                }
                tr.commit()
                    .await
                    .map_err(|e| {
                        tracing::error!(
                            "background: error marking question used transaction {:?}",
                            e
                        );
                        AppError::from(e)
                    })
                    .ok();
            }
        }
    }

    tokio::spawn(set_question_of_the_day(pool.clone()));

    if !CONFIG.secure_cookie {
        tracing::warn!("*** SECURE COOKIE IS DISABLED ***");
    }
    tracing::info!(
        version = %CONFIG.version,
        addr = %addr,
        "starting server",
    );
    warp::serve(routes)
        .run(
            addr.parse::<SocketAddr>()
                .map_err(|e| format!("invalid host/port: {addr}, {e}"))
                .unwrap(),
        )
        .await;
    Ok(())
}
