//! Small structural helpers for navigating dynamically-decoded `scale_value`
//! values (storage entries and runtime-API responses). We decode structurally
//! (by shape) rather than against generated types, because the live runtime is
//! ahead of any committed metadata/codegen.

use subxt::dynamic::{At, Value};
use subxt::ext::scale_value::{Composite, ValueDef};

/// The child values of a composite (named or unnamed), in order.
pub fn as_seq(v: &Value) -> Vec<&Value> {
    match &v.value {
        ValueDef::Composite(Composite::Unnamed(vs)) => vs.iter().collect(),
        ValueDef::Composite(Composite::Named(vs)) => vs.iter().map(|(_, x)| x).collect(),
        ValueDef::Variant(var) => match &var.values {
            Composite::Unnamed(vs) => vs.iter().collect(),
            Composite::Named(vs) => vs.iter().map(|(_, x)| x).collect(),
        },
        _ => Vec::new(),
    }
}

/// Number of items in a sequence/composite value.
pub fn seq_len(v: &Value) -> usize {
    as_seq(v).len()
}

/// Dig through single-field wrapper composites/variants (newtypes like
/// `ParaId(u32)`, `CoreIndex(u16)`, `Perbill(u32)`) to a primitive integer.
pub fn flat_u128(v: &Value) -> Option<u128> {
    if let Some(u) = v.as_u128() {
        return Some(u);
    }
    let inner = as_seq(v);
    if inner.len() == 1 {
        return flat_u128(inner[0]);
    }
    None
}

/// Same as [`flat_u128`] but narrowed to `u32`.
pub fn flat_u32(v: &Value) -> Option<u32> {
    flat_u128(v).and_then(|u| u32::try_from(u).ok())
}

/// The variant name of a value, if it is a variant.
pub fn variant_name(v: &Value) -> Option<&str> {
    match &v.value {
        ValueDef::Variant(var) => Some(var.name.as_str()),
        _ => None,
    }
}

/// Navigate to a named field, tolerating both snake_case and the literal name.
pub fn field<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    v.at(name)
}
