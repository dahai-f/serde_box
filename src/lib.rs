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

impl<'de, T: ?Sized + DeTrait> Deserialize<'de> for SerdeBox<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        use std::*;

        struct Visitor<T: ?Sized>(PhantomData<T>);
        impl<'de, T: ?Sized> serde::de::Visitor<'de> for Visitor<T> {
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

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};

    use crate::{DeTrait, SerTrait, SerdeBox};

    trait Message: SerTrait + DeTrait {}

    struct Messages {
        messages: Vec<SerdeBox<dyn Message>>,
    }
}
