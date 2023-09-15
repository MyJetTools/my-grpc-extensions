use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use my_telemetry::MyTelemetryContext;

use tonic::transport::Channel;

use crate::{GrpcReadError, GrpcServiceFactory, RequestBuilder, RequestBuilderWithInputStream};

pub struct RentedChannel<TService: Send + Sync + 'static> {
    channel: Option<Channel>,
    channel_is_alive: AtomicBool,
    pub timeout: Duration,
    service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
    ctx: MyTelemetryContext,
}

impl<TService: Send + Sync + 'static> RentedChannel<TService> {
    pub fn new(
        channel: Channel,
        timeout: Duration,
        service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
        ctx: MyTelemetryContext,
    ) -> Self {
        Self {
            channel: Some(channel),
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

    pub fn start_request<TInputContract: Clone + Send + Sync + 'static>(
        self,
        input_contract: TInputContract,
    ) -> RequestBuilder<TService, TInputContract> {
        RequestBuilder::new(input_contract, self)
    }

    pub fn start_request_with_input_prams_as_stream<
        TInputContract: Clone + Send + Sync + 'static,
    >(
        self,
        input_contract: Vec<TInputContract>,
    ) -> RequestBuilderWithInputStream<TService, TInputContract> {
        RequestBuilderWithInputStream::new(input_contract, self)
    }

    pub async fn execute<
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
                let err = err.into();
                self.drop_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }

    pub async fn execute_with_response_as_stream<
        TRequest: Send + Sync + 'static,
        TResponse: Send + Sync + 'static,
        TExecutor: RequestWithResponseAsStreamGrpcExecutor<TService, TRequest, TResponse>
            + Send
            + Sync
            + 'static,
    >(
        &mut self,
        request_data: TRequest,
        grpc_executor: &TExecutor,
    ) -> Result<tonic::Streaming<TResponse>, GrpcReadError> {
        let service = self.get_service(&self.ctx);

        let future = grpc_executor.execute(service, request_data);

        let result = tokio::time::timeout(self.timeout, future).await;

        if result.is_err() {
            self.mark_channel_is_dead();
            return Err(GrpcReadError::Timeout);
        }

        let result = result.unwrap();

        match result {
            Ok(response) => {
                return Ok(response);
            }
            Err(err) => {
                let err = err.into();
                self.drop_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }

    pub async fn execute_input_as_stream<
        TRequest: Send + Sync + 'static,
        TResponse: Send + Sync + 'static,
        TExecutor: RequestWithInputAsStreamGrpcExecutor<TService, TRequest, TResponse> + Send + Sync + 'static,
    >(
        &mut self,
        request_data: Vec<TRequest>,
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
                let err = err.into();
                self.drop_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }

    pub async fn execute_input_as_stream_response_as_stream<
        TRequest: Send + Sync + 'static,
        TResponse: Send + Sync + 'static,
        TExecutor: RequestWithInputAsStreamWithResponseAsStreamGrpcExecutor<TService, TRequest, TResponse>
            + Send
            + Sync
            + 'static,
    >(
        &mut self,
        request_data: Vec<TRequest>,
        grpc_executor: &TExecutor,
    ) -> Result<tonic::Streaming<TResponse>, GrpcReadError> {
        let service = self.get_service(&self.ctx);

        let future = grpc_executor.execute(service, request_data);

        let result = tokio::time::timeout(self.timeout, future).await;

        if result.is_err() {
            self.mark_channel_is_dead();
            return Err(GrpcReadError::Timeout);
        }

        let result = result.unwrap();

        match result {
            Ok(response) => {
                return Ok(response);
            }
            Err(err) => {
                let err = err.into();
                self.drop_channel_if_needed(&err).await;
                Err(err)
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
    ) -> Result<TResponse, tonic::Status>;
}

#[async_trait::async_trait]
pub trait RequestWithResponseAsStreamGrpcExecutor<
    TService: Send + Sync + 'static,
    TRequest: Send + Sync + 'static,
    TResponse: Send + Sync + 'static,
>
{
    async fn execute(
        &self,
        service: TService,
        input_data: TRequest,
    ) -> Result<tonic::Streaming<TResponse>, tonic::Status>;
}

#[async_trait::async_trait]
pub trait RequestWithInputAsStreamGrpcExecutor<
    TService: Send + Sync + 'static,
    TRequest: Send + Sync + 'static,
    TResponse: Send + Sync + 'static,
>
{
    async fn execute(
        &self,
        service: TService,
        input_data: Vec<TRequest>,
    ) -> Result<TResponse, tonic::Status>;
}

#[async_trait::async_trait]
pub trait RequestWithInputAsStreamWithResponseAsStreamGrpcExecutor<
    TService: Send + Sync + 'static,
    TRequest: Send + Sync + 'static,
    TResponse: Send + Sync + 'static,
>
{
    async fn execute(
        &self,
        service: TService,
        input_data: Vec<TRequest>,
    ) -> Result<tonic::Streaming<TResponse>, tonic::Status>;
}
