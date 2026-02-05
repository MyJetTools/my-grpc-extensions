use std::{pin::Pin, sync::Arc, time::Duration};

#[deprecated(note = "Please use StreamedResultWriter")]
pub type GrpcOutputStream<TResult> = StreamedResponseWriter<TResult>;

pub struct StreamedResponseWriter<TResult: Send + Sync + 'static> {
    tx: Arc<tokio::sync::mpsc::Sender<Result<TResult, tonic::Status>>>,
    rx: Option<tokio::sync::mpsc::Receiver<Result<TResult, tonic::Status>>>,
    time_out: Duration,
}

impl<TResult: Send + Sync + 'static> StreamedResponseWriter<TResult> {
    pub fn new(channel_size: usize) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(channel_size);
        Self {
            tx: Arc::new(tx),
            rx: Some(rx),
            time_out: Duration::from_secs(10),
        }
    }

    pub fn set_timeout(mut self, time_out: Duration) -> Self {
        self.time_out = time_out;
        self
    }

    pub async fn send(&self, itm: TResult) {
        self.tx
            .send_timeout(Result::<_, tonic::Status>::Ok(itm), self.time_out)
            .await
            .unwrap();
    }

    pub async fn send_error(&self, err: tonic::Status) {
        self.tx
            .send_timeout(Result::<_, tonic::Status>::Err(err), self.time_out)
            .await
            .unwrap();
    }

    pub fn get_stream_producer(&self) -> StreamedResponseProducer<TResult> {
        StreamedResponseProducer {
            tx: self.tx.clone(),
            time_out: self.time_out,
        }
    }

    pub fn get_result(
        mut self,
    ) -> Result<
        tonic::Response<
            Pin<
                Box<
                    dyn futures_util::Stream<Item = Result<TResult, tonic::Status>>
                        + Send
                        + Sync
                        + 'static,
                >,
            >,
        >,
        tonic::Status,
    >
    where
        TResult: Send + Sync + std::fmt::Debug + 'static,
    {
        let rx = self.rx.take();

        if rx.is_none() {
            panic!("Result is already taken");
        }

        let rx = rx.unwrap();

        let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let response: Pin<
            Box<
                dyn futures_util::Stream<Item = Result<TResult, tonic::Status>>
                    + Send
                    + Sync
                    + 'static,
            >,
        > = Box::pin(output_stream);
        return Ok(tonic::Response::new(response));
    }
}

pub struct StreamedResponseProducer<TResult: Send + Sync + 'static> {
    tx: Arc<tokio::sync::mpsc::Sender<Result<TResult, tonic::Status>>>,
    time_out: Duration,
}

impl<TResult: Send + Sync + 'static> StreamedResponseProducer<TResult> {
    pub async fn send(&self, item: TResult) -> Result<(), String> {
        let result = self
            .tx
            .send_timeout(Result::<_, tonic::Status>::Ok(item), self.time_out)
            .await;

        if let Err(err) = result {
            return Err(format!("{:?}", err));
        }

        Ok(())
    }

    pub async fn send_error(&self, err: tonic::Status) {
        self.tx
            .send_timeout(Result::<_, tonic::Status>::Err(err), self.time_out)
            .await
            .unwrap();
    }
}
