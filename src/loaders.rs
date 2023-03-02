use crate::models::{
    Friend, GroupAssociation, Pinion, Profile, Question, QuestionMultiOption, User,
};
use crate::AppError;
use async_graphql::dataloader::{DataLoader, HashMapCache};
use sqlx::PgPool;
use std::collections::HashMap;

pub struct PgLoader {
    pool: PgPool,
}
impl PgLoader {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
pub type AppLoader = DataLoader<PgLoader, HashMapCache>;

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct UserId(pub i64);

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<UserId> for PgLoader {
    type Value = User;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[UserId],
    ) -> std::result::Result<HashMap<UserId, Self::Value>, Self::Error> {
        tracing::info!("loading {} users", keys.len());
        let query = r##"
            select u.*, p.number as phone_number, p.verified as phone_verified,
            p.verification_sent as phone_verification_sent,
            p.verification_attempts as phone_verification_attempts
            from pin.users u
                inner join pin.phones p on p.user_id = u.id
            where u.id in (select * from unnest($1))
        "##;
        let u_ids = keys.iter().map(|c| c.0).collect::<Vec<_>>();
        let res: Vec<User> = sqlx::query_as(query)
            .bind(&u_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("error loading users: {:?}", e);
                AppError::from(e)
            })?;
        tracing::info!("loaded {} users", res.len());
        let res = res.into_iter().fold(HashMap::new(), |mut acc, u| {
            acc.insert(UserId(u.id), u);
            acc
        });
        Ok(res)
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct ProfileForUserId(pub i64);

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<ProfileForUserId> for PgLoader {
    type Value = Profile;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[ProfileForUserId],
    ) -> std::result::Result<HashMap<ProfileForUserId, Self::Value>, Self::Error> {
        tracing::info!("loading {} profiles", keys.len());
        let query = r##"
            select * from pin.profiles
            where user_id in (select * from unnest($1))
        "##;
        let u_ids = keys.iter().map(|c| c.0).collect::<Vec<_>>();
        let res: Vec<Profile> = sqlx::query_as(query)
            .bind(&u_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("error loading profiles: {:?}", e);
                AppError::from(e)
            })?;
        tracing::info!("loaded {} profiles", res.len());
        let res = res.into_iter().fold(HashMap::new(), |mut acc, p| {
            acc.insert(ProfileForUserId(p.user_id), p);
            acc
        });
        Ok(res)
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct GroupAssociationsForUserId(pub i64);

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<GroupAssociationsForUserId> for PgLoader {
    type Value = Vec<GroupAssociation>;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[GroupAssociationsForUserId],
    ) -> std::result::Result<HashMap<GroupAssociationsForUserId, Self::Value>, Self::Error> {
        tracing::info!("loading group associations for {} users", keys.len());
        let query = r##"
            select ga.* from pin.group_associations ga
                inner join pin.users u on ga.user_id = u.id
            where ga.user_id in (select * from unnest($1))
                and ga.deleted is false
                and u.deleted is false
        "##;
        let keys = keys.iter().map(|ga| ga.0).collect::<Vec<_>>();
        let res: Vec<GroupAssociation> = sqlx::query_as(query)
            .bind(&keys)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;
        tracing::info!("loaded {} group associations", res.len());
        let res = res.into_iter().fold(HashMap::new(), |mut acc, ga| {
            {
                let e = acc
                    .entry(GroupAssociationsForUserId(ga.user_id))
                    .or_insert_with(Vec::new);
                e.push(ga);
            }
            acc
        });
        Ok(res)
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct FriendsForUserId(pub i64);

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<FriendsForUserId> for PgLoader {
    type Value = Vec<Friend>;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[FriendsForUserId],
    ) -> std::result::Result<HashMap<FriendsForUserId, Self::Value>, Self::Error> {
        tracing::info!("loading friends for {} users", keys.len());
        let query = r##"
            select * from pin.friends
            where requestor_id in (select * from unnest($1))
                or acceptor_id in (select * from unnest($1))
                and deleted is false;
        "##;
        let keys = keys.iter().map(|u| u.0).collect::<Vec<_>>();
        let res: Vec<Friend> = sqlx::query_as(query)
            .bind(&keys)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;
        tracing::info!("loaded {} friends", res.len());
        let res = res.into_iter().fold(HashMap::new(), |mut acc, f| {
            {
                // Need to push both sides because we don't know who is "asking"
                // When we the dataloader iterators over the input keys, both "sides"
                // of the friendship will be available to pull.
                let e = acc
                    .entry(FriendsForUserId(f.requestor_id))
                    .or_insert_with(Vec::new);
                e.push(f.clone());
                let e = acc
                    .entry(FriendsForUserId(f.acceptor_id))
                    .or_insert_with(Vec::new);
                e.push(f);
            }
            acc
        });
        Ok(res)
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct QuestionOfDay;

pub static QOD_QUERY: &str = r##"
        select * from pin.questions
            where
                deleted is false and
                (
                    used::date >= timezone('America/New_York', now())::date
                    or used is null
                )
            order by used asc nulls last, priority asc nulls last, created asc
            limit 1
        "##;

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<QuestionOfDay> for PgLoader {
    type Value = Question;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[QuestionOfDay],
    ) -> std::result::Result<HashMap<QuestionOfDay, Self::Value>, Self::Error> {
        tracing::info!("loading question of the day");
        let question: Question = sqlx::query_as(QOD_QUERY)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("error loading question of the day {:?}", e);
                AppError::from(e)
            })?;
        tracing::info!("loaded question of the day");
        let mut res = HashMap::new();
        res.insert(keys[0].clone(), question);
        Ok(res)
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct MultiOptionsForQuestion(pub i64);

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<MultiOptionsForQuestion> for PgLoader {
    type Value = Vec<QuestionMultiOption>;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[MultiOptionsForQuestion],
    ) -> std::result::Result<HashMap<MultiOptionsForQuestion, Self::Value>, Self::Error> {
        tracing::info!("loading multi options for {} questions", keys.len());
        let query = r##"
        select * from pin.question_multi_options
            where
                deleted is false and
                question_id in (select * from unnest($1))
            order by rank asc
        "##;
        let keys = keys.iter().map(|ga| ga.0).collect::<Vec<_>>();
        let res: Vec<QuestionMultiOption> = sqlx::query_as(query)
            .bind(&keys)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("error loading question multi options {:?}", e);
                AppError::from(e)
            })?;
        tracing::info!("loaded {} question multi options", res.len());
        let res = res.into_iter().fold(HashMap::new(), |mut acc, opt| {
            {
                let e = acc
                    .entry(MultiOptionsForQuestion(opt.question_id))
                    .or_insert_with(Vec::new);
                e.push(opt);
            }
            acc
        });
        Ok(res)
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct PinionForQuestion(pub i64, pub i64);

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<PinionForQuestion> for PgLoader {
    type Value = Pinion;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[PinionForQuestion],
    ) -> std::result::Result<HashMap<PinionForQuestion, Self::Value>, Self::Error> {
        tracing::info!("loading pinions for {} questions", keys.len());
        let query = r##"
        select * from pin.pinions
            where
                deleted is false and
                question_id in (select * from unnest($1)) and
                user_id in (select * from unnest($2))
        "##;
        let q_ids = keys.iter().map(|ga| ga.0).collect::<Vec<_>>();
        let user_ids = keys.iter().map(|ga| ga.1).collect::<Vec<_>>();
        let res: Vec<Pinion> = sqlx::query_as(query)
            .bind(&q_ids)
            .bind(&user_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("error loading pinions {:?}", e);
                AppError::from(e)
            })?;
        tracing::info!("loaded {} pinions", res.len());
        let res = res.into_iter().fold(HashMap::new(), |mut acc, pin| {
            acc.insert(PinionForQuestion(pin.question_id, pin.user_id), pin);
            acc
        });
        Ok(res)
    }
}

