use serde_json::Value;

pub fn write_scalar(
    field: &mut Value,
    mut value: Value,
    definition: Option<&Value>,
) -> Result<(), String> {
    let wrapper = match field["__type"].as_str() {
        Some("Int") if value.as_i64().is_some_and(|n| i32::try_from(n).is_ok()) => "V_Int",
        Some("Float") if value.is_number() => "V_Float",
        Some("Bool") if value.is_boolean() => "V_Bool",
        _ => return Err("Value does not match a supported scalar field type".into()),
    };
    if let Some(definition) = definition {
        if wrapper == "V_Int" {
            let mut n = value.as_i64().unwrap();
            if let Some(min) = definition["min"].as_f64() {
                n = n.max((min + 0.5).floor() as i64);
            }
            if let Some(max) = definition["max"].as_f64() {
                n = n.min((max + 0.5).floor() as i64);
            }
            value = n.into();
        } else if wrapper == "V_Float" {
            let mut n = value.as_f64().unwrap();
            if let Some(min) = definition["min"].as_f64() {
                n = n.max(min);
            }
            if let Some(max) = definition["max"].as_f64() {
                n = n.min(max);
            }
            value = n.into();
        }
    }
    field["realEditorValues"] = serde_json::json!([{"id":wrapper,"params":[value.clone()]}]);
    field["__value"] = value;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn changing_an_integer_updates_the_editor_value_too() {
        let mut field = json!({"__type":"Int","__value":10,"realEditorValues":[],"defUid":40,"extension":"keep"});
        write_scalar(&mut field, json!(25), None).unwrap();
        assert_eq!(field["__value"], 25);
        assert_eq!(
            field["realEditorValues"],
            json!([{"id":"V_Int","params":[25]}])
        );
        assert_eq!(field["extension"], "keep");
    }

    #[test]
    fn bool_and_float_are_serialized_with_their_original_wrappers() {
        let mut field = json!({"__type":"Bool","__value":false});
        write_scalar(&mut field, json!(true), None).unwrap();
        assert_eq!(
            field["realEditorValues"],
            json!([{"id":"V_Bool","params":[true]}])
        );
        field["__type"] = json!("Float");
        write_scalar(&mut field, json!(2.5), None).unwrap();
        assert_eq!(
            field["realEditorValues"],
            json!([{"id":"V_Float","params":[2.5]}])
        );
    }

    #[test]
    fn wrong_type_does_not_modify_the_field() {
        let mut field = json!({"__type":"Int","__value":3,"realEditorValues":[]});
        let before = field.clone();
        assert!(write_scalar(&mut field, json!("bad"), None).is_err());
        assert_eq!(field, before);
    }

    #[test]
    fn integer_and_float_bounds_apply_to_both_representations() {
        let mut field = json!({"__type":"Int","__value":10,"realEditorValues":[]});
        let definition = json!({"min":0,"max":100});
        write_scalar(&mut field, json!(200), Some(&definition)).unwrap();
        assert_eq!(field["__value"], 100);
        assert_eq!(field["realEditorValues"][0]["params"][0], 100);
        field["__type"] = json!("Float");
        let definition = json!({"min":0.5,"max":2.5});
        write_scalar(&mut field, json!(-2.0), Some(&definition)).unwrap();
        assert_eq!(field["__value"], 0.5);
        assert_eq!(field["realEditorValues"][0]["params"][0], 0.5);
    }
}
