use crate::model::event::EventTable;
use crate::model::talk_room::{TalkRoomCardTable, TalkRoomDbTable, TalkRoomTable};
use crate::repository::{
    DbFirestoreRepositoryImpl, RepositoryError, EVENT_COLLECTION_NAME,
    TALK_ROOM_CARD_COLLECTION_NAME, TALK_ROOM_COLLECTION_NAME,
};
use anyhow::anyhow;
use async_trait::async_trait;
use domain::{
    model::{
        primary_user_id::PrimaryUserId,
        talk_room::{NewTalkRoom, TalkRoom},
    },
    repository::talk_room::TalkRoomRepository,
};
use firestore::*;
use std::sync::Arc;

#[async_trait]
impl TalkRoomRepository for DbFirestoreRepositoryImpl<TalkRoom> {
    async fn get_talk_room(&self, primary_user_id: PrimaryUserId) -> anyhow::Result<TalkRoom> {
        /*
         * DBのtalk_roomsテーブルからprimary_user_idを元にtalk_roomを取得する
         */
        let pool = Arc::clone(self.db.pool());
        let primary_user_id_str = primary_user_id.value().to_string();
        let talk_room_db_table = sqlx::query_as::<_, TalkRoomDbTable>(
            r#"
            select * from talk_rooms
            where primary_user_id = ?
            "#,
        )
        .bind(primary_user_id_str.clone())
        .fetch_one(&*pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => {
                anyhow!(RepositoryError::NotFound(
                    "talk_rooms".to_string(),
                    primary_user_id_str.clone()
                ))
            }
            _ => anyhow!(RepositoryError::Unexpected(e.to_string())),
        })?;
        println!("talk_room_db_table: {:?}", talk_room_db_table);
        /*
         * FirestoreのtalkRoomsとtalkRoomCardsコレクションからdocument_idを元にtalk_roomとtalk_room_cardを取得する
         */
        let firestore = Arc::clone(&self.firestore.0);
        let document_id = talk_room_db_table.document_id;
        let talk_room_table: TalkRoomTable = firestore
            .fluent()
            .select()
            .by_id_in(TALK_ROOM_COLLECTION_NAME)
            .obj()
            .one(&document_id)
            .await?
            .ok_or(RepositoryError::NotFound(
                TALK_ROOM_COLLECTION_NAME.to_string(),
                document_id.clone(),
            ))?;
        println!("talk_room_table: {:?}", talk_room_table);

        let talk_room_card_table: TalkRoomCardTable = firestore
            .fluent()
            .select()
            .by_id_in(TALK_ROOM_CARD_COLLECTION_NAME)
            .obj()
            .one(&document_id)
            .await?
            .ok_or(RepositoryError::NotFound(
                TALK_ROOM_CARD_COLLECTION_NAME.to_string(),
                document_id.clone(),
            ))?;
        println!("talk_room_card_table: {:?}", talk_room_card_table);

        let event_document_id = talk_room_card_table.latest_message.document_id();
        let event_table: EventTable = firestore
            .fluent()
            .select()
            .by_id_in(EVENT_COLLECTION_NAME)
            .parent(&firestore.parent_path(TALK_ROOM_COLLECTION_NAME, &document_id)?)
            .obj()
            .one(&event_document_id)
            .await?
            .ok_or(RepositoryError::NotFound(
                EVENT_COLLECTION_NAME.to_string(),
                event_document_id.to_string(),
            ))?;
        println!("event_table: {:?}", event_table.clone());

