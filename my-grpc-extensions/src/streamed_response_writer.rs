use std::{
    fmt,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, ready},
    time::Duration,
};

use tokio::sync::mpsc::error::SendTimeoutError;

#[deprecated(note = "Please use StreamedResponseWriter")]
pub type GrpcOutputStream<TResult> = StreamedResponseWriter<TResult>;

pub const DEFAULT_SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Error of pushing a value into the response stream. Both cases hand the undelivered value back.
pub enum StreamedSendError<TItem> {
    /// Channel had no free slot for longer than the send timeout: the consumer is alive, but not
    /// reading. The value is missing from the response, so the stream is marked as aborted and ends
    /// with an error whatever is sent after it. Stop producing; if the consumer is expected to be
    /// slow, raise the timeout instead of retrying.
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

/// The client sees a clean end of the stream only when every producer finished normally. If one of
/// them panicked, or one of its sends timed out, the stream ends with `Status::internal` instead,
/// so a partial response never passes for a whole one.
pub struct StreamedResponseWriter<TResult: Send + Sync + 'static> {
    tx: Arc<tokio::sync::mpsc::Sender<Result<TResult, tonic::Status>>>,
    rx: Option<tokio::sync::mpsc::Receiver<Result<TResult, tonic::Status>>>,
    time_out: Arc<AtomicU64>,
    aborted: Arc<AtomicBool>,
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
            aborted: Arc::new(AtomicBool::new(false)),
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
            aborted: self.aborted.clone(),
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

        let output_stream = ResponseStream {
            rx,
            aborted: Some(self.aborted.clone()),
        };
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

// The channel, plus a final error in place of the clean end if a producer was aborted.
struct ResponseStream<TResult> {
    rx: tokio::sync::mpsc::Receiver<Result<TResult, tonic::Status>>,
    // Taken once the channel is drained, so that the final error is yielded only once.
    aborted: Option<Arc<AtomicBool>>,
}

impl<TResult> futures_util::Stream for ResponseStream<TResult> {
    type Item = Result<TResult, tonic::Status>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(item) = ready!(self.rx.poll_recv(cx)) {
            return Poll::Ready(Some(item));
        }

        // Relaxed is enough: the flag is set only by a producer that still holds a sender, and the
        // channel reports its end only after the last sender is dropped.
        match self.aborted.take() {
            Some(aborted) if aborted.load(Ordering::Relaxed) => Poll::Ready(Some(Err(
                tonic::Status::internal("stream producer stopped before finishing"),
            ))),
            _ => Poll::Ready(None),
        }
    }
}

pub struct StreamedResponseProducer<TResult: Send + Sync + 'static> {
    tx: Arc<tokio::sync::mpsc::Sender<Result<TResult, tonic::Status>>>,
    time_out: Arc<AtomicU64>,
    aborted: Arc<AtomicBool>,
}

impl<TResult: Send + Sync + 'static> StreamedResponseProducer<TResult> {
    fn get_timeout(&self) -> Duration {
        Duration::from_millis(self.time_out.load(Ordering::Relaxed))
    }

    // A timed out value is missing from the response, so it can't end cleanly anymore. Closed is
    // not an abort: there is nobody left to tell.
    fn abort_on_timeout<TItem>(&self, err: &StreamedSendError<TItem>) {
        if err.is_timeout() {
            self.aborted.store(true, Ordering::Relaxed);
        }
    }

    pub async fn send(&self, item: TResult) -> Result<(), StreamedSendError<TResult>> {
        let result = self
            .tx
            .send_timeout(Result::<_, tonic::Status>::Ok(item), self.get_timeout())
            .await;

        if let Err(err) = result {
            let err = to_item_send_error(err);
            self.abort_on_timeout(&err);
            return Err(err);
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
            let err = to_status_send_error(err);
            self.abort_on_timeout(&err);
            return Err(err);
        }

        Ok(())
    }
}

impl<TResult: Send + Sync + 'static> Drop for StreamedResponseProducer<TResult> {
    fn drop(&mut self) {
        // Dropped by unwinding: the producer panicked half-way, and nobody is left to send the
        // error.
        if std::thread::panicking() {
            self.aborted.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;

    use super::*;

    type TestStream =
        Pin<Box<dyn futures_util::Stream<Item = Result<u32, tonic::Status>> + Send + Sync>>;

    fn get_stream(writer: StreamedResponseWriter<u32>) -> TestStream {
        writer.get_result().unwrap().into_inner()
    }

    async fn read_to_end(mut stream: TestStream) -> Vec<Result<u32, tonic::Code>> {
        let mut result = Vec::new();
        while let Some(item) = stream.next().await {
            result.push(item.map_err(|status| status.code()));
            assert!(result.len() <= 16, "the stream does not end");
        }

        result
    }

    #[tokio::test]
    async fn test_stream_of_finished_producers_ends_cleanly() {
        let writer = StreamedResponseWriter::new(16);
        let producer = writer.get_stream_producer();
        let stream = get_stream(writer);

        tokio::spawn(async move {
            for i in 0..3 {
                producer.send(i).await.unwrap();
            }
        });

        assert_eq!(read_to_end(stream).await, vec![Ok(0), Ok(1), Ok(2)]);
    }

    #[tokio::test]
    async fn test_panicked_producer_ends_stream_with_error() {
        let writer = StreamedResponseWriter::new(16);
        let producer = writer.get_stream_producer();
        let stream = get_stream(writer);

        let task = tokio::spawn(async move {
            producer.send(0).await.unwrap();
            producer.send(1).await.unwrap();
            panic!("producer failed half-way");
        });
        assert!(task.await.unwrap_err().is_panic());

        assert_eq!(
            read_to_end(stream).await,
            vec![Ok(0), Ok(1), Err(tonic::Code::Internal)]
        );
    }

    #[tokio::test]
    async fn test_one_panicked_producer_of_many_ends_stream_with_error() {
        let writer = StreamedResponseWriter::new(16);
        let finished = writer.get_stream_producer();
        let panicked = writer.get_stream_producer();
        let stream = get_stream(writer);

        tokio::spawn(async move {
            finished.send(0).await.unwrap();
        });
        let task = tokio::spawn(async move {
            let _producer = panicked;
            panic!("producer failed before sending anything");
        });
        assert!(task.await.unwrap_err().is_panic());

        assert_eq!(
            read_to_end(stream).await,
            vec![Ok(0), Err(tonic::Code::Internal)]
        );
    }

    #[tokio::test]
    async fn test_timed_out_send_ends_stream_with_error() {
        let writer = StreamedResponseWriter::new_with_timeout(1, Duration::from_millis(10));
        let producer = writer.get_stream_producer();
        let stream = get_stream(writer);

        producer.send(0).await.unwrap();
        // The only slot is taken, and nobody reads yet.
        assert!(producer.send(1).await.unwrap_err().is_timeout());
        drop(producer);

        assert_eq!(
            read_to_end(stream).await,
            vec![Ok(0), Err(tonic::Code::Internal)]
        );
    }

    #[tokio::test]
    async fn test_timed_out_send_error_ends_stream_with_error() {
        let writer = StreamedResponseWriter::new_with_timeout(1, Duration::from_millis(10));
        let producer = writer.get_stream_producer();
        let stream = get_stream(writer);

        producer.send(0).await.unwrap();
        let err = producer
            .send_error(tonic::Status::not_found("gone"))
            .await
            .unwrap_err();
        assert!(err.is_timeout());
        drop(producer);

        assert_eq!(
            read_to_end(stream).await,
            vec![Ok(0), Err(tonic::Code::Internal)]
        );
    }
}
