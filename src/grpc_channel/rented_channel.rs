use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use futures::Future;
use my_telemetry::MyTelemetryContext;
use tokio::sync::Mutex;
use tonic::{codegen::InterceptedService, transport::Channel};

use crate::{GrpcChannelPool, GrpcClientInterceptor, GrpcReadError};

pub struct RentedChannel<TService> {
    channel: Option<Channel>,
    channel_pool: Arc<Mutex<GrpcChannelPool>>,
    channel_is_alive: AtomicBool,
    timeout: Duration,
    service: Option<TService>,
}

impl<TService> RentedChannel<TService> {
    pub fn new(
        channel: Channel,
        channel_pool: Arc<Mutex<GrpcChannelPool>>,
        timeout: Duration,
    ) -> Self {
        Self {
            channel: Some(channel),
            channel_pool,
            channel_is_alive: AtomicBool::new(true),
            timeout,
            service: None,
        }
    }

    pub fn get_channel(&self) -> Channel {
        self.channel.as_ref().unwrap().clone()
    }

    pub fn assign_service(&mut self, service: TService) {
        self.service = Some(service);
    }

    pub fn mark_channel_is_dead(&self) {
        self.channel_is_alive
            .store(false, std::sync::atomic::Ordering::Relaxed);
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
            self.mark_channel_is_dead();
            return Err(GrpcReadError::Timeout);
        }

        let result = result.unwrap();

        match result {
            Ok(result) => Ok(result),
            Err(err) => {
                let err = GrpcReadError::TonicStatus(err);
                self.drop_channel_if_needed(&err).await;
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
                    self.handle_error(err, &mut attempt_no, max_attempts_amount)
                        .await?;
                }
            }
        }
    }

    async fn drop_channel_if_needed(&self, err: &GrpcReadError) -> bool {
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
            self.mark_channel_is_dead();
        }

        remove
    }

    pub async fn handle_error(
        &self,
        err: GrpcReadError,
        attempt_no: &mut usize,
        max_attempts_amount: usize,
    ) -> Result<(), GrpcReadError> {
        let channel_removed = self.drop_channel_if_needed(&err).await;
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
                self.drop_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }

    pub async fn execute_stream_as_vec_with_transformation<
        TResult,
        TOut,
        TFuture: Future<Output = Result<tonic::Response<tonic::Streaming<TResult>>, tonic::Status>>,
        TTransform: Fn(TResult) -> TOut,
    >(
        &self,
        future: TFuture,
        transform: TTransform,
    ) -> Result<Option<Vec<TOut>>, GrpcReadError> {
        let response = self.execute_with_timeout(future).await?;

        let result = crate::read_grpc_stream::as_vec_with_transformation(
            response.into_inner(),
            self.timeout,
            &transform,
        )
        .await;

        match result {
            Ok(result) => Ok(result),
            Err(err) => {
                self.drop_channel_if_needed(&err).await;
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
                            self.handle_error(err, &mut attempt_no, max_attempts_amount)
                                .await?;
                        }
                    }
                }
                Err(err) => {
                    self.handle_error(err, &mut attempt_no, max_attempts_amount)
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
        get_future: &TGetFuture,
        max_attempts_amount: usize,
        transform: TTransform,
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
                        &transform,
                    )
                    .await;

                    match result {
                        Ok(result) => return Ok(result),
                        Err(err) => {
                            self.handle_error(err, &mut attempt_no, max_attempts_amount)
                                .await?;
                        }
                    }
                }
                Err(err) => {
                    self.handle_error(err, &mut attempt_no, max_attempts_amount)
                        .await?;
                }
            }
        }
    }

    pub async fn execute_stream_as_hash_map<
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
                self.drop_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }

    pub async fn execute_stream_as_hash_map_with_retries<
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
                            self.handle_error(err, &mut attempt_no, max_attempts_amount)
                                .await?;
                        }
                    }
                }
                Err(err) => {
                    self.handle_error(err, &mut attempt_no, max_attempts_amount)
                        .await?;
                }
            }
        }
    }
}

impl<TService> AsRef<TService> for RentedChannel<TService> {
    fn as_ref(&self) -> &TService {
        self.service.as_ref().unwrap()
    }
}

impl<TService> Drop for RentedChannel<TService> {
    fn drop(&mut self) {
        let channel_is_alive = self
            .channel_is_alive
            .load(std::sync::atomic::Ordering::Relaxed);

        if channel_is_alive {
            if let Some(channel) = self.channel.take() {
                let channel_pool = self.channel_pool.clone();
                tokio::spawn(async move {
                    let mut write_access = channel_pool.lock().await;
                    write_access.push_channel_back(channel);
                });
            }
        }
    }
}
