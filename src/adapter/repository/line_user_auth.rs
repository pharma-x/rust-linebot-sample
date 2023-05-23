use crate::adapter::model::line_user_auth::ResponseLineAuth;
use crate::adapter::repository::user_auth::UserAuthRepository;
use crate::domain::model::line_user_auth::LineUserAuthData;
use crate::domain::model::user::UserProfile;
use crate::domain::model::user_auth::{AuthToken, AuthUserId, UserAuthData};
use crate::domain::repository::line_user_auth::LineUserAuthRepository;
use anyhow::Ok;
use async_trait::async_trait;
use reqwest::header;

#[async_trait]
impl LineUserAuthRepository for UserAuthRepository<LineUserAuthData> {
    async fn get_user_profile(&self, source: LineUserAuthData) -> anyhow::Result<UserProfile> {
        let body = &self
            .client
            .get(format!(
                "https://api.line.me/v2/bot/profile/{}",
                source.user_id().value()
            ))
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", source.token().value()),
            )
            .send()
            .await?
            .text()
            .await?;

        let res_line_auth: ResponseLineAuth = serde_json::from_str(&body).expect(&format!(
            "cannot convert ResponseLineAuth instance. body: {}",
            body
        ));

        // LineUserProfileをUserProfileに変換する
        Ok(UserProfile::Line(res_line_auth.try_into()?))
    }
}
