use crate::loaders::{
    AppLoader, GroupAssociationsForUserId, MultiOptionsForQuestion, PinionForQuestion,
    QuestionOfDay, UserId,
};
use crate::{AppError, Result};
use async_graphql::{Context, ErrorExtensions, FieldResult, Object};
use chrono::{DateTime, Utc};

#[derive(Clone)]
pub struct ChallengePhone {
    pub number: String,
}

#[derive(Clone, sqlx::FromRow)]
pub struct BaseUser {
    pub id: i64,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub name: Option<String>,
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
               p.verification_attempts as phone_verification_attempts,
               pr.name as name
           from pin.users u
               inner join pin.phones p on p.user_id = u.id
               left outer join pin.profiles pr on pr.user_id = u.id
           where u.id = $1
               and u.deleted is false
               and p.deleted is false
               and (pr.deleted is false or pr.deleted is null)
           "##,
        )
        .bind(user_id)
        .fetch_one(&mut *tr)
        .await
        .map_err(AppError::from)?;
        Ok(user)
    }
    pub async fn fetch_user_by_number(
        tr: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        phone_number: &str,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as(
            r##"
           select
               u.*,
               p.number as phone_number,
               p.verified as phone_verified,
               p.verification_sent as phone_verification_sent,
               p.verification_attempts as phone_verification_attempts,
               pr.name as name
           from pin.users u
               inner join pin.phones p on p.user_id = u.id
               left outer join pin.profiles pr on pr.user_id = u.id
           where p.number = $1
               and u.deleted is false
               and p.deleted is false
               and (pr.deleted is false or pr.deleted is null)
           "##,
        )
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
    async fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    async fn handle(&self) -> &str {
        &self.handle
    }
    async fn needs_handle(&self) -> bool {
        uuid::Uuid::try_parse(&self.handle).is_ok()
    }
    async fn phone_verified(&self) -> Option<DateTime<Utc>> {
        self.phone_verified
    }
    async fn group_associations(&self, ctx: &Context<'_>) -> FieldResult<Vec<GroupAssociation>> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(GroupAssociationsForUserId(self.id))
            .await?
            .unwrap_or_default();
        Ok(r)
    }
    async fn question_of_day(&self, ctx: &Context<'_>) -> FieldResult<Question> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(QuestionOfDay {})
            .await?
            .unwrap();
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
impl From<User> for SimpleUser {
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

#[derive(Clone, sqlx::FromRow)]
pub struct Question {
    pub id: i64,
    pub kind: String,
    pub prompt: String,
    pub used: Option<DateTime<Utc>>,
    pub priority: i64,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}
impl Question {
    pub async fn mark_used(
        id: i64,
        tr: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<Question> {
        let question: Question = sqlx::query_as(
            r##"
            update pin.questions set used = now() where id = $1 returning *
            "##,
        )
        .bind(id)
        .fetch_one(&mut *tr)
        .await
        .map_err(AppError::from)?;
        Ok(question)
    }
}

#[Object]
impl Question {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn kind(&self) -> String {
        self.kind.clone()
    }
    async fn prompt(&self) -> String {
        self.prompt.clone()
    }
    async fn pinion(&self, ctx: &Context<'_>) -> FieldResult<Option<Pinion>> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(PinionForQuestion(self.id))
            .await?;
        Ok(r)
    }
    async fn options(&self, ctx: &Context<'_>) -> FieldResult<Option<Vec<QuestionMultiOption>>> {
        if self.kind != "multi" {
            Ok(None)
        } else {
            let r = ctx
                .data_unchecked::<AppLoader>()
                .load_one(MultiOptionsForQuestion(self.id))
                .await?
                .unwrap_or_default();
            Ok(Some(r))
        }
    }
    async fn summary(&self) -> Option<QuestionSummary> {
        Some(QuestionSummary { id: 1 })
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct QuestionSummary {
    pub id: i64,
    // pub question_id: i64,
    // pub deleted: bool,
    // pub created: DateTime<Utc>,
    // pub modified: DateTime<Utc>,
}

struct AnswerPercentage {
    option_id: i64,
    rank: i64,
    percentage: i64,
}

#[Object]
impl AnswerPercentage {
    async fn option_id(&self) -> String {
        self.option_id.to_string()
    }
    async fn rank(&self) -> i64 {
        self.rank
    }
    async fn percentage(&self) -> i64 {
        self.percentage
    }
}

#[Object]
impl QuestionSummary {
    async fn percentages(&self) -> Vec<AnswerPercentage> {
        vec![
            AnswerPercentage {
                option_id: 1,
                rank: 0,
                percentage: 99,
            },
            AnswerPercentage {
                option_id: 2,
                rank: 1,
                percentage: 1,
            },
        ]
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct QuestionMultiOption {
    pub id: i64,
    pub question_id: i64,
    pub rank: i64,
    pub value: String,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[Object]
impl QuestionMultiOption {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn question_id(&self) -> String {
        self.question_id.to_string()
    }
    async fn rank(&self) -> i64 {
        self.rank
    }
    async fn value(&self) -> String {
        self.value.to_string()
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct Pinion {
    pub id: i64,
    pub user_id: i64,
    pub question_id: i64,
    pub multi_selection: i64,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[Object]
impl Pinion {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn question_id(&self) -> String {
        self.question_id.to_string()
    }
    async fn multi_selection_id(&self) -> String {
        self.multi_selection.to_string()
    }
}
