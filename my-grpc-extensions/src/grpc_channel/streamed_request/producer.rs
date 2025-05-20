use std::sync::Arc;

use super::StreamedRequestInner;

// Producer
pub struct StreamedRequestProducer<TItem: Send + Sync + 'static + Clone> {
    pub(crate) inner: Arc<StreamedRequestInner<TItem>>,
}

impl<TItem: Send + Sync + 'static + Clone> StreamedRequestProducer<TItem> {
    pub async fn send(&self, item: TItem) {
        self.inner.send(item).await;
    }
}

impl<TItem: Send + Sync + 'static + Clone> Drop for StreamedRequestProducer<TItem> {
    fn drop(&mut self) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            inner.send_eof().await;
        });
    }
}
