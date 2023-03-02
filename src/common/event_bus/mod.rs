use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use tracing::warn;

pub(crate) trait NacosEvent: Any + Send + Sync + 'static {
    fn event_type(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    fn as_any(&self) -> &dyn Any;

    fn event_identity(&self) -> String;
}

pub(crate) trait Subscriber: Send + Sync + 'static {
    fn on_event(&self, event: Arc<dyn NacosEvent>);

    fn event_type(&self) -> TypeId;

    fn subscriber_type(&self) -> TypeId;
}

impl<T: NacosEventSubscriber> Subscriber for T {
    fn on_event(&self, event: Arc<dyn NacosEvent>) {
        let event_identity = event.event_identity();
        let event = event.as_any().downcast_ref::<T::EventType>();
        if event.is_none() {
            warn!("event {} cannot cast target object", event_identity);
            return;
        }
        let event = event.unwrap();
        self.on_event(event)
    }

    fn event_type(&self) -> TypeId {
        TypeId::of::<T::EventType>()
    }

    fn subscriber_type(&self) -> TypeId {
        TypeId::of::<T>()
    }
}

pub(crate) trait NacosEventSubscriber: Send + Sync + 'static {
    type EventType;

    fn on_event(&self, event: &Self::EventType);
}

mod __private {

    use lazy_static::lazy_static;
    use std::{any::TypeId, collections::HashMap, sync::Arc};
    use tokio::sync::{
        mpsc::{channel, Receiver, Sender},
        RwLock,
    };
    use tracing::warn;

    use crate::common::executor;

    use super::{NacosEvent, Subscriber};

    lazy_static! {
        pub(crate) static ref EVENT_BUS: EventBus = EventBus::new();
    }

    type SubscribersContainerType = Arc<RwLock<HashMap<TypeId, Vec<Arc<dyn Subscriber>>>>>;

    pub(crate) struct EventBus {
        subscribers: SubscribersContainerType,
        sender: Arc<Sender<Arc<dyn NacosEvent>>>,
    }

    impl EventBus {
        pub(crate) fn new() -> Self {
            let (sender, receiver) = channel::<Arc<dyn NacosEvent>>(2048);

            let subscribers = Arc::new(RwLock::new(
                HashMap::<TypeId, Vec<Arc<dyn Subscriber>>>::new(),
            ));
            Self::hand_event(receiver, subscribers.clone());
            EventBus {
                subscribers: subscribers.clone(),
                sender: Arc::new(sender),
            }
        }

        fn hand_event(
            mut receiver: Receiver<Arc<dyn NacosEvent>>,
            subscribers: SubscribersContainerType,
        ) {
            executor::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    let lock_guard = subscribers.read().await;
                    let key = event.event_type();

                    let subscribers = lock_guard.get(&key);

                    if let Some(subscribers) = subscribers {
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

        pub(crate) fn post(&self, event: Arc<dyn NacosEvent>) {
            let sender = self.sender.clone();

            executor::spawn(async move {
                let _ = sender.send(event).await;
            });
        }

        pub(crate) fn register(&self, subscriber: Arc<dyn Subscriber>) {
            let subscribers = self.subscribers.clone();
            let subscriber = subscriber.clone();
            executor::spawn(async move {
                let mut lock_guard = subscribers.write().await;

                let key = subscriber.event_type();

                let vec = lock_guard.get_mut(&key);
                if let Some(vec) = vec {
                    let index = Self::index_of_subscriber(vec, &subscriber);
                    if let Some(index) = index {
                        vec.remove(index);
                    }
                    vec.push(subscriber);
                } else {
                    let vec = vec![subscriber];
                    lock_guard.insert(key, vec);
                }
            });
        }

        pub(crate) fn unregister(&self, subscriber: Arc<dyn Subscriber>) {
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
            vec: &[Arc<dyn Subscriber>],
            target: &Arc<dyn Subscriber>,
        ) -> Option<usize> {
            for (index, subscriber) in vec.iter().enumerate() {
                let subscriber_trait_ptr = subscriber.as_ref() as *const dyn Subscriber;
                let (subscriber_data_ptr, _): (*const u8, *const u8) =
                    unsafe { std::mem::transmute(subscriber_trait_ptr) };

                let target_trait_ptr = target.as_ref() as *const dyn Subscriber;
                let (target_data_ptr, _): (*const u8, *const u8) =
                    unsafe { std::mem::transmute(target_trait_ptr) };

                if subscriber_data_ptr == target_data_ptr {
                    return Some(index);
                }
            }
            None
        }
    }
}

pub(crate) fn post(event: Arc<dyn NacosEvent>) {
    __private::EVENT_BUS.post(event);
}

pub(crate) fn register(subscriber: Arc<dyn Subscriber>) {
    __private::EVENT_BUS.register(subscriber);
}

#[allow(dead_code)]
pub(crate) fn unregister(subscriber: Arc<dyn Subscriber>) {
    __private::EVENT_BUS.unregister(subscriber);
}

#[cfg(test)]
mod tests {

    use core::time;
    use std::{any::Any, sync::Arc, thread};

    use super::{NacosEvent, NacosEventSubscriber};

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

        let subscriber = Arc::new(NamingChangeSubscriber);

        super::register(subscriber);

        super::post(Arc::new(event));

        let three_millis = time::Duration::from_secs(3);
        thread::sleep(three_millis);
    }

    #[test]
    pub fn test_register_and_unregister() {
        let event = NamingChangeEvent {
            message: "register".to_owned(),
        };

        let subscriber = Arc::new(NamingChangeSubscriber);

        super::register(subscriber.clone());

        super::post(Arc::new(event));

        let one_millis = time::Duration::from_secs(1);
        thread::sleep(one_millis);

        super::unregister(subscriber);

        let event = NamingChangeEvent {
            message: "unregister".to_owned(),
        };
        super::post(Arc::new(event));

        let one_millis = time::Duration::from_secs(1);
        thread::sleep(one_millis);
    }
}
