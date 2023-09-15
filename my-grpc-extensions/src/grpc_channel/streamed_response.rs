use std::{collections::HashMap, time::Duration};

use crate::GrpcReadError;

pub struct StreamedResponse<TResponse> {
    stream: tonic::Streaming<TResponse>,
    time_out: Duration,
}

impl<TResponse> StreamedResponse<TResponse> {
    pub fn new(stream: tonic::Streaming<TResponse>, time_out: Duration) -> Self {
        Self { stream, time_out }
    }

    pub async fn as_vec(self) -> Result<Option<Vec<TResponse>>, GrpcReadError> {
        crate::read_grpc_stream::as_vec(self.stream, self.time_out).await
    }

    pub async fn as_vec_with_transformation<TDest>(
        self,
        transform: impl Fn(TResponse) -> TDest,
    ) -> Result<Option<Vec<TDest>>, GrpcReadError> {
        crate::read_grpc_stream::as_vec_with_transformation(self.stream, self.time_out, &transform)
            .await
    }

    pub async fn as_has_map<TKey, TGetKey: Fn(TResponse) -> (TKey, TResponse)>(
        self,
        get_key: TGetKey,
    ) -> Result<Option<HashMap<TKey, TResponse>>, GrpcReadError>
    where
        TKey: std::cmp::Eq + core::hash::Hash + Clone,
    {
        crate::read_grpc_stream::as_hash_map(self.stream, &get_key, self.time_out).await
    }
}
