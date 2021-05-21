use std::collections::HashMap;
use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use serde::ser::SerializeTuple;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

struct SerdeBox<T: ?Sized>(Box<T>);

trait SerTrait {
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}
impl<T: Serialize> SerTrait for T {}

trait DeTrait {}
impl<T: DeserializeOwned> DeTrait for T {}

impl<T: ?Sized + SerTrait> Serialize for SerdeBox<T> {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let mut tuple: <S as Serializer>::SerializeTuple = serializer.serialize_tuple(2)?;
        tuple.serialize_element(self.0.type_name())?;
        tuple.serialize_element(&ErasedSer(self.0.as_ref()))?;
        tuple.end()
    }
}

struct ErasedDe<T: ?Sized + DeTrait>(*const T);
impl<'de, T: ?Sized + DeTrait> serde::de::DeserializeSeed<'de> for ErasedDe<T> {
    type Value = Box<T>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let deserializer = &mut erased_serde::Deserializer::erase(deserializer);
        erased_serde::deserialize(deserializer)
    }
}

struct ErasedSer<'s, T: ?Sized + SerTrait>(&'s T);
impl<'s, T: ?Sized + SerTrait> serde::Serialize for ErasedSer<'s, T> {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        erased_serde::serialize(self, serializer)
    }
}

struct Registry {
    type_name_to_vtable: dashmap::DashMap<String, &'static ()>,
}

impl Registry {
    pub fn insert(&self, type_name: String, vtable: &'static ()) {
        self.type_name_to_vtable.insert(type_name, vtable);
    }

    pub fn get(&self, type_name: &str) -> Option<&'static ()> {
        self.type_name_to_vtable
            .get(type_name)
            .map(|pair| *pair.value())
    }
}

trait SerdeBoxRegistry {
    fn get_registry() -> &'static Registry;
}

trait MyTraitA {}

impl SerdeBoxRegistry for dyn MyTraitA {
    fn get_registry() -> &'static Registry {
        static REGISTRY: Registry = Registry {
            type_name_to_vtable: Default::default(),
        };
        &REGISTRY
    }
}
impl<'de, T: ?Sized + DeTrait + SerdeBoxRegistry> Deserialize<'de> for SerdeBox<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        use std::*;

        struct Visitor<T: ?Sized>(PhantomData<T>);
        impl<'de, T: ?Sized + SerdeBoxRegistry> serde::de::Visitor<'de> for Visitor<T> {
            type Value = SerdeBox<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a MyTraitBox")
            }
            #[inline]
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let type_name: String = match seq.next_element()? {
                    Some(value) => value,
                    None => return Err(serde::de::Error::invalid_length(0, &self)),
                };

                let registry = T::get_registry();
                let vtable = match registry.get(&type_name) {
                    None => return Err(serde::de::Error::invalid_value(&type_name, &self)),
                    Some(vtable) => vtable,
                };

                let val: Box<T> = match seq.next_element_seed(ErasedDe())? {
                    Some(value) => value,
                    None => return Err(serde::de::Error::invalid_length(1, &self)),
                };
                Ok(SerdeBox(val))
            }
        }
        deserializer.deserialize_tuple(2, Visitor::<T>(PhantomData::default()))
    }
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};

    use crate::{DeTrait, SerTrait, SerdeBox};

    trait Message: SerTrait + DeTrait {}
    serde_box_register!(SerdeBoxMessage);

    struct Messages {
        messages: Vec<SerdeBox<dyn Message>>,
    }

    #[test]
    fn ser() {}
}
