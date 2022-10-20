use std::sync::Arc;

use crate::api::events::{NacosEvent, Subscriber};

pub(self) mod __private {

    use lazy_static::lazy_static;
    use std::{
        any::TypeId,
        collections::{HashMap, HashSet},
        sync::{Arc, RwLock},
    };
    use tokio::sync::mpsc::{channel, Receiver, Sender};
    use tracing::error;

    use crate::{
        api::events::{NacosEvent, Subscriber},
        common::executor,
    };

    lazy_static! {
        pub static ref EVENT_BUS: EventBus = EventBus::new();
    }

    type SubscribersContainerType = Arc<RwLock<HashMap<TypeId, HashSet<Arc<Box<dyn Subscriber>>>>>>;

    pub struct EventBus {
        subscribers: SubscribersContainerType,
        sender: Arc<Sender<Box<dyn NacosEvent>>>,
    }

    impl EventBus {
        pub fn new() -> Self {
            let (sender, receiver) = channel::<Box<dyn NacosEvent>>(2048);

            let subscribers = Arc::new(RwLock::new(HashMap::<
                TypeId,
                HashSet<Arc<Box<dyn Subscriber>>>,
            >::new()));
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
                    let lock = subscribers.read();
                    if let Err(error) = lock {
                        error!("hand event failed, cannot get lock! {:?}", error);
                        return;
                    }
                    let lock_guard = lock.unwrap();

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
            let lock = self.subscribers.write();
            if let Err(error) = lock {
                error!("register failed, cannot get lock! {:?}", error);
                return;
            }
            let mut lock_guard = lock.unwrap();

            let key = subscriber.event_type();

            let set = lock_guard.get_mut(&key);
            if let Some(set) = set {
                set.insert(subscriber);
            } else {
                let mut set = HashSet::default();
                set.insert(subscriber);
                lock_guard.insert(key, set);
            }
        }

        pub fn unregister(&self, subscriber: Arc<Box<dyn Subscriber>>) {
            let lock = self.subscribers.write();
            if let Err(error) = lock {
                error!("unregister failed, cannot get lock! {:?}", error);
                return;
            }
            let mut lock_guard = lock.unwrap();

            let key = subscriber.event_type();

            let set = lock_guard.get_mut(&key);

            if set.is_none() {
                return;
            }

            let set = set.unwrap();
            set.remove(&subscriber);
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
}
