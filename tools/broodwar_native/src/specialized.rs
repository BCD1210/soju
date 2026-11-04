//! Specialized commands routed through the original selection/command handlers.
//! Packet layouts: BWAPI d727fed68558c506163048ea889131d8cbb33915,
//! BW/OrderTypes.h; extended unload: ShieldBattery
//! f5eab0dde407c61c4f7d2b0f64c24c960e3ef596 game/src/bw/commands.rs.
use bw_dat::{structs::Unit, UnitId};
use serde_json::Value;
use super::{Runtime, commands};

pub(super) fn merge_packet(types: &[u16], dark: bool) -> Result<[u8; 1], &'static str> {
    let caster = if dark { 61 } else { 67 };
    if types.len() < 2 || types.len() > 12 || types.iter().any(|kind| *kind != caster) {
        return Err("select 2 to 12 matching templars");
    }
    Ok([if dark { 0x5a } else { 0x2a }])
}

// SAFETY: caller is the authenticated game-thread dispatcher; selected pointers
// are current owned units and target is null or a current visible/owned unit.
pub(super) unsafe fn execute(r: &Runtime, selected: &[*mut Unit], request: &Value) -> Result<bool, &'static str> {
    let op = request["op"].as_str().ok_or("missing op")?;
    let packet = match op {
        "merge_archon" | "merge_dark_archon" => {
            let types: Vec<_> = selected.iter().map(|unit| (**unit).unit_id).collect();
            merge_packet(&types, op == "merge_dark_archon")?.to_vec()
        }
        "train_ammunition" => {
            if selected.iter().any(|unit| UnitId((**unit).unit_id).fighter_id().is_none()) {
                return Err("select carriers or reavers");
            }
            vec![0x27]
        }
        "nydus_exit" => {
            if selected.len() != 1 || (*selected[0]).unit_id != 134 {
                return Err("select one Nydus Canal");
            }
            let game = (r.game)().cast::<bw_dat::structs::Game>();
            let x = request["tileX"].as_u64().ok_or("missing exit tileX")?;
            let y = request["tileY"].as_u64().ok_or("missing exit tileY")?;
            if x >= u64::from((*game).map_width_tiles) || y >= u64::from((*game).map_height_tiles) {
                return Err("exit outside map");
            }
            let mut bytes = vec![0x0c, 46];
            bytes.extend(u16::try_from(x).map_err(|_| "exit x overflow")?.to_le_bytes());
            bytes.extend(u16::try_from(y).map_err(|_| "exit y overflow")?.to_le_bytes());
            bytes.extend(134u16.to_le_bytes());
            bytes
        }
        _ => return Ok(false),
    };
    commands::send_selected(r, selected, &packet)?;
    Ok(true)
}

// SAFETY: same game-thread/ownership contract as execute; target was resolved
// from the active pool and validated as visible by the dispatcher.
pub(super) unsafe fn targeted(r: &Runtime, units: &[*mut Unit], target: *mut Unit) -> Result<(), &'static str> {
    if units.len() != 1 || target.is_null() || (*target).related != units[0] || (*target).flags & 0x60 == 0 {
        return Err("select the passenger's transport and a loaded passenger");
    }
    let mut packet = vec![0x62];
    packet.extend(commands::unit_tag(r, target)?.to_le_bytes());
    commands::send_selected(r, units, &packet)
}

#[cfg(test)]
mod tests {
    use super::merge_packet;
    #[test]
    fn merge_preserves_original_group_handler_and_rejects_mixed_casters() {
        assert_eq!(merge_packet(&[67, 67], false), Ok([0x2a]));
        assert_eq!(merge_packet(&[61; 12], true), Ok([0x5a]));
        for types in [&[][..], &[67][..], &[67, 61][..], &[67; 13][..]] {
            assert!(merge_packet(types, false).is_err());
        }
        assert!(merge_packet(&[67, 67], true).is_err());
    }
}
