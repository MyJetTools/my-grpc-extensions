use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use futures::Future;
use my_telemetry::MyTelemetryContext;
use tokio::sync::Mutex;
use tonic::transport::Channel;

use crate::{
    GrpcChannelPool, GrpcReadError, GrpcServiceFactory, RequestBuilderWithInputAsStruct,
    RequestBuilderWithInputAsStructWithRetries,
};

pub struct RentedChannel<TService: Send + Sync + 'static> {
    channel: Option<Channel>,
    channel_pool: Arc<Mutex<GrpcChannelPool>>,
    channel_is_alive: AtomicBool,
    timeout: Duration,
    service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
    ctx: MyTelemetryContext,
}

impl<TService: Send + Sync + 'static> RentedChannel<TService> {
    pub fn new(
        channel: Channel,
        channel_pool: Arc<Mutex<GrpcChannelPool>>,
        timeout: Duration,
        service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
        ctx: MyTelemetryContext,
    ) -> Self {
        Self {
            channel: Some(channel),
            channel_pool,
            channel_is_alive: AtomicBool::new(true),
            timeout,
            service_factory,
            ctx,
        }
    }

    pub fn get_channel(&self) -> Channel {
        self.channel.as_ref().unwrap().clone()
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

    pub async fn drop_channel_if_needed(&self, err: &GrpcReadError) -> bool {
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

    pub fn get_service(&self, ctx: &MyTelemetryContext) -> TService {
        let channel = self.get_channel();
        self.service_factory.create_service(channel, ctx)
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

    pub fn start_request<TInputContract: Send + Sync + 'static>(
        self,
        input_contract: TInputContract,
    ) -> RequestBuilderWithInputAsStruct<TService, TInputContract> {
        RequestBuilderWithInputAsStruct::new(input_contract, self)
    }

    pub fn start_request_with_retries<TInputContract: Clone + Send + Sync + 'static>(
        self,
        input_contract: TInputContract,
        max_attempts_amount: usize,
    ) -> RequestBuilderWithInputAsStructWithRetries<TService, TInputContract> {
        RequestBuilderWithInputAsStructWithRetries::new(input_contract, self, max_attempts_amount)
    }

    pub async fn execute_with_timeout_2<
        TRequest: Send + Sync + 'static,
        TResponse: Send + Sync + 'static,
        TExecutor: RequestResponseGrpcExecutor<TService, TRequest, TResponse> + Send + Sync + 'static,
    >(
        &mut self,
        request_data: TRequest,
        grpc_executor: &TExecutor,
    ) -> Result<TResponse, GrpcReadError> {
        let service = self.get_service(&self.ctx);

        let future = grpc_executor.execute(service, request_data);

        let result = tokio::time::timeout(self.timeout, future).await;

        if result.is_err() {
            self.mark_channel_is_dead();
            return Err(GrpcReadError::Timeout);
        }

        let result = result.unwrap();

        match result {
            Ok(result) => Ok(result),
            Err(err) => {
                self.drop_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }
}

impl<TService: Send + Sync + 'static> Drop for RentedChannel<TService> {
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

#[async_trait::async_trait]
pub trait RequestResponseGrpcExecutor<
    TService: Send + Sync + 'static,
    TRequest: Send + Sync + 'static,
    TResponse: Send + Sync + 'static,
>
{
    async fn execute(
        &self,
        service: TService,
        input_data: TRequest,
    ) -> Result<TResponse, GrpcReadError>;
}
