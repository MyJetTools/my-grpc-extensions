use std::time::Duration;

use tokio::sync::Mutex;

pub enum StreamedRequestInner<TItem: Clone> {
    AsVec(Vec<TItem>),
    AsStream(Mutex<Vec<Option<TItem>>>),
}

impl<TItem: Clone> StreamedRequestInner<TItem> {
    pub async fn send(&self, item: TItem) {
        match self {
            StreamedRequestInner::AsVec(_) => {
                panic!("Can not enqueue new item to send to GRPC mode since it in Vector Mode");
            }
            StreamedRequestInner::AsStream(items) => {
                let mut write_access = items.lock().await;
                write_access.push(Some(item));
            }
        }
    }

    pub async fn send_eof(&self) {
        match self {
            StreamedRequestInner::AsVec(_) => {}
            StreamedRequestInner::AsStream(items) => {
                let mut write_access = items.lock().await;
                write_access.push(None);
            }
        }
    }

    pub async fn receive(&self, index: usize) -> Option<TItem> {
        match self {
            StreamedRequestInner::AsVec(items) => items.get(index).cloned(),
            StreamedRequestInner::AsStream(mutex) => loop {
                let read_access = mutex.lock().await;
                if index < read_access.len() {
                    return read_access.get(index).cloned().unwrap();
                }

                //todo!("Tech debt")
                tokio::time::sleep(Duration::from_millis(100)).await;
            },
        }
    }
}
