#[cfg(feature = "with-telemetry")]
use my_telemetry::MyTelemetryContext;
use std::{sync::Arc, time::Duration};

use tonic::transport::Channel;

use crate::{
    GrpcChannelPoolInner, GrpcClientSettings, GrpcReadError, GrpcServiceFactory, RequestBuilder,
    RequestBuilderWithInputStream,
};

pub struct GrpcChannel<TService: Send + Sync + 'static> {
    channel_pool: Arc<GrpcChannelPoolInner>,
    pub request_timeout: Duration,
    service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
    get_grpc_address: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
    #[cfg(feature = "with-telemetry")]
    ctx: MyTelemetryContext,
}

impl<TService: Send + Sync + 'static> GrpcChannel<TService> {
    pub fn new(
        channel_pool: Arc<GrpcChannelPoolInner>,
        request_timeout: Duration,
        service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
        get_grpc_address: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
        #[cfg(feature = "with-telemetry")] ctx: MyTelemetryContext,
    ) -> Self {
        Self {
            channel_pool,
            request_timeout,
            service_factory,
            get_grpc_address,
            #[cfg(feature = "with-telemetry")]
            ctx,
        }
    }

    pub async fn get_connect_url(&self) -> String {
        self.get_grpc_address
            .get_grpc_url(self.service_factory.get_service_name())
            .await
    }

    pub async fn get_channel(&self) -> Result<Channel, GrpcReadError> {
        if let Some(channel) = self.channel_pool.rent().await {
            return Ok(channel);
        }

        let connect_url = self.get_connect_url().await;
        let service_name = self.service_factory.get_service_name();

        self.channel_pool
            .create_channel(connect_url, service_name, self.request_timeout)
            .await
    }

    pub async fn drop_dead_channel(&self, err: String) {
        self.channel_pool.disconnect_channel(err).await;
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
            self.drop_dead_channel(format!("{:?}", err)).await;
        }

        remove
    }

    pub async fn get_service(
        &self,
        #[cfg(feature = "with-telemetry")] ctx: &MyTelemetryContext,
    ) -> Result<TService, GrpcReadError> {
        let channel = self.get_channel().await?;
        let result = self.service_factory.create_service(
            channel,
            #[cfg(feature = "with-telemetry")]
            ctx,
        );

        Ok(result)
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
        let service = self
            .get_service(
                #[cfg(feature = "with-telemetry")]
                &self.ctx,
            )
            .await?;

        let future = grpc_executor.execute(service, request_data);

        let result = tokio::time::timeout(self.request_timeout, future).await;

        if result.is_err() {
            self.drop_dead_channel("Timeout".to_string()).await;
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
        let service = self
            .get_service(
                #[cfg(feature = "with-telemetry")]
                &self.ctx,
            )
            .await?;

        let future = grpc_executor.execute(service, request_data);

        let result = tokio::time::timeout(self.request_timeout, future).await;

        if result.is_err() {
            self.drop_dead_channel("Timeout".to_string()).await;
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
        let service = self
            .get_service(
                #[cfg(feature = "with-telemetry")]
                &self.ctx,
            )
            .await?;

        let future = grpc_executor.execute(service, request_data);

        let result = tokio::time::timeout(self.request_timeout, future).await;

        if result.is_err() {
            self.drop_dead_channel("Timeout".to_string()).await;
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
        let service = self
            .get_service(
                #[cfg(feature = "with-telemetry")]
                &self.ctx,
            )
            .await?;

        let future = grpc_executor.execute(service, request_data);

        let result = tokio::time::timeout(self.request_timeout, future).await;

        if result.is_err() {
            self.drop_dead_channel("Timeout".to_string()).await;
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
