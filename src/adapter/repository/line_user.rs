use crate::adapter::model::{line_user::LineUserTable, primary_user::PrimaryUserTable};
use crate::adapter::repository::DatabaseRepositoryImpl;
use crate::domain::model::line_user::{LineUser, LineUserProfile};
use crate::domain::repository::line_user::LineUserRepository;
use async_trait::async_trait;

#[async_trait]
impl LineUserRepository for DatabaseRepositoryImpl<LineUser> {
    async fn create_user(&self, source: LineUserProfile) -> anyhow::Result<LineUser> {
        let pool = self.pool.0.clone();
        let tx = pool.begin().await.expect("Unable to begin transaction");
        let primary_user_row = sqlx::query_as::<_, PrimaryUserTable>(
            r#"
insert into primary_users
values (default)
returning *"#,
        )
        .fetch_one(&*pool)
        .await
        .expect("Unable to insert a primary user");

        let line_user_row = sqlx::query_as::<_, LineUserTable>(
            r#"
insert into line_users(primary_user_id, line_id, display_name, picture_url, created_at, updated_at)
values ($1, $2, $3, $4, default, default)
returning *"#,
        )
        .bind(primary_user_row.id)
        .bind(source.auth_id.value())
        .bind(source.display_name)
        .bind(source.picture_url)
        .fetch_one(&*pool)
        .await
        .expect("Unable to insert a primary user");

        tx.commit().await.expect("Unable to commit transaction");

        Ok(line_user_row.try_into()?)
    }
}
