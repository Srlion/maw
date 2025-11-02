use serde::Serialize;
use std::any::Any;

// A trait that combines Any + Serialize
pub trait SerializableAny: Any + erased_serde::Serialize + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    fn clone_box(&self) -> Box<dyn SerializableAny>;
}

// Implement for all types that are 'static + Serialize
impl<T: Any + Serialize + Clone + Send + Sync> SerializableAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn clone_box(&self) -> Box<dyn SerializableAny> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn SerializableAny> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

erased_serde::serialize_trait_object!(SerializableAny);
