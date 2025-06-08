use std::{collections::HashMap, time::Duration};

use crate::GrpcReadError;

pub struct StreamedResponse<TItem> {
    stream: tonic::Streaming<TItem>,
    time_out: Duration,
}

impl<TResponse> StreamedResponse<TResponse> {
    pub fn new(stream: tonic::Streaming<TResponse>, time_out: Duration) -> Self {
        Self { stream, time_out }
    }

    pub async fn into_vec(self) -> Result<Vec<TResponse>, GrpcReadError> {
        crate::read_grpc_stream::as_vec(self.stream, self.time_out).await
    }

    pub fn set_timeout(&mut self, time_out: Duration) {
        self.time_out = time_out
    }

    pub fn get_timeout(&mut self) -> Duration {
        self.time_out
    }

    pub async fn into_vec_with_transformation<TDest>(
        self,
        transform: impl Fn(TResponse) -> TDest,
    ) -> Result<Vec<TDest>, GrpcReadError> {
        crate::read_grpc_stream::as_vec_with_transformation(self.stream, self.time_out, &transform)
            .await
    }

    pub async fn into_has_map<TKey, TGetKey: Fn(TResponse) -> (TKey, TResponse)>(
        self,
        get_key: TGetKey,
    ) -> Result<HashMap<TKey, TResponse>, GrpcReadError>
    where
        TKey: std::cmp::Eq + core::hash::Hash + Clone,
    {
        crate::read_grpc_stream::as_hash_map(self.stream, &get_key, self.time_out).await
    }

    pub async fn get_next_item(&mut self) -> Result<Option<TResponse>, String> {
        use futures_util::StreamExt;
        let future = self.stream.next();

        let result = match tokio::time::timeout(self.time_out, future).await {
            Ok(result) => result,
            Err(_) => return Err(format!("Timeout {:?}", self.time_out)),
        };

        let Some(result) = result else {
            return Ok(None);
        };

        match result {
            Ok(result) => Ok(Some(result)),
            Err(err) => Err(err.to_string()),
        }
    }
}
