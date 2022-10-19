use std::{
    any::{Any, TypeId},
    sync::Arc,
};

pub mod naming;

pub trait NacosEvent: Any + Send + Sync + 'static {
    fn event_type(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    fn as_any(&self) -> &dyn Any;
}

pub trait Subscriber: Send + Sync + 'static {
    fn on_event(&self, event: Arc<Box<dyn NacosEvent>>);

    fn event_type(&self) -> TypeId;

    fn subscriber_type(&self) -> TypeId;
}

impl<T: NacosEventSubscriber> Subscriber for T {
    fn on_event(&self, event: Arc<Box<dyn NacosEvent>>) {
        let event = event.as_any().downcast_ref::<T::EventType>();
        if event.is_none() {
            return;
        }
        let event = event.unwrap();
        self.on_event(event);
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