// #[derive(Debug, Clone, Hash, PartialEq, Eq)]
// pub struct CreatureUserId(pub i64, pub i64);
//
// #[async_trait::async_trait]
// impl async_graphql::dataloader::Loader<CreatureUserId> for PgLoader {
//     type Value = CreatureRelation;
//     type Error = std::sync::Arc<AppError>;
//
//     async fn load(
//         &self,
//         keys: &[CreatureUserId],
//     ) -> std::result::Result<HashMap<CreatureUserId, Self::Value>, Self::Error> {
//         tracing::info!("loading {} creatures for users", keys.len());
//         let query = r##"
//             select c.*, ca.user_id, ca.kind from poop.creatures c
//                 inner join poop.creature_access ca on ca.creature_id = c.id
//             where c.deleted is false
//                 and ca.deleted is false
//                 and (
//                     ca.user_id in (select * from unnest($1))
//                     or ca.creature_id in (select * from unnest($2))
//                 )
//         "##;
//         let c_ids = keys.iter().map(|c| c.0).collect::<Vec<_>>();
//         let u_ids = keys.iter().map(|c| c.1).collect::<Vec<_>>();
//         let res: Vec<CreatureRelation> = sqlx::query_as(query)
//             .bind(&u_ids)
//             .bind(&c_ids)
//             .fetch_all(&self.pool)
//             .await
//             .map_err(AppError::from)?;
//         tracing::info!("loaded {} creatures for users", res.len());
//         let res = res.into_iter().fold(HashMap::new(), |mut acc, c| {
//             acc.insert(CreatureUserId(c.id, c.user_id), c);
//             acc
//         });
//         Ok(res)
//     }
// }
//
// #[derive(Clone, Hash, PartialEq, Eq)]
// pub struct CreaturesForUserId(pub i64);
//
// #[async_trait::async_trait]
// impl async_graphql::dataloader::Loader<CreaturesForUserId> for PgLoader {
//     type Value = Vec<CreatureRelation>;
//     type Error = std::sync::Arc<AppError>;
//
//     async fn load(
//         &self,
//         keys: &[CreaturesForUserId],
//     ) -> std::result::Result<HashMap<CreaturesForUserId, Self::Value>, Self::Error> {
//         tracing::info!("loading {} creatures", keys.len());
//         let query = r##"
//             select c.*, ca.user_id, ca.kind from poop.creatures c
//                 inner join poop.creature_access ca on ca.creature_id = c.id
//             where ca.user_id in (select * from unnest($1))
//                 and ca.deleted is false
//                 and c.deleted is false
//         "##;
//         let keys = keys.iter().map(|c| c.0).collect::<Vec<_>>();
//         let res: Vec<CreatureRelation> = sqlx::query_as(query)
//             .bind(&keys)
//             .fetch_all(&self.pool)
//             .await
//             .map_err(AppError::from)?;
//         tracing::info!("loaded {} creatures", res.len());
//         let res = res.into_iter().fold(HashMap::new(), |mut acc, c| {
//             {
//                 let e = acc
//                     .entry(CreaturesForUserId(c.user_id))
//                     .or_insert_with(Vec::new);
//                 e.push(c);
//             }
//             acc
//         });
//         Ok(res)
//     }
// }
//
// #[derive(Clone, Hash, PartialEq, Eq)]
// pub struct PoopsForCreatureId(pub i64);
//
// #[async_trait::async_trait]
// impl async_graphql::dataloader::Loader<PoopsForCreatureId> for PgLoader {
//     type Value = Vec<Poop>;
//     type Error = std::sync::Arc<AppError>;
//
//     async fn load(
//         &self,
//         keys: &[PoopsForCreatureId],
//     ) -> std::result::Result<HashMap<PoopsForCreatureId, Self::Value>, Self::Error> {
//         tracing::info!("loading {} poops for creatures", keys.len());
//         let query = r##"
//             select p.* from poop.poops p
//             where p.creature_id in (select * from unnest($1))
//                 and p.deleted is false
//                 order by p.created desc
//         "##;
//         let keys = keys.iter().map(|c| c.0).collect::<Vec<_>>();
//         let res: Vec<Poop> = sqlx::query_as(query)
//             .bind(&keys)
//             .fetch_all(&self.pool)
//             .await
//             .map_err(AppError::from)?;
//         tracing::info!("loaded {} poops for creatures", res.len());
//         let res = res.into_iter().fold(HashMap::new(), |mut acc, p| {
//             {
//                 let e = acc
//                     .entry(PoopsForCreatureId(p.creature_id))
//                     .or_insert_with(Vec::new);
//                 e.push(p);
//             }
//             acc
//         });
//         Ok(res)
//     }
// }
