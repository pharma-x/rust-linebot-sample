use crate::model::event::CreateUserEvent;
use adapter::{module::RepositoriesModuleExt, repository::RepositoryError};
use derive_new::new;
use domain::{
    model::event::NewEvent,
    repository::{
        talk_room::TalkRoomRepository, user::UserRepository, user_auth::UserAuthRepository,
    },
};
use std::sync::Arc;

#[derive(new)]
pub struct LinebotWebhookUseCase<R: RepositoriesModuleExt> {
    repositories: Arc<R>,
}

impl<R: RepositoriesModuleExt> LinebotWebhookUseCase<R> {
    pub async fn create_user(&self, source: CreateUserEvent) -> anyhow::Result<()> {
        /*
         * userを取得、なければ作成する
         */
        let create_line_user_auth = source.clone().create_line_user_auth;
        let res_user = self
            .repositories
            .user_repository()
            .get_user(create_line_user_auth.clone().into())
            .await;
        let user = match res_user {
            Ok(s) => s,
            Err(anyhow_err) => {
                if let Some(repository_err) = anyhow_err.downcast_ref::<RepositoryError>() {
                    match repository_err {
                        RepositoryError::NotAuthFound(_) => {
                            let user_profile = self
                                .repositories
                                .user_auth_repository()
                                .get_user_profile(create_line_user_auth.try_into()?)
                                .await?;
                            self.repositories
                                .user_repository()
                                .create_user(user_profile)
                                .await?
                        }
                        _ => return Err(anyhow_err),
                    }
                } else {
                    // anyhow_errがRepositoryErrorではない場合
                    return Err(anyhow_err);
                }
            }
        };

        /*
         * talk_roomを取得、なければ作成する
         */
        let new_event = NewEvent::from(source.create_event);
        let res_talk_room = self
            .repositories
            .talk_room_repository()
            .get_talk_room(user.clone().id)
            .await;
        let talk_room = match res_talk_room {
            Ok(s) => s,
            Err(anyhow_err) => {
                if let Some(repository_err) = anyhow_err.downcast_ref::<RepositoryError>() {
                    match repository_err {
                        RepositoryError::NotFound(_) => {
                            self.repositories
                                .talk_room_repository()
                                .create_talk_room((user, new_event.clone()).into())
                                .await?
                        }
                        _ => return Err(anyhow_err),
                    }
                } else {
                    // anyhow_errがRepositoryErrorではない場合
                    return Err(anyhow_err);
                }
            }
        };

        let create_event_talk_room = (talk_room, new_event).into();
        self.repositories
            .talk_room_repository()
            .create_event(create_event_talk_room)
            .await?;

        Ok(())
    }
}