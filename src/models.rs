use crate::loaders::{AppLoader, GroupAssociationsForUserId, UserId};
use crate::{AppError, Result};
use async_graphql::{Context, ErrorExtensions, FieldResult, Object};
use chrono::{DateTime, Utc};

#[derive(Clone, sqlx::FromRow)]
pub struct BaseUser {
    pub id: i64,
    pub name: String,
    pub handle: String,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub handle: String,
    pub phone_number: String,
    pub phone_verified: Option<DateTime<Utc>>,
    pub phone_verification_sent: Option<DateTime<Utc>>,
    pub phone_verification_attempts: i32,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}
impl User {
    pub async fn fetch_user(
        tr: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: i64,
    ) -> Result<User> {
        let user: User = sqlx::query_as(
            r##"
           select
               u.*,
               p.number as phone_number,
               p.verified as phone_verified,
               p.verification_sent as phone_verification_sent,
               p.verification_attempts as phone_verification_attempts
           from pin.users u
               inner join pin.phones p on p.user_id = u.id
           where u.id = $1
               and u.deleted is false
               and p.deleted is false
           "##,
        )
        .bind(user_id)
        .fetch_one(&mut *tr)
        .await
        .map_err(AppError::from)?;
        Ok(user)
    }
    pub async fn fetch_user_by_handle_number(
        tr: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        handle: &str,
        phone_number: &str,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as(
            r##"
           select
               u.*,
               p.number as phone_number,
               p.verified as phone_verified,
               p.verification_sent as phone_verification_sent,
               p.verification_attempts as phone_verification_attempts
           from pin.users u
               inner join pin.phones p on p.user_id = u.id
           where u.handle = $1
               and p.number = $2
               and u.deleted is false
               and p.deleted is false
           "##,
        )
        .bind(handle)
        .bind(phone_number)
        .fetch_optional(&mut *tr)
        .await
        .map_err(AppError::from)?;
        Ok(user)
    }
}

#[Object]
impl User {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn name(&self) -> &str {
        &self.name
    }

    async fn handle(&self) -> &str {
        &self.handle
    }
    async fn group_associations(&self, ctx: &Context<'_>) -> FieldResult<Vec<GroupAssociation>> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(GroupAssociationsForUserId(self.id))
            .await?
            .unwrap_or_else(Vec::new);
        Ok(r)
    }
    async fn created(&self) -> DateTime<Utc> {
        self.created
    }
    async fn modified(&self) -> DateTime<Utc> {
        self.modified
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct SimpleUser {
    pub id: i64,
    pub handle: String,
}
impl std::convert::From<User> for SimpleUser {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            handle: u.handle,
        }
    }
}
#[Object]
impl SimpleUser {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn handle(&self) -> &str {
        &self.handle
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct VerificationCode {
    pub id: i64,
    pub user_id: i64,
    pub salt: String,
    pub hash: String,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow)]
pub struct Phone {
    pub id: i64,
    pub number: String,
    pub verification_attempts: i32,
    pub verification_sent: Option<DateTime<Utc>>,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[Object]
impl Phone {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn number(&self) -> &str {
        &self.number
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Group {
    pub id: i64,
    pub creating_user_id: i64,
    pub name: String,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[Object]
impl Group {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn name(&self) -> &str {
        &self.name
    }
    async fn creating_user(&self, ctx: &Context<'_>) -> FieldResult<SimpleUser> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(UserId(self.creating_user_id))
            .await?
            .ok_or_else(|| {
                AppError::E(format!(
                    "missing expected creating_user_id {} of group {}",
                    self.creating_user_id, self.id
                ))
                .extend()
            })?
            .into();
        Ok(r)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GroupAssociation {
    pub id: i64,
    pub user_id: i64,
    pub group_id: i64,
    pub role: String,
    pub sort_rank: i64,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[Object]
impl GroupAssociation {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn role(&self) -> &str {
        &self.role
    }
    async fn user(&self, ctx: &Context<'_>) -> FieldResult<SimpleUser> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(UserId(self.user_id))
            .await?
            .ok_or_else(|| {
                AppError::E(format!(
                    "missing expected user {} of group association {}",
                    self.user_id, self.id
                ))
                .extend()
            })?
            .into();
        Ok(r)
    }
    async fn created(&self) -> DateTime<Utc> {
        self.created
    }
    async fn modified(&self) -> DateTime<Utc> {
        self.modified
    }
}
