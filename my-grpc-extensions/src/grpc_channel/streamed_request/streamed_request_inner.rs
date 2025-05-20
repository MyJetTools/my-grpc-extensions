use tokio::sync::Mutex;

pub struct RequestAsStream<TItem: Clone> {
    items: Vec<TItem>,
    has_end_of_stream: bool,
    sender: Option<tokio::sync::mpsc::Sender<TItem>>,
}

pub enum StreamedRequestInner<TItem: Clone> {
    AsVec(Vec<TItem>),
    AsStream(Mutex<RequestAsStream<TItem>>),
}

impl<TItem: Clone> StreamedRequestInner<TItem> {
    pub async fn send(&self, item: TItem) {
        match self {
            StreamedRequestInner::AsVec(_) => {
                panic!("Can not enqueue new item to send to GRPC mode since it in Vector Mode");
            }
            StreamedRequestInner::AsStream(inner) => {
                let mut write_access = inner.lock().await;

                if write_access.has_end_of_stream {
                    panic!("Can not send message when end of stream is initiated");
                }
                write_access.items.push(item.clone());
                if let Some(sender) = write_access.sender.as_ref() {
                    let err = sender.send(item).await;

                    if let Err(err) = err {
                        println!("Can not send grpc item to stream #3. Err: {}", err);
                        return;
                    }
                }
            }
        }
    }

    pub async fn set_sender(&self, sender: tokio::sync::mpsc::Sender<TItem>) {
        match self {
            StreamedRequestInner::AsVec(items) => {
                for itm in items {
                    let err = sender.send(itm.clone()).await;

                    if let Err(err) = err {
                        println!("Can not send grpc item to stream #1. Err: {}", err);
                        return;
                    }
                }
            }
            StreamedRequestInner::AsStream(mutex) => {
                let mut write_access = mutex.lock().await;

                if let Some(sender) = write_access.sender.as_ref() {
                    for itm in write_access.items.iter() {
                        let err = sender.send(itm.clone()).await;
                        if let Err(err) = err {
                            println!("Can not send grpc item to stream #2. Err: {}", err);
                            return;
                        }
                    }
                }

                if !write_access.has_end_of_stream {
                    write_access.sender = Some(sender);
                }
            }
        }
    }

    pub async fn send_eof(&self) {
        match self {
            StreamedRequestInner::AsVec(_) => {}
            StreamedRequestInner::AsStream(items) => {
                let mut write_access = items.lock().await;
                write_access.has_end_of_stream = true;
                write_access.sender.take();
            }
        }
    }
}
