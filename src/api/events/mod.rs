use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use tracing::warn;
pub mod naming;

pub trait NacosEvent: Any + Send + Sync + 'static {
    fn event_type(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    fn as_any(&self) -> &dyn Any;

    fn event_identity(&self) -> String;
}

pub trait Subscriber: Send + Sync + 'static {
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

pub trait NacosEventSubscriber: Send + Sync + 'static {
    type EventType;

    fn on_event(&self, event: &Self::EventType);
}
