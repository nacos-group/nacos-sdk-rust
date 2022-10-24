use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use crate::api::error::Result;
use futures::Future;
pub mod naming;

pub type HandEventFuture = Box<dyn Future<Output = Result<()>> + Send + Unpin + 'static>;
pub trait NacosEvent: Any + Send + Sync + 'static {
    fn event_type(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    fn as_any(&self) -> &dyn Any;
}

pub trait Subscriber: Send + Sync + 'static {
    fn on_event(&self, event: Arc<Box<dyn NacosEvent>>) -> Option<HandEventFuture>;

    fn event_type(&self) -> TypeId;

    fn subscriber_type(&self) -> TypeId;
}

impl<T: NacosEventSubscriber> Subscriber for T {
    fn on_event(&self, event: Arc<Box<dyn NacosEvent>>) -> Option<HandEventFuture> {
        let event = event.as_any().downcast_ref::<T::EventType>()?;
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

    fn on_event(&self, event: &Self::EventType) -> Option<HandEventFuture>;
}
