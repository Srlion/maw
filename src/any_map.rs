use std::{any::Any, collections::HashMap};

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

pub trait AnyValue: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl AnyValue for dyn CloneableAny {
    fn as_any(&self) -> &dyn Any {
        CloneableAny::as_any(self)
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        CloneableAny::as_any_mut(self)
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        CloneableAny::into_any(self)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnyMap<V: ?Sized + AnyValue> {
    // If map is never used, no need to carry around an empty HashMap.
    // That's 3 words. Instead, this is only 1 word.
    map: Option<Box<HashMap<String, Box<V>>>>,
}

impl Clone for AnyMap<dyn CloneableAny> {
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
        }
    }
}

impl<V: ?Sized + AnyValue> AnyMap<V> {
    pub const fn new() -> Self {
        Self { map: None }
    }

    pub fn get<T: 'static>(&self, key: impl AsRef<str>) -> Option<&T> {
        self.map
            .as_ref()
            .and_then(|map| map.get(key.as_ref()))
            .and_then(|b| (*b).as_any().downcast_ref())
    }

    pub fn get_mut<T: 'static>(&mut self, key: impl AsRef<str>) -> Option<&mut T> {
        self.map
            .as_mut()
            .and_then(|map| map.get_mut(key.as_ref()))
            .and_then(|b| b.as_any_mut().downcast_mut())
    }

    pub fn remove<T: 'static>(&mut self, key: impl AsRef<str>) -> Option<T> {
        self.map
            .as_mut()
            .and_then(|map| map.remove(key.as_ref()))
            .and_then(|b| b.into_any().downcast().ok().map(|b| *b))
    }

    pub fn clear(&mut self) {
        if let Some(ref mut map) = self.map {
            map.clear();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.map.as_ref().is_none_or(|map| map.is_empty())
    }

    pub fn len(&self) -> usize {
        self.map.as_ref().map_or(0, |map| map.len())
    }

    pub fn contains_key(&self, key: impl AsRef<str>) -> bool {
        self.map
            .as_ref()
            .is_some_and(|map| map.contains_key(key.as_ref()))
    }

    pub fn extend(&mut self, other: Self) {
        if let Some(other_map) = other.map {
            if let Some(ref mut map) = self.map {
                map.extend(*other_map);
            } else {
                self.map = Some(other_map);
            }
        }
    }

    pub fn iter(&self) -> AnyMapIter<'_, V> {
        self.into_iter()
    }

    pub fn iter_mut(&mut self) -> AnyMapIterMut<'_, V> {
        self.into_iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &Box<V>> {
        self.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Box<V>> {
        self.iter_mut().map(|(_, v)| v)
    }
}

impl AnyMap<dyn CloneableAny> {
    pub fn insert<T: Clone + Send + Sync + 'static>(
        &mut self,
        key: impl Into<String>,
        val: T,
    ) -> Option<T> {
        self.map
            .get_or_insert_with(Box::default)
            .insert(key.into(), Box::new(val))
            .and_then(|b| b.into_any().downcast().ok().map(|b| *b))
    }

    pub fn set<T: Clone + Send + Sync + 'static>(&mut self, key: impl Into<String>, val: T) {
        self.map
            .get_or_insert_with(Box::default)
            .insert(key.into(), Box::new(val));
    }

    pub fn get_or_insert_with<T>(&mut self, key: impl Into<String>, f: impl FnOnce() -> T) -> &mut T
    where
        T: Clone + Send + Sync + 'static,
    {
        let out = self
            .map
            .get_or_insert_with(Box::default)
            .entry(key.into())
            .or_insert_with(|| Box::new(f()));
        out.as_any_mut().downcast_mut().unwrap()
    }

    pub fn get_or_insert<T>(&mut self, key: impl AsRef<str>, val: T) -> &mut T
    where
        T: Clone + Send + Sync + 'static,
    {
        self.get_or_insert_with(key.as_ref(), || val)
    }

    pub fn get_or_insert_default<T>(&mut self, key: impl AsRef<str>) -> &mut T
    where
        T: Default + Clone + Send + Sync + 'static,
    {
        self.get_or_insert_with(key.as_ref(), || T::default())
    }
}

pub struct AnyMapIter<'a, V: ?Sized + AnyValue> {
    inner: Option<std::collections::hash_map::Iter<'a, String, Box<V>>>,
}

impl<'a, V: ?Sized + AnyValue> Iterator for AnyMapIter<'a, V> {
    type Item = (&'a String, &'a Box<V>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_mut().and_then(|iter| iter.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner
            .as_ref()
            .map_or((0, Some(0)), |iter| iter.size_hint())
    }
}

pub struct AnyMapIterMut<'a, V: ?Sized + AnyValue> {
    inner: Option<std::collections::hash_map::IterMut<'a, String, Box<V>>>,
}

