use std::{collections::HashMap, time::Duration};

use futures::Future;
use tokio::{sync::RwLock, time::error::Elapsed};
use tonic::transport::Channel;

#[derive(Debug)]
pub enum GrpcReadError {
    Timeout,
    TransportError(tonic::transport::Error),
    TonicStatus(tonic::Status),
}

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

    pub async fn get_channel(&self) -> Result<Channel, GrpcReadError> {
        {
            let access = self.channel.read().await;
            if let Some(channel) = access.as_ref() {
                return Ok(channel.clone());
            }
        }

        let mut access = self.channel.write().await;

        if let Some(channel) = access.as_ref() {
            return Ok(channel.clone());
        }

        let end_point = Channel::from_shared(self.grpc_address.clone()).unwrap();

        let channel = tokio::time::timeout(self.timeout, end_point.connect()).await?;

        let channel = channel?;
        *access = Some(channel.clone());

        Ok(channel)
    }

    pub async fn execute_with_timeout<
        TResult,
        TFuture: Future<Output = Result<TResult, tonic::Status>>,
    >(
        &self,
        future: TFuture,
    ) -> Result<TResult, GrpcReadError> {
        let result = tokio::time::timeout(self.timeout, future).await;

        if result.is_err() {
            let mut access = self.channel.write().await;
            *access = None;
            return Err(GrpcReadError::Timeout);
        }

        let result = result.unwrap();

        match result {
            Ok(result) => Ok(result),
            Err(err) => {
                let err = GrpcReadError::TonicStatus(err);
                self.remove_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }

    pub async fn execute_with_retries<
        TResult,
        TFutureResult: Future<Output = Result<TResult, tonic::Status>>,
        TGetFuture: Fn() -> TFutureResult,
    >(
        &self,
        get_future: TGetFuture,
        max_attempts_amount: usize,
    ) -> Result<TResult, GrpcReadError> {
        let mut attempt_no = 0;
        loop {
            let future = get_future();

            match self.execute_with_timeout(future).await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    self.handle_timeout_error(err, &mut attempt_no, max_attempts_amount)
                        .await?;
                }
            }
        }
    }

    pub async fn execute_stream_as_vec<
        TResult,
        TFuture: Future<Output = Result<tonic::Response<tonic::Streaming<TResult>>, tonic::Status>>,
    >(
        &self,
        future: TFuture,
    ) -> Result<Option<Vec<TResult>>, GrpcReadError> {
        let response = self.execute_with_timeout(future).await?;

        let result = crate::read_grpc_stream::as_vec(response.into_inner(), self.timeout).await;

        match result {
            Ok(result) => Ok(result),
            Err(err) => {
                self.remove_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }

    pub async fn execute_stream_as_vec_with_retries<
        TResult,
        TFuture: Future<Output = Result<tonic::Response<tonic::Streaming<TResult>>, tonic::Status>>,
        TGetFuture: Fn() -> TFuture,
    >(
        &self,
        get_future: TGetFuture,
        max_attempts_amount: usize,
    ) -> Result<Option<Vec<TResult>>, GrpcReadError> {
        let mut attempt_no = 0;
        loop {
            let future = get_future();
            let response = self.execute_with_timeout(future).await;

            match response {
                Ok(response) => {
                    let result =
                        crate::read_grpc_stream::as_vec(response.into_inner(), self.timeout).await;

                    match result {
                        Ok(result) => return Ok(result),
                        Err(err) => {
                            self.handle_timeout_error(err, &mut attempt_no, max_attempts_amount)
                                .await?;
                        }
                    }
                }
                Err(err) => {
                    self.handle_timeout_error(err, &mut attempt_no, max_attempts_amount)
                        .await?;
                }
            }
        }
    }

    pub async fn execute_stream_as_vec_with_transformation_and_retries<
        TResult,
        TOut,
        TFuture: Future<Output = Result<tonic::Response<tonic::Streaming<TResult>>, tonic::Status>>,
        TGetFuture: Fn() -> TFuture,
        TTransform: Fn(TResult) -> TOut,
    >(
        &self,
        get_future: TGetFuture,
        max_attempts_amount: usize,
        transform: &TTransform,
    ) -> Result<Option<Vec<TOut>>, GrpcReadError> {
        let mut attempt_no = 0;
        loop {
            let future = get_future();
            let response = self.execute_with_timeout(future).await;

            match response {
                Ok(response) => {
                    let result = crate::read_grpc_stream::as_vec_with_transformation(
                        response.into_inner(),
                        self.timeout,
                        transform,
                    )
                    .await;

                    match result {
                        Ok(result) => return Ok(result),
                        Err(err) => {
                            self.handle_timeout_error(err, &mut attempt_no, max_attempts_amount)
                                .await?;
                        }
                    }
                }
                Err(err) => {
                    self.handle_timeout_error(err, &mut attempt_no, max_attempts_amount)
                        .await?;
                }
            }
        }
    }

    pub async fn excute_stream_as_hash_map<
        TSrc,
        TKey,
        TValue,
        TFuture: Future<Output = Result<tonic::Response<tonic::Streaming<TSrc>>, tonic::Status>>,
        TGetKey: Fn(TSrc) -> (TKey, TValue),
    >(
        &self,
        future: TFuture,
        get_key: TGetKey,
    ) -> Result<Option<HashMap<TKey, TValue>>, GrpcReadError>
    where
        TKey: std::cmp::Eq + core::hash::Hash + Clone,
    {
        let response = self.execute_with_timeout(future).await?;

        let result =
            crate::read_grpc_stream::as_hash_map(response.into_inner(), &get_key, self.timeout)
                .await;

        match result {
            Ok(result) => Ok(result),
            Err(err) => {
                self.remove_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }

    pub async fn excute_stream_as_hash_map_with_retries<
        TSrc,
        TKey,
        TValue,
        TFuture: Future<Output = Result<tonic::Response<tonic::Streaming<TSrc>>, tonic::Status>>,
        TGetFuture: Fn() -> TFuture,
        TGetKey: Fn(TSrc) -> (TKey, TValue),
    >(
        &self,
        get_future: TGetFuture,
        max_attempts_amount: usize,
        get_key: TGetKey,
    ) -> Result<Option<HashMap<TKey, TValue>>, GrpcReadError>
    where
        TKey: std::cmp::Eq + core::hash::Hash + Clone,
    {
        let mut attempt_no = 0;
        loop {
            let future = get_future();
            let response = self.execute_with_timeout(future).await;

            match response {
                Ok(response) => {
                    let result = crate::read_grpc_stream::as_hash_map(
                        response.into_inner(),
                        &get_key,
                        self.timeout,
                    )
                    .await;

                    match result {
                        Ok(result) => return Ok(result),
                        Err(err) => {
                            self.handle_timeout_error(err, &mut attempt_no, max_attempts_amount)
                                .await?;
                        }
                    }
                }
                Err(err) => {
                    self.handle_timeout_error(err, &mut attempt_no, max_attempts_amount)
                        .await?;
                }
            }
        }
    }

    async fn remove_channel_if_needed(&self, err: &GrpcReadError) -> bool {
        let remove = match err {
            GrpcReadError::TonicStatus(status) => {
                let code = status.code();

                if code == tonic::Code::Unknown {
                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        if remove {
            let mut access = self.channel.write().await;
            *access = None;
        }

        remove
    }

    async fn handle_timeout_error(
        &self,
        err: GrpcReadError,
        attempt_no: &mut usize,
        max_attempts_amount: usize,
    ) -> Result<(), GrpcReadError> {
        let channel_removed = self.remove_channel_if_needed(&err).await;
        if *attempt_no >= max_attempts_amount {
            return Err(err);
        }

        *attempt_no += 1;

        if channel_removed {
            Ok(())
        } else {
            Err(err)
        }
    }
}

impl From<Elapsed> for GrpcReadError {
    fn from(_: Elapsed) -> Self {
        Self::Timeout
    }
}

impl From<tonic::Status> for GrpcReadError {
    fn from(value: tonic::Status) -> Self {
        Self::TonicStatus(value)
    }
}

impl From<tonic::transport::Error> for GrpcReadError {
    fn from(value: tonic::transport::Error) -> Self {
        Self::TransportError(value)
    }
}
