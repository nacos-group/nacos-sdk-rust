use std::sync::Arc;

use crate::api::events::{NacosEvent, Subscriber};

mod __private {

    use lazy_static::lazy_static;
    use std::{any::TypeId, collections::HashMap, sync::Arc};
    use tokio::sync::{
        mpsc::{channel, Receiver, Sender},
        RwLock,
    };
    use tracing::warn;

    use crate::{
        api::events::{NacosEvent, Subscriber},
        common::executor,
    };

    lazy_static! {
        pub static ref EVENT_BUS: EventBus = EventBus::new();
    }

    type SubscribersContainerType = Arc<RwLock<HashMap<TypeId, Vec<Arc<Box<dyn Subscriber>>>>>>;

    pub struct EventBus {
        subscribers: SubscribersContainerType,
        sender: Arc<Sender<Box<dyn NacosEvent>>>,
    }

    impl EventBus {
        pub fn new() -> Self {
            let (sender, receiver) = channel::<Box<dyn NacosEvent>>(2048);

            let subscribers = Arc::new(RwLock::new(
                HashMap::<TypeId, Vec<Arc<Box<dyn Subscriber>>>>::new(),
            ));
            Self::hand_event(receiver, subscribers.clone());
            EventBus {
                subscribers: subscribers.clone(),
                sender: Arc::new(sender),
            }
        }

        fn hand_event(
            mut receiver: Receiver<Box<dyn NacosEvent>>,
            subscribers: SubscribersContainerType,
        ) {
            executor::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    let lock_guard = subscribers.read().await;
                    let key = event.event_type();

                    let subscribers = lock_guard.get(&key);

                    if let Some(subscribers) = subscribers {
                        let event = Arc::new(event);
                        for subscriber in subscribers {
                            let event = event.clone();
                            let subscriber = subscriber.clone();
                            executor::spawn(async move {
                                subscriber.on_event(event);
                            });
                        }
                    } else {
                        warn!("{:?} has not been subscribed by anyone.", key);
                    }
                }
            });
        }

        pub fn post(&self, event: Box<dyn NacosEvent>) {
            let sender = self.sender.clone();

            executor::spawn(async move {
                let _ = sender.send(event).await;
            });
        }

        pub fn register(&self, subscriber: Arc<Box<dyn Subscriber>>) {
            let subscribers = self.subscribers.clone();
            let subscriber = subscriber.clone();
            executor::spawn(async move {
                let mut lock_guard = subscribers.write().await;

                let key = subscriber.event_type();

                let vec = lock_guard.get_mut(&key);
                if let Some(vec) = vec {
                    vec.push(subscriber);
                } else {
                    let vec = vec![subscriber];
                    lock_guard.insert(key, vec);
                }
            });
        }

        pub fn unregister(&self, subscriber: Arc<Box<dyn Subscriber>>) {
            let subscribers = self.subscribers.clone();
            let subscriber = subscriber.clone();

            executor::spawn(async move {
                let mut lock_guard = subscribers.write().await;

                let key = subscriber.event_type();

                let vec = lock_guard.get_mut(&key);

                if vec.is_none() {
                    return;
                }

                let vec = vec.unwrap();

                let index = Self::index_of_subscriber(vec, &subscriber);
                if let Some(index) = index {
                    vec.remove(index);
                }
            });
        }

        fn index_of_subscriber(
            vec: &[Arc<Box<dyn Subscriber>>],
            target: &Arc<Box<dyn Subscriber>>,
        ) -> Option<usize> {
            for (index, subscriber) in vec.iter().enumerate() {
                if Arc::ptr_eq(subscriber, target) {
                    return Some(index);
                }
            }
            None
        }
    }
}

pub fn post(event: Box<dyn NacosEvent>) {
    __private::EVENT_BUS.post(event);
}

pub fn register(subscriber: Arc<Box<dyn Subscriber>>) {
    __private::EVENT_BUS.register(subscriber);
}

pub fn unregister(subscriber: Arc<Box<dyn Subscriber>>) {
    __private::EVENT_BUS.unregister(subscriber);
}

#[cfg(test)]
mod tests {

    use core::time;
    use std::{any::Any, sync::Arc, thread};

    use crate::api::events::{NacosEvent, NacosEventSubscriber, Subscriber};

    #[derive(Clone, Debug)]
    pub(crate) struct NamingChangeEvent {
        message: String,
    }

    impl NacosEvent for NamingChangeEvent {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn event_identity(&self) -> String {
            "NamingChangeEvent".to_string()
        }
    }

    #[derive(Hash, PartialEq)]
    pub(crate) struct NamingChangeSubscriber;

    impl NacosEventSubscriber for NamingChangeSubscriber {
        type EventType = NamingChangeEvent;

        fn on_event(&self, event: &Self::EventType) {
            println!("it has already received an event. {:?}", event);
        }
    }

    #[test]
    pub fn test_post_event() {
        let event = NamingChangeEvent {
            message: "test".to_owned(),
        };

        let subscriber = Arc::new(Box::new(NamingChangeSubscriber) as Box<dyn Subscriber>);

        super::register(subscriber);

        super::post(Box::new(event));

        let three_millis = time::Duration::from_secs(3);
        thread::sleep(three_millis);
    }

    #[test]
    pub fn test_register_and_unregister() {
        let event = NamingChangeEvent {
            message: "register".to_owned(),
        };

        let subscriber = Arc::new(Box::new(NamingChangeSubscriber) as Box<dyn Subscriber>);

        super::register(subscriber.clone());

        super::post(Box::new(event));

        let one_millis = time::Duration::from_secs(1);
        thread::sleep(one_millis);

        super::unregister(subscriber);

        let event = NamingChangeEvent {
            message: "unregister".to_owned(),
        };
        super::post(Box::new(event));

        let one_millis = time::Duration::from_secs(1);
        thread::sleep(one_millis);
    }
}
