use crate::error::LogError;
use crate::loaders::{
    AppLoader, FriendsForUserId, GroupAssociationsForUserId, MultiOptionsForQuestion,
    PinionForQuestion, PinionsOfFriendsForUserQuestionId, ProfileForUserId, QuestionOfDay, UserId,
};
use crate::{AppError, Result};
use async_graphql::{Context, ErrorExtensions, FieldResult, Object, ResultExt};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;

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
    /// The user ID
    async fn id(&self) -> String {
        self.id.to_string()
    }

    /// The user's human name, prefer loading user.profile.name
    async fn name(&self, ctx: &Context<'_>) -> FieldResult<Option<String>> {
        let u = ctx.data_opt::<User>().expect("no current user");
        let p = ctx
            .data_unchecked::<AppLoader>()
            .load_one(ProfileForUserId(u.id))
            .await?;
        Ok(p.and_then(|p| p.name))
    }

    async fn profile(&self, ctx: &Context<'_>) -> FieldResult<Option<Profile>> {
        let u = ctx.data_opt::<User>().expect("no current user");
        let p = ctx
            .data_unchecked::<AppLoader>()
            .load_one(ProfileForUserId(u.id))
            .await?;
        Ok(p)
    }

    /// The user's chosen handle/username
    async fn handle(&self) -> &str {
        &self.handle
    }

    /// Whether this user still needs to be setup with a handle.
    /// If this is true, then it means that the account was just
    /// created and the user needs to set a handle befor being able
    /// to use the app. Handle can be set using the `setHandle` mutation
    async fn needs_handle(&self) -> bool {
        uuid::Uuid::try_parse(&self.handle).is_ok()
    }

    /// The UTC time at which this user's phone number was last verified.
    /// This is the last time that a user entered a valid verification code.
    async fn phone_verified(&self) -> Option<DateTime<Utc>> {
        self.phone_verified
    }

    async fn friends(&self, ctx: &Context<'_>) -> FieldResult<Vec<Friend>> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(FriendsForUserId(self.id))
            .await?
            .unwrap_or_default();
        Ok(r)
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
pub struct FriendUser {
    pub id: i64,
    pub handle: String,
    pub phone_number: String,
}

impl From<User> for FriendUser {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            handle: u.handle,
            phone_number: u.phone_number,
        }
    }
}

#[Object]
impl FriendUser {
    async fn id(&self) -> String {
        self.id.to_string()
    }

    /// The user's human name, prefer loading user.profile.name
    async fn name(&self, ctx: &Context<'_>) -> FieldResult<Option<String>> {
        let u = ctx.data_opt::<User>().expect("no current user");
        let p = ctx
            .data_unchecked::<AppLoader>()
            .load_one(ProfileForUserId(u.id))
            .await?;
        Ok(p.and_then(|p| p.name))
    }

