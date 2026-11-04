//! Original command packets. Layout reference: BWAPI OrderTypes.h at
//! d727fed68558c506163048ea889131d8cbb33915. The game enforces tech/resources.
use serde_json::Value;

pub fn packet(request: &Value, building: bool) -> Result<Option<Vec<u8>>, &'static str> {
    let op = request["op"].as_str().ok_or("missing op")?;
    let bytes = match op {
        "research" | "upgrade" => {
            if !building { return Err("select a research building"); }
            let id = request["type"].as_u64().ok_or("missing technology/upgrade type")?;
            let limit = if op == "research" {44} else {61};
            if id >= limit {return Err("technology/upgrade outside DAT range");}
            vec![if op == "research" {0x30} else {0x32}, u8::try_from(id).map_err(|_| "type overflow")?]
        }
        "morph" => {
            let id = request["type"].as_u64().ok_or("missing morph type")?;
            if id >= 228 {return Err("morph type outside DAT range");}
            let kind = u16::try_from(id).map_err(|_| "type overflow")?;
            let mut bytes = vec![if building {0x35} else {0x23}];
            bytes.extend(kind.to_le_bytes());bytes
        }
        "cancel" => match request["action"].as_str().ok_or("missing cancellation action")? {
            "research" => vec![0x31], "upgrade" => vec![0x33], "addon" => vec![0x34],
            "construction" => vec![0x18], "morph" => vec![if building {0x18} else {0x19}],
            "train" => {
                let slot = match request.get("slot") {Some(value)=>value.as_u64().ok_or("invalid queue slot")?,None=>254};
                if slot > 4 && slot != 254 {return Err("queue slot must be 0..4 or omitted");}
                vec![0x20,u8::try_from(slot).map_err(|_| "slot overflow")?,0]
            }
            _ => return Err("unsupported cancellation action"),
        },
        "ability" => match request["action"].as_str().ok_or("missing ability action")? {
            "stim" => vec![0x36], "siege" => vec![0x26,0], "unsiege" => vec![0x25,0],
            "burrow" => vec![0x2c,0], "unburrow" => vec![0x2d,0],
            "cloak" => vec![0x21,0], "decloak" => vec![0x22,0],
            _ => return Err("unsupported ability; targeted spells need native targeting validation"),
        },
        "unload" => vec![0x28,0],
        _ => return Ok(None),
    };
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn research_rejects_units_and_out_of_range_tech() {
        assert!(packet(&json!({"op":"research","type":0}),false).is_err());
        assert!(packet(&json!({"op":"research","type":44}),true).is_err());
        assert_eq!(packet(&json!({"op":"research","type":0}),true).unwrap(),Some(vec![0x30,0]));
    }
    #[test]
    fn morph_distinguishes_building_and_unit_handler() {
        let request=json!({"op":"morph","type":131});
        assert_eq!(packet(&request,true).unwrap(),Some(vec![0x35,131,0]));
        assert_eq!(packet(&request,false).unwrap(),Some(vec![0x23,131,0]));
    }
    #[test]
    fn cancellation_rejects_bad_slot_without_truncation() {
        assert!(packet(&json!({"op":"cancel","action":"train","slot":256}),true).is_err());
        assert_eq!(packet(&json!({"op":"cancel","action":"train","slot":3}),true).unwrap(),Some(vec![0x20,3,0]));
    }
    #[test]
    fn unsupported_ability_never_becomes_raw_order() {
        assert!(packet(&json!({"op":"ability","action":"unknown","order":0}),false).is_err());
        assert_eq!(packet(&json!({"op":"ability","action":"stim"}),false).unwrap(),Some(vec![0x36]));
    }
}
