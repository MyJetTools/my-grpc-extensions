use std::pin::Pin;

use tokio::sync::mpsc::Sender;

use crate::grpc_server::SendStream;

pub struct GrpcServerStreamResult<TModel: Send + Sync + 'static> {
    tx: Sender<Result<TModel, tonic::Status>>,
}

impl<TModel: Send + Sync + 'static> GrpcServerStreamResult<TModel> {
    pub fn new() -> (Self, SendStream<TModel>) {
        let (tx, rx) = tokio::sync::mpsc::channel(32768);

        let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let response: Pin<
            Box<dyn futures::Stream<Item = Result<TModel, tonic::Status>> + Send + Sync + 'static>,
        > = Box::pin(output_stream);

        let result = tonic::Response::new(response);
        (Self { tx }, result)
    }

    pub async fn send_item(&mut self, item: TModel) {
        self.tx.send(Ok(item)).await.unwrap()
    }
}
