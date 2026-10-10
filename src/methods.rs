//! User-registered methods: untyped ones that work on `FactValue`s directly, and typed
//! ones that work on the caller's own Rust types (converted with serde around each
//! call), the equivalent of methods on Go fact structs in Grule.

use crate::facts::FactValue;
use crate::functions::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::any::{type_name, TypeId};
use std::collections::HashMap;

/// A typed method: returns `None` when the receiver isn't of its type.
type TypedMethod =
    dyn Fn(&mut FactValue, &[FactValue]) -> Option<Result<FactValue, String>> + Send + Sync;

enum Entry {
    Typed {
        type_id: TypeId,
        call: Box<TypedMethod>,
    },
    Untyped(Box<Method>),
}

#[derive(Default)]
pub(crate) struct MethodRegistry {
    methods: HashMap<String, Vec<Entry>>,
}

impl MethodRegistry {
    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.methods.contains_key(name)
    }

    pub fn register(&mut self, name: &str, method: Box<Method>) -> Result<(), String> {
        let entries = self.methods.entry(name.to_string()).or_default();
        if entries.iter().any(|e| matches!(e, Entry::Untyped(_))) {
            return Err(format!("Method '{}' is already registered", name));
        }
        entries.push(Entry::Untyped(method));
        Ok(())
    }

    pub fn register_typed<T, A, R, F>(&mut self, name: &str, method: F) -> Result<(), String>
    where
        T: Serialize + DeserializeOwned + 'static,
        A: DeserializeOwned + 'static,
        R: Serialize + 'static,
        F: Fn(&mut T, A) -> Result<R, String> + Send + Sync + 'static,
    {
        let entries = self.methods.entry(name.to_string()).or_default();
        let type_id = TypeId::of::<T>();
        if entries
            .iter()
            .any(|e| matches!(e, Entry::Typed { type_id: t, .. } if *t == type_id))
        {
            return Err(format!(
                "Method '{}' is already registered for {}",
                name,
                type_name::<T>()
            ));
        }
        let call = move |receiver: &mut FactValue, args: &[FactValue]| {
            // A receiver that doesn't convert into `T` is some other type
            let mut value: T = receiver.deserialize().ok()?;
            Some((|| {
                let args: A = decode_args(args)?;
                let result = method(&mut value, args)?;
                merge(receiver, FactValue::from_serialize(&value)?);
                FactValue::from_serialize(&result)
            })())
        };
        entries.push(Entry::Typed {
            type_id,
            call: Box::new(call),
        });
        Ok(())
    }

    /// Calls the method `name` on `receiver`: the first typed registration whose type
    /// the receiver converts into, otherwise the untyped one. Returns `None` if no
    /// method of that name is registered.
    pub fn call(
        &self,
        receiver: &mut FactValue,
        name: &str,
        args: &[FactValue],
    ) -> Option<Result<FactValue, String>> {
        let entries = self.methods.get(name)?;
        for entry in entries {
            if let Entry::Typed { call, .. } = entry {
                if let Some(result) = call(receiver, args) {
                    return Some(result);
                }
            }
        }
        for entry in entries {
            if let Entry::Untyped(method) = entry {
                return Some(method(receiver, args));
            }
        }
        Some(Err(format!(
            "no registered type matches this {}",
            receiver.type_name()
        )))
    }
}

/// Decodes a method's arguments into `A`: a tuple for fixed arguments (`(f64,)`,
/// `(String, i64)`), `()` for none, or a `Vec` for any number of them.
fn decode_args<A: DeserializeOwned>(args: &[FactValue]) -> Result<A, String> {
    let as_array = FactValue::Array(args.to_vec()).deserialize::<A>();
    match as_array {
        // `()` decodes from nil rather than from an empty list
        Err(_) if args.is_empty() => FactValue::Null.deserialize::<A>(),
        result => result,
    }
    .map_err(|e| format!("invalid arguments: {}", e))
}

/// Writes a method's updated receiver back, keeping fields the receiver had that its
/// Rust type doesn't (for example ones a rule assigned).
fn merge(receiver: &mut FactValue, updated: FactValue) {
    match (receiver, updated) {
        (FactValue::Object(fields), FactValue::Object(updated)) => fields.extend(updated),
        (receiver, updated) => *receiver = updated,
    }
}
