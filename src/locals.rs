use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use crate::serializable_any::SerializableAny;

type AnyMap = HashMap<String, Box<dyn SerializableAny>>;

/// A type map of protocol locals.
///
/// `Locals` can be used by `Request` and `Response` to store
/// extra data derived from the underlying protocol.
#[derive(Clone, Default)]
pub struct Locals {
    // If locals are never used, no need to carry around an empty HashMap.
    // That's 3 words. Instead, this is only 1 word.
    map: Option<Box<AnyMap>>,
}

impl Locals {
    /// Create an empty `Locals`.
    #[inline]
    pub const fn new() -> Locals {
        Locals { map: None }
    }

    /// Insert a type into this `Locals`.
    ///
    /// If a extension of this type already existed, it will
    /// be returned and replaced with the new one.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// assert!(ext.insert("count", 5i32).is_none());
    /// assert!(ext.insert("byte", 4u8).is_none());
    /// assert_eq!(ext.insert("count", 9i32), Some(5i32));
    /// ```
    pub fn insert<T: Serialize + Clone + Send + Sync + 'static>(
        &mut self,
        key: impl Into<String>,
        val: T,
    ) -> Option<T> {
        self.map
            .get_or_insert_with(Box::default)
            .insert(key.into(), Box::new(val))
            .and_then(|boxed| boxed.into_any().downcast().ok().map(|boxed| *boxed))
    }

    /// Get a serializable reference for a stored value by key.
    ///
    /// Returns a reference to the trait object that can be used with any serializer.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// ext.insert("count", 5i32);
    ///
    /// if let Some(value) = ext.get("count") {
    ///     // Can serialize this value with any serializer
    ///     let json = serde_json::to_string(&value).unwrap();
    /// }
    /// ```
    pub fn get<T: 'static>(&self, key: &str) -> Option<&T> {
        self.map
            .as_ref()
            .and_then(|map| map.get(key))
            .and_then(|boxed| (**boxed).as_any().downcast_ref())
    }

    /// Get a mutable reference to a type previously inserted on this `Locals`.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// ext.insert(String::from("Hello"));
    /// ext.get_mut::<String>().unwrap().push_str(" World");
    ///
    /// assert_eq!(ext.get::<String>().unwrap(), "Hello World");
    /// ```
    pub fn get_mut<T: Send + Sync + 'static>(&mut self, key: &str) -> Option<&mut T> {
        self.map
            .as_mut()
            .and_then(|map| map.get_mut(key))
            .and_then(|boxed| (**boxed).as_any_mut().downcast_mut())
    }

    /// Get a mutable reference to a type, inserting `value` if not already present on this
    /// `Locals`.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// *ext.get_or_insert(1i32) += 2;
    ///
    /// assert_eq!(*ext.get::<i32>().unwrap(), 3);
    /// ```
    pub fn get_or_insert<T: Serialize + Clone + Send + Sync + 'static>(
        &mut self,
        key: impl Into<String>,
        value: T,
    ) -> &mut T {
        self.get_or_insert_with(key.into(), || value)
    }

    /// Get a mutable reference to a type, inserting the value created by `f` if not already present
    /// on this `Locals`.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// *ext.get_or_insert_with(|| 1i32) += 2;
    ///
    /// assert_eq!(*ext.get::<i32>().unwrap(), 3);
    /// ```
    pub fn get_or_insert_with<T: Serialize + Clone + Send + Sync + 'static, F: FnOnce() -> T>(
        &mut self,
        key: impl Into<String>,
        f: F,
    ) -> &mut T {
        let out = self
            .map
            .get_or_insert_with(Box::default)
            .entry(key.into())
            .or_insert_with(|| Box::new(f()));
        (**out).as_any_mut().downcast_mut().unwrap()
    }

    /// Get a mutable reference to a type, inserting the type's default value if not already present
    /// on this `Locals`.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// *ext.get_or_insert_default::<i32>() += 2;
    ///
    /// assert_eq!(*ext.get::<i32>().unwrap(), 2);
    /// ```
    pub fn get_or_insert_default<T: Serialize + Default + Clone + Send + Sync + 'static>(
        &mut self,
        key: impl Into<String>,
    ) -> &mut T {
        self.get_or_insert_with(key, T::default)
    }

    /// Remove a value from this `Locals`.
    ///
    /// If a value with this key existed, it will be returned.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// ext.insert("count", 5i32);
    /// assert!(ext.remove("count").is_some());
    /// assert!(ext.get("count").is_none());
    /// ```
    pub fn remove<T: 'static>(&mut self, key: impl Into<String>) -> Option<T> {
        self.map
            .as_mut()
            .and_then(|map| map.remove(&key.into()))
            .and_then(|boxed| boxed.into_any().downcast().ok().map(|boxed| *boxed))
    }

    /// Clear the `Locals` of all inserted locals.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// ext.insert("count", 5i32);
    /// ext.clear();
    ///
    /// assert!(ext.get("count").is_none());
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        if let Some(ref mut map) = self.map {
            map.clear();
        }
    }

    /// Check whether the extension set is empty or not.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// assert!(ext.is_empty());
    /// ext.insert("count", 5i32);
    /// assert!(!ext.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.as_ref().is_none_or(|map| map.is_empty())
    }

    /// Get the number of locals available.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext = Locals::new();
    /// assert_eq!(ext.len(), 0);
    /// ext.insert("count", 5i32);
    /// assert_eq!(ext.len(), 1);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.map.as_ref().map_or(0, |map| map.len())
    }

    /// Extends `self` with another `Locals`.
    ///
    /// If an instance of a specific key exists in both, the one in `self` is overwritten with the
    /// one from `other`.
    ///
    /// # Example
    ///
    /// ```
    /// # use maw::Locals;
    /// let mut ext_a = Locals::new();
    /// ext_a.insert("byte", 8u8);
    /// ext_a.insert("short", 16u16);
    ///
    /// let mut ext_b = Locals::new();
    /// ext_b.insert("byte", 4u8);
    /// ext_b.insert("greeting", "hello");
    ///
    /// ext_a.extend(ext_b);
    /// assert_eq!(ext_a.len(), 3);
    /// ```
    pub fn extend(&mut self, other: Self) {
        if let Some(other) = other.map {
            if let Some(map) = &mut self.map {
                map.extend(*other);
            } else {
                self.map = Some(other);
            }
        }
    }

    /// Check whether a key exists in the `Locals`.
    pub fn contains_key(&self, key: &str) -> bool {
        self.map.as_ref().is_some_and(|map| map.contains_key(key))
    }
}

