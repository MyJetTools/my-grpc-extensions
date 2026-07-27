use std::{collections::HashSet, hash::Hash, sync::Arc};

use tokio::sync::Mutex;

use super::*;

pub struct StreamedRequest<TItem: Send + Sync + 'static + Clone> {
    inner: Arc<StreamedRequestInner<TItem>>,
    channel_size: usize,
}

impl<TItem: Send + Sync + 'static + Clone> StreamedRequest<TItem> {
    pub fn new_as_vec(data: Vec<TItem>) -> Self {
        Self {
            inner: Arc::new(StreamedRequestInner::AsVec(data)),
            channel_size: 1024,
        }
    }

    pub fn new_as_stream() -> Self {
        let inner = StreamedRequestInner::AsStream(Mutex::new(RequestAsStream::default()));
        Self {
            inner: Arc::new(inner),
            channel_size: 1024,
        }
    }

    pub fn set_channel_size(mut self, value: usize) -> Self {
        self.channel_size = value;
        self
    }
    pub fn get_producer(&self) -> StreamedRequestProducer<TItem> {
        StreamedRequestProducer {
            inner: self.inner.clone(),
        }
    }

    /// Tells whether the same request can be sent one more time - see
    /// [`StreamedRequestInner::can_be_retried`].
    pub async fn can_be_retried(&self) -> bool {
        self.inner.can_be_retried().await
    }

    pub fn get_consumer(&self) -> tokio_stream::wrappers::ReceiverStream<TItem> {
        let (tx, rx) = tokio::sync::mpsc::channel(self.channel_size);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            inner.set_sender(tx).await;
        });

        tokio_stream::wrappers::ReceiverStream::new(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read_all<TItem: Send + Sync + 'static + Clone>(
        request: &StreamedRequest<TItem>,
    ) -> Vec<TItem> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
        request.inner.set_sender(tx).await;

        let mut result = Vec::new();
        while let Ok(item) = rx.try_recv() {
            result.push(item);
        }

        result
    }

    #[tokio::test]
    async fn test_vec_mode_is_sent_the_same_way_on_retry() {
        let request: StreamedRequest<u8> = vec![1u8, 2, 3].into();

        assert!(request.can_be_retried().await);
        assert_eq!(read_all(&request).await, vec![1, 2, 3]);

        assert!(request.can_be_retried().await);
        assert_eq!(read_all(&request).await, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_stream_mode_completed_before_the_first_attempt_is_sent_the_same_way_on_retry() {
        let request = StreamedRequest::<u8>::new_as_stream();

        let producer = request.get_producer();
        producer.send(1).await.unwrap();
        producer.send(2).await.unwrap();
        request.inner.send_eof().await;

        assert!(request.can_be_retried().await);
        assert_eq!(read_all(&request).await, vec![1, 2]);

        assert!(request.can_be_retried().await);
        assert_eq!(read_all(&request).await, vec![1, 2]);
    }

    #[tokio::test]
    async fn test_stream_mode_with_live_producer_is_not_retried() {
        let request = StreamedRequest::<u8>::new_as_stream();

        let producer = request.get_producer();
        producer.send(1).await.unwrap();

        assert!(request.can_be_retried().await);
        assert_eq!(read_all(&request).await, vec![1]);

        // Items are handed over to the consumer of the first attempt and are not kept, so the
        // second attempt would send a stream without them.
        assert!(!request.can_be_retried().await);
    }
}

impl<TItem: Send + Sync + 'static + Clone> Into<StreamedRequest<TItem>> for Vec<TItem> {
    fn into(self) -> StreamedRequest<TItem> {
        StreamedRequest::new_as_vec(self)
    }
}

impl<TItem: Send + Sync + 'static + Clone + Eq + Hash> Into<StreamedRequest<TItem>>
    for HashSet<TItem>
{
    fn into(self) -> StreamedRequest<TItem> {
        let data: Vec<_> = self.into_iter().collect();
        StreamedRequest::new_as_vec(data)
    }
}