    async fn profile(&self, ctx: &Context<'_>) -> FieldResult<Option<Profile>> {
        let u = ctx.data_opt::<User>().expect("no current user");
        let p = ctx
            .data_unchecked::<AppLoader>()
            .load_one(ProfileForUserId(u.id))
            .await?;
        Ok(p)
    }
    async fn handle(&self) -> &str {
        &self.handle
    }
    async fn phone_number(&self) -> &str {
        &self.phone_number
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct Profile {
    pub id: i64,
    pub user_id: i64,
    pub name: Option<String>,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[Object]
impl Profile {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn user_id(&self) -> String {
        self.id.to_string()
    }
    async fn name(&self) -> &Option<String> {
        &self.name
    }
}

#[derive(Clone)]
pub struct LoginSuccess {
    pub auth_token: String,
    pub user: User,
}

#[Object]
impl LoginSuccess {
    async fn auth_token(&self) -> String {
        self.auth_token.clone()
    }
    async fn user(&self) -> &User {
        &self.user
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

#[derive(Clone, sqlx::FromRow)]
pub struct PhoneCheck {
    pub number: String,
    pub signed_up: bool,
}

#[Object]
impl PhoneCheck {
    async fn number(&self) -> &str {
        &self.number
    }
    async fn signed_up(&self) -> bool {
        self.signed_up
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Friend {
    pub id: i64,
    pub requestor_id: i64,
    pub acceptor_id: i64,
    pub accepted: Option<DateTime<Utc>>,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[Object]
impl Friend {
    async fn relationship_id(&self) -> String {
        self.id.to_string()
    }
    async fn accepted(&self) -> Option<DateTime<Utc>> {
        self.accepted
    }
    async fn user(&self, ctx: &Context<'_>) -> FieldResult<FriendUser> {
        let u = ctx.data_opt::<User>().expect("no current user");
        let other_user_id = if self.acceptor_id != u.id {
            self.acceptor_id
        } else {
            self.requestor_id
        };
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(UserId(other_user_id))
            .await?
            .ok_or_else(|| {
                AppError::E(format!(
                    "missing expected friend user {} of user {}",
                    other_user_id, u.id
                ))
                .extend()
            })?
            .into();
        Ok(r)
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

#[derive(Debug, Clone, sqlx::FromRow)]
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

    pub async fn get_options(
        id: i64,
        tr: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<Vec<QuestionMultiOption>> {
        let options: Vec<QuestionMultiOption> = sqlx::query_as(
            r##"select * from pin.question_multi_options where question_id = $1 and deleted is false order by rank"##,
        ).bind(id)
            .fetch_all(&mut *tr)
            .await
            .map_err(AppError::from)?;
        Ok(options)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QuestionOptionCount {
    pub multi_selection: i64,
    pub count: i64,
}

impl QuestionOptionCount {
    pub async fn get_option_counts(
        question_id: i64,
        tr: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<Vec<Self>> {
        let counts: Vec<Self> = sqlx::query_as(
            r##"
            select multi_selection, count(*)
            from pin.pinions
            where question_id = $1 and deleted is false
            group by multi_selection
            "##,
        )
        .bind(question_id)
        .fetch_all(&mut *tr)
        .await
        .map_err(AppError::from)?;
        Ok(counts)
    }
    pub async fn get_option_counts_friends(
        question_id: i64,
        user_id: i64,
        tr: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<Vec<Self>> {
        let counts: Vec<Self> = sqlx::query_as(
            r##"
            select p.multi_selection, count(distinct (p.id, p.user_id))
            from pin.pinions p
                inner join pin.friends f
                    on p.user_id = f.requestor_id
                    or p.user_id = f.acceptor_id
            where
                 p.question_id = $1
                 and (f.requestor_id = $2 or f.acceptor_id = $2)
                 and p.deleted is false
                 and f.deleted is false
            group by p.multi_selection
            "##,
        )
        .bind(question_id)
        .bind(user_id)
        .fetch_all(&mut *tr)
        .await
        .map_err(AppError::from)?;
        Ok(counts)
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

    /// Question prompt
    async fn prompt(&self) -> String {
        self.prompt.clone()
    }

    /// The current user's response to this question
    async fn pinion(&self, ctx: &Context<'_>) -> FieldResult<Option<Pinion>> {
        let u = ctx.data_opt::<User>().expect("no current user");
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(PinionForQuestion(self.id, u.id))
            .await?;
        Ok(r)
    }

    /// List of answer options
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

    /// A summary of responses (counts and percentages)
    async fn summary(&self, ctx: &Context<'_>) -> FieldResult<QuestionSummary> {
        let pool = ctx.data_unchecked::<PgPool>();
        question_summary(self.id, pool)
            .await
            .log_error_msg(|| "error querying summary")
            .extend_err(|_e, ex| ex.set("key", "DATABASE_ERROR"))
    }

    /// A summary of friends responses (counts and percentages)
    async fn friend_summary(&self, ctx: &Context<'_>) -> FieldResult<QuestionSummary> {
        let u = ctx.data_opt::<User>().expect("no current user");
        let pool = ctx.data_unchecked::<PgPool>();
        question_friends_summary(self.id, u.id, pool)
            .await
            .log_error_msg(|| "error querying summary")
            .extend_err(|_e, ex| ex.set("key", "DATABASE_ERROR"))
    }

    /// Load pinions of friends for this question
    async fn friend_pinions(&self, ctx: &Context<'_>) -> FieldResult<Vec<FriendPinion>> {
        let u = ctx.data_opt::<User>().expect("no current user");
        ctx.data_unchecked::<AppLoader>()
            .load_one(PinionsOfFriendsForUserQuestionId(u.id, self.id))
            .await?
            .map(|ps| ps.into_iter().map(FriendPinion::from).collect())
            .ok_or_else(|| {
                AppError::from(format!(
                    "unable to load pinions of friends for user {}, q {}",
                    u.id, self.id
                ))
            })
            .extend()
    }
}

use cached::proc_macro::cached;
use cached::TimedSizedCache;

#[cached(
    result = true,
    sync_writes = true,
    type = "TimedSizedCache<i64, QuestionSummary>",
    create = "{ TimedSizedCache::with_size_and_lifespan(10, 10) }",
    convert = r#"{ id }"#
)]
async fn question_summary(id: i64, pool: &PgPool) -> Result<QuestionSummary> {
    tracing::info!("loading question summary for question {}", id);
    let query = r##"
        select * from pin.question_multi_option_tallies
            where
                deleted is false
                and question_id = $1
            order by count asc
        "##;
    let res: Vec<QuestionMultiOptionTally> = sqlx::query_as(query)
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(AppError::from)
        .log_error_msg(|| "error loading question multi option tallies")?;
    tracing::info!("loaded {} question multi option tallies", res.len());
    if res.is_empty() {
        return Ok(QuestionSummary {
            total_count: 0,
            options: vec![],
        });
    }
    let total_count = res.iter().map(|ot| ot.count).sum();
    let options = res
        .into_iter()
        .map(|ot| ot.to_option_summary(total_count))
        .collect::<Vec<_>>();
    Ok(QuestionSummary {
        total_count,
        options,
    })
}

#[cached(
    result = true,
    sync_writes = true,
    type = "TimedSizedCache<(i64, i64), QuestionSummary>",
    create = "{ TimedSizedCache::with_size_and_lifespan(10, 10) }",
    convert = r#"{ (id, user_id) }"#
)]
async fn question_friends_summary(id: i64, user_id: i64, pool: &PgPool) -> Result<QuestionSummary> {
    tracing::info!(
        "loading friend question summary for question {} user {}",
        id,
        user_id
    );
    let mut tr = pool
        .begin()
        .await
        .map_err(AppError::from)
        .log_error_msg(|| "friends: error starting transaction {:?}")?;
    let options = Question::get_options(id, &mut tr)
        .await
        .log_error_msg(|| "friends: error getting question options")?;
    let option_counts = QuestionOptionCount::get_option_counts_friends(id, user_id, &mut tr)
        .await
        .log_error_msg(|| "friends: error getting option counts")?;
    let option_counts = option_counts
        .into_iter()
        .map(|count| (count.multi_selection, count.count))
        .collect::<HashMap<i64, i64>>();

    let mut total_count = 0;
    let mut option_tallies = vec![];
    for opt in options.into_iter() {
        let count = option_counts.get(&opt.id).unwrap_or(&0);
        total_count += count;
        option_tallies.push(QuestionMultiOptionTally {
            id: 0,
            question_id: id,
            multi_selection: opt.id,
            count: *count,
            deleted: false,
        });
    }
    let option_summaries = option_tallies
        .into_iter()
        .map(|opt| opt.to_option_summary(total_count))
        .collect::<Vec<_>>();
    tr.commit()
        .await
        .map_err(AppError::from)
        .log_error_msg(|| "friends: error committing changes")?;
    let res = QuestionSummary {
        total_count,
        options: option_summaries,
    };
    Ok(res)
}

#[derive(Clone, sqlx::FromRow)]
pub struct QuestionMultiOptionTally {
    pub id: i64,
    pub question_id: i64,
    pub multi_selection: i64,
    pub count: i64,
    pub deleted: bool,
}

impl QuestionMultiOptionTally {
    fn to_option_summary(&self, total_answer_count: i64) -> OptionSummary {
        OptionSummary {
            option_id: self.multi_selection,
            count: self.count,
            percentage: (self.count as f64 / total_answer_count as f64 * 100.).round() as i64,
        }
    }
}

#[derive(Clone)]
pub struct QuestionSummary {
    pub total_count: i64,
    pub options: Vec<OptionSummary>,
}

#[Object]
impl QuestionSummary {
    async fn total_count(&self) -> i64 {
        self.total_count
    }
    async fn options(&self) -> &[OptionSummary] {
        &self.options
    }
}

#[derive(Clone)]
pub struct OptionSummary {
    option_id: i64,
    count: i64,
    percentage: i64,
}

#[Object]
impl OptionSummary {
    /// The identifier of the option associated with the question
    async fn id(&self) -> String {
        self.option_id.to_string()
    }
    /// Total count for this option
    async fn count(&self) -> i64 {
        self.count
    }
    /// Percentage of responses that chose this option
    async fn percentage(&self) -> i64 {
        self.percentage
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
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
pub struct PinionWithFriendRelation {
    pub id: i64,
    pub user_id: i64,
    pub requestor_id: i64,
    pub acceptor_id: i64,
    pub question_id: i64,
    pub multi_selection: i64,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
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

impl From<PinionWithFriendRelation> for Pinion {
    fn from(p: PinionWithFriendRelation) -> Self {
        Self {
            id: p.id,
            user_id: p.user_id,
            question_id: p.question_id,
            multi_selection: p.multi_selection,
            deleted: p.deleted,
            created: p.created,
            modified: p.modified,
        }
    }
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

#[derive(Clone, sqlx::FromRow)]
pub struct FriendPinion {
    pub id: i64,
    pub user_id: i64,
    pub question_id: i64,
    pub multi_selection: i64,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}
impl From<Pinion> for FriendPinion {
    fn from(p: Pinion) -> Self {
        Self {
            id: p.id,
            user_id: p.user_id,
            question_id: p.question_id,
            multi_selection: p.multi_selection,
            deleted: p.deleted,
            created: p.created,
            modified: p.modified,
        }
    }
}

#[Object]
impl FriendPinion {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn user(&self, ctx: &Context<'_>) -> FieldResult<FriendUser> {
        ctx.data_unchecked::<AppLoader>()
            .load_one(UserId(self.user_id))
            .await?
            .map(FriendUser::from)
            .ok_or_else(|| AppError::from(format!("unable to load friend user {}", self.user_id)))
            .extend()
    }
    async fn question_id(&self) -> String {
        self.question_id.to_string()
    }
    async fn multi_selection_id(&self) -> String {
        self.multi_selection.to_string()
    }
}
