use crate::{GrpcReadError, RentedChannel, RequestResponseGrpcExecutor};

pub struct RequestBuilderWithInputAsStructWithRetries<
    TService: Send + Sync + 'static,
    TRequest: Clone + Send + Sync + 'static,
> {
    input_contract: TRequest,
    channel: RentedChannel<TService>,
    max_attempts_amount: usize,
}

impl<TService: Send + Sync + 'static, TRequest: Clone + Send + Sync + 'static>
    RequestBuilderWithInputAsStructWithRetries<TService, TRequest>
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

    pub async fn execute<
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
                .execute_with_timeout_2(self.input_contract.clone(), grpc_executor)
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
}
