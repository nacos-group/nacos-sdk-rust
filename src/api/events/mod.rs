use std::hash::Hash;
use std::{
    any::{Any, TypeId},
    collections::hash_map::DefaultHasher,
    hash::Hasher,
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

    fn as_any(&self) -> &dyn Any;

    fn eq_subscriber(&self, other: &dyn Subscriber) -> bool;

    fn hash_value(&self) -> u64;
}

impl PartialEq for dyn Subscriber {
    fn eq(&self, other: &Self) -> bool {
        self.eq_subscriber(other)
    }
}

impl Hash for dyn Subscriber {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash_value().hash(state);
    }
}

impl Eq for dyn Subscriber {}

impl<T: NacosEventSubscriber + PartialEq + Hash> Subscriber for T {
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

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn eq_subscriber(&self, other: &dyn Subscriber) -> bool {
        other
            .as_any()
            .downcast_ref::<T>()
            .map_or(false, |subscriber| self == subscriber)
    }

    fn hash_value(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

pub trait NacosEventSubscriber: Send + Sync + 'static {
    type EventType;

    fn on_event(&self, event: &Self::EventType);
}