impl<'a, V: ?Sized + AnyValue> Iterator for AnyMapIterMut<'a, V> {
    type Item = (&'a String, &'a mut Box<V>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_mut().and_then(|iter| iter.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner
            .as_ref()
            .map_or((0, Some(0)), |iter| iter.size_hint())
    }
}

pub struct AnyMapIntoIter<V: ?Sized + AnyValue> {
    inner: std::collections::hash_map::IntoIter<String, Box<V>>,
}

impl<V: ?Sized + AnyValue> Iterator for AnyMapIntoIter<V> {
    type Item = (String, Box<V>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl IntoIterator for AnyMap<dyn CloneableAny> {
    type Item = (String, Box<dyn CloneableAny>);
    type IntoIter = AnyMapIntoIter<dyn CloneableAny>;

    fn into_iter(self) -> Self::IntoIter {
        AnyMapIntoIter {
            inner: self
                .map
                .map(|b| b.into_iter())
                .unwrap_or_else(|| HashMap::new().into_iter()),
        }
    }
}

impl<'a, V: ?Sized + AnyValue> IntoIterator for &'a AnyMap<V> {
    type Item = (&'a String, &'a Box<V>);
    type IntoIter = AnyMapIter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        AnyMapIter {
            inner: self.map.as_ref().map(|b| b.iter()),
        }
    }
}

impl<'a, V: ?Sized + AnyValue> IntoIterator for &'a mut AnyMap<V> {
    type Item = (&'a String, &'a mut Box<V>);
    type IntoIter = AnyMapIterMut<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        AnyMapIterMut {
            inner: self.map.as_mut().map(|b| b.iter_mut()),
        }
    }
}

#[cfg(feature = "minijinja")]
mod serializable_any {
    use super::*;

    pub trait SerializableAny: CloneableAny + erased_serde::Serialize {
        fn clone_box_serializable(&self) -> Box<dyn SerializableAny>;
    }

    impl<T: Any + serde::Serialize + Clone + Send + Sync> SerializableAny for T {
        fn clone_box_serializable(&self) -> Box<dyn SerializableAny> {
            Box::new(self.clone())
        }
    }

    impl Clone for Box<dyn SerializableAny> {
        fn clone(&self) -> Self {
            self.clone_box_serializable()
        }
    }

    impl AnyValue for dyn SerializableAny {
        fn as_any(&self) -> &dyn Any {
            CloneableAny::as_any(self)
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            CloneableAny::as_any_mut(self)
        }
        fn into_any(self: Box<Self>) -> Box<dyn Any> {
            CloneableAny::into_any(self)
        }
    }

    erased_serde::serialize_trait_object!(SerializableAny);

    impl Clone for AnyMap<dyn SerializableAny> {
        fn clone(&self) -> Self {
            Self {
                map: self.map.clone(),
            }
        }
    }

    impl AnyMap<dyn SerializableAny> {
        pub fn insert<T: serde::Serialize + Clone + Send + Sync + 'static>(
            &mut self,
            key: impl Into<String>,
            val: T,
        ) -> Option<T> {
            self.map
                .get_or_insert_with(Box::default)
                .insert(key.into(), Box::new(val))
                .and_then(|b| b.into_any().downcast().ok().map(|b| *b))
        }

        pub fn set<T: serde::Serialize + Clone + Send + Sync + 'static>(
            &mut self,
            key: impl Into<String>,
            val: T,
        ) {
            self.map
                .get_or_insert_with(Box::default)
                .insert(key.into(), Box::new(val));
        }

        pub fn get_or_insert_with<T>(
            &mut self,
            key: impl Into<String>,
            f: impl FnOnce() -> T,
        ) -> &mut T
        where
            T: serde::Serialize + Clone + Send + Sync + 'static,
        {
            let out = self
                .map
                .get_or_insert_with(Box::default)
                .entry(key.into())
                .or_insert_with(|| Box::new(f()));
            out.as_any_mut().downcast_mut().unwrap()
        }

        pub fn get_or_insert<T>(&mut self, key: impl AsRef<str>, val: T) -> &mut T
        where
            T: serde::Serialize + Clone + Send + Sync + 'static,
        {
            self.get_or_insert_with(key.as_ref(), || val)
        }

        pub fn get_or_insert_default<T>(&mut self, key: impl AsRef<str>) -> &mut T
        where
            T: Default + serde::Serialize + Clone + Send + Sync + 'static,
        {
            self.get_or_insert_with(key.as_ref(), || T::default())
        }
    }

    impl IntoIterator for AnyMap<dyn SerializableAny> {
        type Item = (String, Box<dyn SerializableAny>);
        type IntoIter = AnyMapIntoIter<dyn SerializableAny>;

        fn into_iter(self) -> Self::IntoIter {
            AnyMapIntoIter {
                inner: self
                    .map
                    .map(|b| b.into_iter())
                    .unwrap_or_else(|| HashMap::new().into_iter()),
            }
        }
    }
}

#[cfg(feature = "minijinja")]
pub use serializable_any::*;

#[cfg(not(feature = "minijinja"))]
pub use CloneableAny as SerializableAny;
