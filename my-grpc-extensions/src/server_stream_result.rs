use std::{pin::Pin, time::Duration};

use tokio::sync::mpsc::Sender;

use crate::grpc_server::SendStream;

pub struct GrpcServerStreamResult<TModel: Send + Sync + 'static> {
    tx: Sender<Result<TModel, tonic::Status>>,
    timeout: std::time::Duration,
}

impl<TModel: Send + Sync + 'static> GrpcServerStreamResult<TModel> {
    pub fn new() -> (Self, SendStream<TModel>) {
        let (tx, rx) = tokio::sync::mpsc::channel(32768);

        let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let response: Pin<
            Box<dyn futures::Stream<Item = Result<TModel, tonic::Status>> + Send + Sync + 'static>,
        > = Box::pin(output_stream);

        let result = tonic::Response::new(response);
        (
            Self {
                tx,
                timeout: Duration::from_secs(3),
            },
            result,
        )
    }

    pub fn set_timeout(&mut self, timeout: std::time::Duration) {
        self.timeout = timeout;
    }

    pub async fn send(&mut self, item: TModel) {
        let future = self.tx.send(Ok(item));
        tokio::time::timeout(self.timeout, future)
            .await
            .unwrap()
            .unwrap();
    }
}
