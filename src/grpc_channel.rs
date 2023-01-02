use std::{collections::HashMap, time::Duration};

use futures::Future;
use tokio::sync::RwLock;
use tonic::transport::Channel;

pub struct GrpcChannel {
    pub channel: RwLock<Option<Channel>>,
    pub timeout: Duration,
    pub grpc_address: String,
}

impl GrpcChannel {
    pub fn new(grpc_address: String, timeout: Duration) -> Self {
        Self {
            channel: RwLock::new(None),
            timeout,
            grpc_address,
        }
    }

    pub async fn get_channel(&self) -> Channel {
        {
            let access = self.channel.read().await;
            if let Some(channel) = access.as_ref() {
                return channel.clone();
            }
        }

        let mut access = self.channel.write().await;
        if let Some(channel) = access.as_ref() {
            return channel.clone();
        }

        let end_point = Channel::from_shared(self.grpc_address.clone()).unwrap();

        let channel = tokio::time::timeout(self.timeout, end_point.connect())
            .await
            .unwrap()
            .unwrap();

        *access = Some(channel.clone());

        channel
    }

    pub async fn execute_with_timeout<TResult, TFuture: Future<Output = TResult>>(
        &self,

        future: TFuture,
    ) -> TResult {
        let result = tokio::time::timeout(self.timeout, future).await;
        match result {
            Ok(result) => return result,
            Err(_) => {
                let mut access = self.channel.write().await;
                *access = None;
                panic!("Grpc {} is Timeouted", self.grpc_address);
            }
        }
    }

    pub async fn execute_stream_as_vec<
        TResult,
        TFuture: Future<Output = tonic::Streaming<TResult>>,
    >(
        &self,
        future: TFuture,
    ) -> Option<Vec<TResult>> {
        let response = self.execute_with_timeout(future).await;

        crate::read_grpc_stream::as_vec(response, self.timeout)
            .await
            .unwrap()
    }

    pub async fn excute_stream_as_hash_map<
        TSrc,
        TKey,
        TValue,
        TFuture: Future<Output = tonic::Streaming<TSrc>>,
        TGetKey: Fn(TSrc) -> (TKey, TValue),
    >(
        &self,
        future: TFuture,
        get_key: TGetKey,
    ) -> Option<HashMap<TKey, TValue>>
    where
        TKey: std::cmp::Eq + core::hash::Hash + Clone,
    {
        let response = self.execute_with_timeout(future).await;

        crate::read_grpc_stream::as_hash_map(response, get_key, self.timeout)
            .await
            .unwrap()
    }
}
