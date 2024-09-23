use async_trait::async_trait;
use axum::{
    extract::{rejection::FormRejection, Form, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};

use serde::de::DeserializeOwned;
use validator::Validate;

use crate::context::{errors::ServerError, validate::ValidatedRequest};

#[async_trait]
impl<T, S> FromRequestParts<S> for ValidatedRequest<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
    Form<T>: FromRequestParts<S, Rejection = FormRejection>,
{
    type Rejection = ServerError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Form(value) = Form::<T>::from_request_parts(parts, state).await?;
        value.validate()?;
        Ok(ValidatedRequest(value))
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        match self {
            ServerError::ValidationError(_) => {
                let message = format!("Input validation error: [{}]", self).replace('\n', ", ");
                (StatusCode::BAD_REQUEST, message)
            }
            ServerError::JsonRejection(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ServerError::AxumFormRejection(_) => (StatusCode::BAD_REQUEST, self.to_string()),
        }
        .into_response()
    }
}
