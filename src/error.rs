use async_graphql::{ErrorExtensions, FieldError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("error")]
    E(String),

    #[error("db error, not found")]
    DBNotFound(sqlx::Error),

    #[error("db error, unique constraint violation")]
    DBUniqueContraintViolation { code: String, constraint: String },

    #[error("db error")]
    DB(sqlx::Error),

    #[error("unauthorized")]
    Unauthorized(String),

    #[error("unverified")]
    Unverified(String),

    #[allow(unused)]
    #[error("forbidden")]
    Forbidden(String),

    #[error("bad request")]
    BadRequest(String),

    #[error("hex error")]
    Hex(#[from] hex::FromHexError),

    #[error("request error")]
    Reqwest(#[from] reqwest::Error),

    #[error("json error")]
    Json(#[from] serde_json::Error),

    #[error("base64 decode error")]
    Base64Decode(#[from] base64::DecodeError),
}
impl From<&str> for AppError {
    fn from(s: &str) -> AppError {
        AppError::E(s.to_string())
    }
}
impl From<String> for AppError {
    fn from(s: String) -> AppError {
        AppError::E(s)
    }
}
impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> AppError {
        match error {
            sqlx::Error::RowNotFound => AppError::DBNotFound(error),
            sqlx::Error::Database(ref e) => {
                let code = e.code().map(String::from);
                let constraint = e.constraint();
                // https://www.postgresql.org/docs/current/errcodes-appendix.html
                if code.is_some() && code.as_ref().unwrap() == "23505" {
                    #[allow(clippy::unnecessary_unwrap)]
                    AppError::DBUniqueContraintViolation {
                        code: code.unwrap(),
                        constraint: constraint.expect("expected constraint name").to_string(),
                    }
                } else {
                    AppError::DB(error)
                }
            }
            _ => AppError::DB(error),
        }
    }
}

impl AppError {
    pub fn unique_constraint_error(&self) -> Option<(String, String)> {
        match self {
            AppError::DBUniqueContraintViolation { code, constraint } => {
                Some((code.to_string(), constraint.to_string()))
            }
            _ => None,
        }
    }
}

impl ErrorExtensions for AppError {
    fn extend(&self) -> FieldError {
        self.extend_with(|err, e| match err {
            AppError::E(s) => {
                e.set("code", "500");
                e.set("error", s.clone());
            }
            AppError::DB(_) => e.set("code", 500),
            AppError::DBNotFound(_) => e.set("code", 404),
            #[allow(unused_variables)]
            AppError::DBUniqueContraintViolation { code, constraint } => e.set("code", 500),
            AppError::Unverified(s) => {
                e.set("code", 401);
                e.set("error", s.clone());
                e.set("key", "UNVERIFIED");
            }
            AppError::Unauthorized(s) => {
                e.set("code", 401);
                e.set("error", s.clone());
            }
            AppError::Forbidden(s) => {
                e.set("code", 403);
                e.set("error", s.clone());
            }
            AppError::BadRequest(s) => {
                e.set("code", 400);
                e.set("error", s.clone());
            }
            AppError::Hex(_) => e.set("code", 500),
            AppError::Reqwest(_) => e.set("code", 500),
            AppError::Json(_) => e.set("code", 500),
            AppError::Base64Decode(_) => e.set("code", 500),
        })
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
