use crate::loaders::{AppLoader, GroupAssociationsForUserId, UserId};
use crate::AppError;
use async_graphql::{Context, ErrorExtensions, FieldResult, Object};
use chrono::{Date, DateTime, Utc};

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
    pub phone_verification_attempts: i32,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
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
