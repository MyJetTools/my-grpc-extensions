use crate::{
    GrpcReadError, RentedChannel, RequestBuilderWithInputStreamWithRetries,
    RequestWithInputAsStreamGrpcExecutor, RequestWithInputAsStreamWithResponseAsStreamGrpcExecutor,
    StreamedResponse,
};

pub struct RequestBuilderWithInputStream<
    TService: Send + Sync + 'static,
    TRequest: Clone + Send + Sync + 'static,
> {
    input_contract: Vec<TRequest>,
    channel: RentedChannel<TService>,
}

impl<TService: Send + Sync + 'static, TRequest: Clone + Send + Sync + 'static>
    RequestBuilderWithInputStream<TService, TRequest>
{
    pub fn new(input_contract: Vec<TRequest>, channel: RentedChannel<TService>) -> Self {
        Self {
            input_contract,
            channel,
        }
    }

    pub fn with_retries(
        self,
        attempts_amount: usize,
    ) -> RequestBuilderWithInputStreamWithRetries<TService, TRequest> {
        RequestBuilderWithInputStreamWithRetries::new(
            self.input_contract,
            self.channel,
            attempts_amount,
        )
    }

    pub async fn get_response<
        TResponse,
        TExecutor: RequestWithInputAsStreamGrpcExecutor<TService, TRequest, TResponse> + Send + Sync + 'static,
    >(
        mut self,
        grpc_executor: &TExecutor,
    ) -> Result<TResponse, GrpcReadError>
    where
        TResponse: Send + Sync + 'static,
    {
        self.channel
            .execute_input_as_stream(self.input_contract, grpc_executor)
            .await
    }

    pub async fn get_streamed_response<
        TResponse,
        TExecutor: RequestWithInputAsStreamWithResponseAsStreamGrpcExecutor<TService, TRequest, TResponse>
            + Send
            + Sync
            + 'static,
    >(
        mut self,
        grpc_executor: &TExecutor,
    ) -> Result<StreamedResponse<TResponse>, GrpcReadError>
    where
        TResponse: Send + Sync + 'static,
    {
        let stream_to_read = self
            .channel
            .execute_input_as_stream_response_as_stream(self.input_contract, grpc_executor)
            .await?;

        Ok(StreamedResponse::new(stream_to_read, self.channel.timeout))
    }
}
