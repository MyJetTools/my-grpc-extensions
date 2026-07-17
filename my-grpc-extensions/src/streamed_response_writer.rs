use std::{
    fmt,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::sync::mpsc::error::SendTimeoutError;

#[deprecated(note = "Please use StreamedResponseWriter")]
pub type GrpcOutputStream<TResult> = StreamedResponseWriter<TResult>;

pub const DEFAULT_SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Error of pushing a value into the response stream. Both cases hand the undelivered value back.
pub enum StreamedSendError<TItem> {
    /// Channel had no free slot for longer than the send timeout. The consumer is alive, just slow:
    /// the stream stays open and the send may be retried.
    Timeout(TItem),
    /// Receiving half is gone - consumer dropped the stream. Nothing can be delivered anymore.
    Closed(TItem),
}

impl<TItem> StreamedSendError<TItem> {
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout(_))
    }

    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed(_))
    }

    pub fn into_inner(self) -> TItem {
        match self {
            Self::Timeout(itm) => itm,
            Self::Closed(itm) => itm,
        }
    }
}

// Debug/Display are implemented without a TItem: Debug bound - same as tokio does for
// SendTimeoutError - so that `send(..).await.unwrap()` keeps compiling for any payload.
impl<TItem> fmt::Debug for StreamedSendError<TItem> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout(_) => f.write_str("StreamedSendError::Timeout(..)"),
            Self::Closed(_) => f.write_str("StreamedSendError::Closed(..)"),
        }
    }
}

impl<TItem> fmt::Display for StreamedSendError<TItem> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout(_) => {
                f.write_str("Timed out waiting for a free slot in the response stream")
            }
            Self::Closed(_) => f.write_str("Response stream is closed by the consumer"),
        }
    }
}

impl<TItem> std::error::Error for StreamedSendError<TItem> {}

// send() pushes Ok(..) into the channel only, so the undelivered payload is always the item itself.
fn to_item_send_error<TResult>(
    err: SendTimeoutError<Result<TResult, tonic::Status>>,
) -> StreamedSendError<TResult> {
    match err {
        SendTimeoutError::Timeout(Ok(itm)) => StreamedSendError::Timeout(itm),
        SendTimeoutError::Closed(Ok(itm)) => StreamedSendError::Closed(itm),
        SendTimeoutError::Timeout(Err(_)) | SendTimeoutError::Closed(Err(_)) => {
            unreachable!("Item send got tonic::Status back as an undelivered payload")
        }
    }
}

// send_error() pushes Err(..) into the channel only.
fn to_status_send_error<TResult>(
    err: SendTimeoutError<Result<TResult, tonic::Status>>,
) -> StreamedSendError<tonic::Status> {
    match err {
        SendTimeoutError::Timeout(Err(status)) => StreamedSendError::Timeout(status),
        SendTimeoutError::Closed(Err(status)) => StreamedSendError::Closed(status),
        SendTimeoutError::Timeout(Ok(_)) | SendTimeoutError::Closed(Ok(_)) => {
            unreachable!("Status send got an item back as an undelivered payload")
        }
    }
}

pub struct StreamedResponseWriter<TResult: Send + Sync + 'static> {
    tx: Arc<tokio::sync::mpsc::Sender<Result<TResult, tonic::Status>>>,
    rx: Option<tokio::sync::mpsc::Receiver<Result<TResult, tonic::Status>>>,
    time_out: Arc<AtomicU64>,
}

impl<TResult: Send + Sync + 'static> StreamedResponseWriter<TResult> {
    pub fn new(channel_size: usize) -> Self {
        Self::new_with_timeout(channel_size, DEFAULT_SEND_TIMEOUT)
    }

    pub fn new_with_timeout(channel_size: usize, time_out: Duration) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(channel_size);
        Self {
            tx: Arc::new(tx),
            rx: Some(rx),
            time_out: Arc::new(AtomicU64::new(time_out.as_millis() as u64)),
        }
    }

    /// Timeout is shared with the producers, including the ones handed out earlier.
    pub fn set_timeout(self, time_out: Duration) -> Self {
        self.time_out.store(time_out.as_millis() as u64, Ordering::Relaxed);
        self
    }

    #[deprecated(
        note = "Use get_stream_producer(): the writer has to be returned from the handler before the stream is read, so sending through it works only while the channel buffer has free slots"
    )]
    pub async fn send(&self, itm: TResult) -> Result<(), StreamedSendError<TResult>> {
        self.get_stream_producer().send(itm).await
    }

    #[deprecated(
        note = "Use get_stream_producer(): the writer has to be returned from the handler before the stream is read, so sending through it works only while the channel buffer has free slots"
    )]
    pub async fn send_error(
        &self,
        err: tonic::Status,
    ) -> Result<(), StreamedSendError<tonic::Status>> {
        self.get_stream_producer().send_error(err).await
    }

    pub fn get_stream_producer(&self) -> StreamedResponseProducer<TResult> {
        StreamedResponseProducer {
            tx: self.tx.clone(),
            time_out: self.time_out.clone(),
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
    > {
        let rx = match self.rx.take() {
            Some(rx) => rx,
            None => {
                return Err(tonic::Status::internal("Result is already taken"));
            }
        };

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
    time_out: Arc<AtomicU64>,
}

impl<TResult: Send + Sync + 'static> StreamedResponseProducer<TResult> {
    fn get_timeout(&self) -> Duration {
        Duration::from_millis(self.time_out.load(Ordering::Relaxed))
    }

    pub async fn send(&self, item: TResult) -> Result<(), StreamedSendError<TResult>> {
        let result = self
            .tx
            .send_timeout(Result::<_, tonic::Status>::Ok(item), self.get_timeout())
            .await;

        if let Err(err) = result {
            return Err(to_item_send_error(err));
        }

        Ok(())
    }

    pub async fn send_error(
        &self,
        err: tonic::Status,
    ) -> Result<(), StreamedSendError<tonic::Status>> {
        let result = self
            .tx
            .send_timeout(Result::<_, tonic::Status>::Err(err), self.get_timeout())
            .await;

        if let Err(err) = result {
            return Err(to_status_send_error(err));
        }

        Ok(())
    }
}
