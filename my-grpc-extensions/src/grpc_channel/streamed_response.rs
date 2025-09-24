use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use crate::GrpcReadError;

pub struct StreamedResponse<TItem> {
    stream: tonic::Streaming<TItem>,
    time_out: Duration,
}

impl<TItem> StreamedResponse<TItem> {
    pub fn new(stream: tonic::Streaming<TItem>, time_out: Duration) -> Self {
        Self { stream, time_out }
    }

    pub async fn into_vec<TResult: From<TItem>>(self) -> Result<Vec<TResult>, GrpcReadError> {
        crate::read_grpc_stream::as_vec(self.stream, self.time_out).await
    }

    pub fn set_timeout(&mut self, time_out: Duration) {
        self.time_out = time_out
    }

    pub fn get_timeout(&mut self) -> Duration {
        self.time_out
    }

    #[deprecated(note = "Please use into_vec and trait From to convert items")]
    pub async fn into_vec_with_transformation<TDest>(
        self,
        transform: impl Fn(TItem) -> TDest,
    ) -> Result<Vec<TDest>, GrpcReadError> {
        crate::read_grpc_stream::as_vec_with_transformation(self.stream, self.time_out, &transform)
            .await
    }

    pub async fn into_has_map<TResult, TKey>(
        self,
        get_key: impl Fn(TItem) -> (TKey, TResult),
    ) -> Result<HashMap<TKey, TResult>, GrpcReadError>
    where
        TKey: std::cmp::Eq + core::hash::Hash + Clone,
    {
        crate::read_grpc_stream::as_hash_map(self.stream, get_key, self.time_out).await
    }

    pub async fn into_b_tree_map<TResult, TKey>(
        self,
        get_key: impl Fn(TItem) -> (TKey, TResult),
    ) -> Result<BTreeMap<TKey, TResult>, GrpcReadError>
    where
        TKey: Ord + core::hash::Hash + Clone,
    {
        crate::read_grpc_stream::as_b_tree_map(self.stream, get_key, self.time_out).await
    }

    pub async fn get_next_item(&mut self) -> Option<tonic::Result<TItem>> {
        use futures_util::StreamExt;
        let future = self.stream.next();

        let result = match tokio::time::timeout(self.time_out, future).await {
            Ok(result) => result,
            Err(_) => {
                return Some(Err(tonic::Status::aborted(format!(
                    "Timeout {:?}",
                    self.time_out
                ))))
            }
        }?;

        Some(result)
    }
}