        Ok(TalkRoom::new(
            document_id.try_into()?,
            primary_user_id,
            talk_room_card_table.display_name,
            talk_room_card_table.rsvp,
            talk_room_card_table.pinned,
            talk_room_card_table.follow,
            event_table.into_event(event_document_id.to_string()),
            talk_room_card_table.latest_messaged_at,
            talk_room_card_table.sort_time,
            talk_room_card_table.created_at,
            talk_room_card_table.updated_at,
        ))
    }

    async fn create_talk_room(&self, source: NewTalkRoom) -> anyhow::Result<TalkRoom> {
        let db = Arc::clone(self.db.pool());
        let document_id = source.id.value.to_string();
        // firestoreの書き込みが失敗したときにもDBへの書き込みも
        let mut tx = db.begin().await.expect("Unable to begin transaction");
        sqlx::query(
            r#"
            insert into talk_rooms(document_id, primary_user_id, created_at)
            values (?, ?, default)
            "#,
        )
        .bind(source.id.value.to_string())
        .bind(source.primary_user_id.value())
        .execute(&mut *tx)
        .await
        .expect("Unable to insert a talk rooms");

        let talk_room_table = TalkRoomTable::from(source.clone());
        println!("talk_room_table: {:?}", talk_room_table);
        let talk_room_card_table = TalkRoomCardTable::from(source.clone());
        println!("talk_room_card_table: {:?}", talk_room_card_table);
        let firestore = Arc::clone(&self.firestore.0);
        firestore
            .fluent()
            .insert()
            .into(TALK_ROOM_COLLECTION_NAME)
            .document_id(&document_id)
            .object(&talk_room_table)
            .execute()
            .await
            .map_err(|e| {
                println!("firestore insert error: {}", e);
                anyhow!(RepositoryError::CouldNotInsert(
                    TALK_ROOM_COLLECTION_NAME.to_string(),
                    "document_id".to_string(),
                    document_id.clone(),
                ))
            })?;
        firestore
            .fluent()
            .insert()
            .into(TALK_ROOM_CARD_COLLECTION_NAME)
            .document_id(&document_id)
            .object(&talk_room_card_table)
            .execute()
            .await
            .map_err(|e| {
                println!("firestore insert error: {}", e);
                anyhow!(RepositoryError::CouldNotInsert(
                    TALK_ROOM_CARD_COLLECTION_NAME.to_string(),
                    "document_id".to_string(),
                    document_id.clone(),
                ))
            })?;
        // トランザクションはスコープ外になると自動的にロールバックしてくれるので、firestoreでエラーが起きた場合もDBへの書き込みも削除される
        tx.commit().await.expect("Unable to commit the transaction");

        /*
         * イベントを作成する
         */
        let talk_room = self.create_event(source.clone()).await?;

        Ok(talk_room)
    }

    /// talkRoomをupdateし、イベントを作成する
    ///
    /// # Arguments
    ///
    /// * `source` - 更新するtalkRoom。latest_messageには最新のイベントを入れる
    ///
    async fn create_event(&self, source: NewTalkRoom) -> anyhow::Result<TalkRoom> {
        let firestore = Arc::clone(&self.firestore.0);
        let talk_room_document_id = source.id.value.to_string();
        let talk_room_card_table = TalkRoomCardTable::from(source.clone());
        firestore
            .fluent()
            .update()
            .in_col(TALK_ROOM_CARD_COLLECTION_NAME)
            .document_id(&talk_room_document_id)
            .object(&talk_room_card_table)
            .execute()
            .await?;
        /*
         * イベントを作成する
         */
        let parent_path =
            firestore.parent_path(TALK_ROOM_COLLECTION_NAME, &talk_room_document_id)?;
        let new_event = source.latest_message;
        let event_table = EventTable::from(new_event.clone());
        let event_document_id = new_event.id().value.to_string();
        firestore
            .fluent()
            .insert()
            .into(EVENT_COLLECTION_NAME)
            .document_id(&event_document_id)
            .parent(&parent_path)
            .object(&event_table)
            .execute()
            .await?;

        Ok(TalkRoom::new(
            talk_room_document_id.try_into()?,
            source.primary_user_id,
            talk_room_card_table.display_name,
            talk_room_card_table.rsvp,
            talk_room_card_table.pinned,
            talk_room_card_table.follow,
            event_table.into_event(event_document_id),
            talk_room_card_table.latest_messaged_at,
            talk_room_card_table.sort_time,
            talk_room_card_table.created_at,
            talk_room_card_table.updated_at,
        ))
    }
}
