use crate::{
    GrpcReadError, RentedChannel, RequestResponseGrpcExecutor,
    RequestWithResponseAsStreamGrpcExecutor, StreamedResponse,
};

pub struct RequestBuilderWithRetries<
    TService: Send + Sync + 'static,
    TRequest: Clone + Send + Sync + 'static,
> {
    input_contract: TRequest,
    channel: RentedChannel<TService>,
    max_attempts_amount: usize,
}

impl<TService: Send + Sync + 'static, TRequest: Clone + Send + Sync + 'static>
    RequestBuilderWithRetries<TService, TRequest>
{
    pub fn new(
        input_contract: TRequest,
        channel: RentedChannel<TService>,
        max_attempts_amount: usize,
    ) -> Self {
        Self {
            input_contract,
            channel,
            max_attempts_amount,
        }
    }

    pub async fn get_response<
        TResponse,
        TExecutor: RequestResponseGrpcExecutor<TService, TRequest, TResponse> + Send + Sync + 'static,
    >(
        mut self,
        grpc_executor: &TExecutor,
    ) -> Result<TResponse, GrpcReadError>
    where
        TResponse: Send + Sync + 'static,
    {
        let mut attempt_no = 0;
        loop {
            let result = self
                .channel
                .execute(self.input_contract.clone(), grpc_executor)
                .await;

            match result {
                Ok(response) => return Ok(response),
                Err(err) => {
                    self.channel
                        .handle_error(err, &mut attempt_no, self.max_attempts_amount)
                        .await?;
                }
            }

            attempt_no += 1
        }
    }

    pub async fn get_streamed_response<
        TResponse,
        TExecutor: RequestWithResponseAsStreamGrpcExecutor<TService, TRequest, TResponse>
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
        let mut attempt_no = 0;
        loop {
            let result = self
                .channel
                .execute_with_response_as_stream(self.input_contract.clone(), grpc_executor)
                .await;

            match result {
                Ok(stream_to_read) => {
                    return Ok(StreamedResponse::new(stream_to_read, self.channel.timeout));
                }
                Err(err) => {
                    self.channel
                        .handle_error(err, &mut attempt_no, self.max_attempts_amount)
                        .await?;
                }
            }

            attempt_no += 1
        }
    }
}
