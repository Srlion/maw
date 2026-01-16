use serde::Serialize;
use std::any::Any;

// Base trait for just Any + Clone
pub trait CloneableAny: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    fn clone_box(&self) -> Box<dyn CloneableAny>;
}

impl<T: Any + Clone + Send + Sync> CloneableAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn clone_box(&self) -> Box<dyn CloneableAny> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn CloneableAny> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

// Extended trait that adds Serialize
pub trait SerializableAny: CloneableAny + erased_serde::Serialize {
    fn clone_box_serializable(&self) -> Box<dyn SerializableAny>;
}

impl<T: Any + Serialize + Clone + Send + Sync> SerializableAny for T {
    fn clone_box_serializable(&self) -> Box<dyn SerializableAny> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn SerializableAny> {
    fn clone(&self) -> Self {
        self.clone_box_serializable()
    }
}

erased_serde::serialize_trait_object!(SerializableAny);