impl fmt::Debug for Locals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Locals").finish()
    }
}

pub struct LocalsIter<'a> {
    inner: Option<std::collections::hash_map::Iter<'a, String, Box<dyn SerializableAny>>>,
}

impl<'a> Iterator for LocalsIter<'a> {
    type Item = (&'a String, &'a Box<dyn SerializableAny>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_mut().and_then(|iter| iter.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner
            .as_ref()
            .map_or((0, Some(0)), |iter| iter.size_hint())
    }
}

pub struct LocalsIterMut<'a> {
    inner: Option<std::collections::hash_map::IterMut<'a, String, Box<dyn SerializableAny>>>,
}

impl<'a> Iterator for LocalsIterMut<'a> {
    type Item = (&'a String, &'a mut Box<dyn SerializableAny>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_mut().and_then(|iter| iter.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner
            .as_ref()
            .map_or((0, Some(0)), |iter| iter.size_hint())
    }
}

pub struct LocalsIntoIter {
    inner: std::collections::hash_map::IntoIter<String, Box<dyn SerializableAny>>,
}

impl Iterator for LocalsIntoIter {
    type Item = (String, Box<dyn SerializableAny>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl IntoIterator for Locals {
    type Item = (String, Box<dyn SerializableAny>);
    type IntoIter = LocalsIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        LocalsIntoIter {
            inner: self
                .map
                .map(|boxed| (*boxed).into_iter())
                .unwrap_or_else(|| HashMap::new().into_iter()),
        }
    }
}

impl<'a> IntoIterator for &'a Locals {
    type Item = (&'a String, &'a Box<dyn SerializableAny>);
    type IntoIter = LocalsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        LocalsIter {
            inner: self.map.as_ref().map(|boxed| boxed.iter()),
        }
    }
}

impl<'a> IntoIterator for &'a mut Locals {
    type Item = (&'a String, &'a mut Box<dyn SerializableAny>);
    type IntoIter = LocalsIterMut<'a>;

    fn into_iter(self) -> Self::IntoIter {
        LocalsIterMut {
            inner: self.map.as_mut().map(|boxed| boxed.iter_mut()),
        }
    }
}

impl Locals {
    pub fn iter(&self) -> LocalsIter<'_> {
        self.into_iter()
    }

    pub fn iter_mut(&mut self) -> LocalsIterMut<'_> {
        self.into_iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &Box<dyn SerializableAny>> {
        self.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn SerializableAny>> {
        self.iter_mut().map(|(_, v)| v)
    }
}
