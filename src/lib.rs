#![feature(arbitrary_self_types)]
#![feature(once_cell)]

use std::marker::PhantomData;
use std::ptr::NonNull;

pub use ctor::ctor;
use erased_serde::Error;
use metatype::type_coerce;
use serde::de::DeserializeOwned;
use serde::de::Unexpected::Str;
use serde::ser::SerializeTuple;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use serde_box_macro::serde_box;

#[derive(Eq, PartialEq, Debug)]
pub struct SerdeBox<T: ?Sized>(Box<T>);

pub trait SerdeBoxSer: erased_serde::Serialize {
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

impl<T: Serialize> SerdeBoxSer for T {}

pub trait SerdeBoxDe {
    fn de(
        self: *const Self,
        deserializer: &mut dyn erased_serde::Deserializer,
    ) -> Result<NonNull<()>, erased_serde::Error>;
}

impl<T: DeserializeOwned> SerdeBoxDe for T {
    fn de(
        self: *const Self,
        deserializer: &mut dyn erased_serde::Deserializer,
    ) -> Result<NonNull<()>, Error> {
        erased_serde::deserialize::<Self>(deserializer)
            .map(|value| NonNull::new(Box::into_raw(Box::new(value)).cast()).unwrap())
    }
}

impl<T: ?Sized + SerdeBoxSer> Serialize for SerdeBox<T> {
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

struct ErasedDe<T: ?Sized + SerdeBoxDe>(*const T);
impl<'de, T: ?Sized + SerdeBoxDe> serde::de::DeserializeSeed<'de> for ErasedDe<T> {
    type Value = Box<T>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let deserializer = &mut <dyn erased_serde::Deserializer>::erase(deserializer);
        self.0
            .de(deserializer)
            .map(|raw| {
                let object: *mut T =
                    metatype::Type::fatten(raw.as_ptr(), metatype::Type::meta(self.0));
                unsafe { Box::from_raw(object) }
            })
            .map_err(serde::de::Error::custom)
    }
}

struct ErasedSer<'s, T: 's + ?Sized + SerdeBoxSer>(&'s T);
impl<'s, T: ?Sized + SerdeBoxSer> serde::Serialize for ErasedSer<'s, T> {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        erased_serde::serialize(self.0, serializer)
    }
}

impl<'de, T: ?Sized + SerdeBoxDe + SerdeBoxRegistry> Deserialize<'de> for SerdeBox<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        use std::*;

        struct Visitor<T: ?Sized>(PhantomData<T>);
        impl<'de, T: ?Sized + SerdeBoxDe + SerdeBoxRegistry> serde::de::Visitor<'de> for Visitor<T> {
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
                    None => return Err(serde::de::Error::invalid_value(Str(&type_name), &self)),
                    Some(vtable) => vtable,
                };

                let meta = metatype::TraitObject { vtable };
                let object: *const T = metatype::Type::dangling(type_coerce(meta)).as_ptr();
                let object: Box<T> = match seq.next_element_seed(ErasedDe(object))? {
                    Some(value) => value,
                    None => return Err(serde::de::Error::invalid_length(1, &self)),
                };
                Ok(SerdeBox(object))
            }
        }
        deserializer.deserialize_tuple(2, Visitor::<T>(PhantomData::default()))
    }
}

pub struct Registry {
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

pub trait SerdeBoxRegistry {
    fn get_registry() -> &'static Registry;
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::fmt::Debug;

    use serde::{Deserialize, Serialize};

    use crate::*;

    #[serde_box]
    trait Message: SerdeBoxSer + SerdeBoxDe + Any + Debug {
        fn as_any(&self) -> &dyn Any;
        fn is_eq(&self, other: &dyn Message) -> bool;
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Messages {
        messages: Vec<SerdeBox<dyn Message>>,
    }

    impl PartialEq<dyn Message> for dyn Message {
        fn eq(&self, other: &dyn Message) -> bool {
            self.is_eq(other)
        }
    }

    #[derive(Deserialize, Serialize, Eq, PartialEq, Debug)]
    struct MyMessage1 {
        val: i32,
        val_b: u32,
    }

    #[serde_box]
    impl Message for MyMessage1 {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn is_eq(&self, other: &dyn Message) -> bool {
            if let Some(other) = other.as_any().downcast_ref::<Self>() {
                self.eq(other)
            } else {
                false
            }
        }
    }

    #[derive(Deserialize, Serialize, Eq, PartialEq, Debug)]
    struct MyMessage2 {
        val2: String,
        val2_b: i128,
    }

    #[serde_box]
    impl Message for MyMessage2 {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn is_eq(&self, other: &dyn Message) -> bool {
            if let Some(other) = other.as_any().downcast_ref::<Self>() {
                self.eq(other)
            } else {
                false
            }
        }
    }

    #[test]
    fn ser() {
        let messages = get_messages();
        let ser = serde_json::to_string(&messages).unwrap();
        assert_eq!(get_messages_json(), &ser);
    }

    #[test]
    fn de() {
        let messages_json = get_messages_json();
        let messages: Messages = serde_json::from_str(messages_json).unwrap();
        assert_eq!(messages, get_messages());
    }

    fn get_messages() -> Messages {
        Messages {
            messages: vec![
                SerdeBox(Box::new(MyMessage1 { val: 1, val_b: 2 })),
                SerdeBox(Box::new(MyMessage2 {
                    val2: "3".to_string(),
                    val2_b: 4,
                })),
            ],
        }
    }

    fn get_messages_json() -> &'static str {
        r##"{"messages":[["serde_box::tests::MyMessage1",{"val":1,"val_b":2}],["serde_box::tests::MyMessage2",{"val2":"3","val2_b":4}]]}"##
    }
}
