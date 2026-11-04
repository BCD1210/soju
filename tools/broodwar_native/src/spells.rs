//! Targeted commands use named caster allowlists and original orders.dat rows.
use bw_dat::{order, OrderId};

pub struct Spell {
    pub action: &'static str,
    pub title: &'static str,
    pub order: OrderId,
    pub casters: &'static [u16],
    pub unit_target: bool,
}

pub const SPELLS: &[Spell] = &[
    Spell {action:"lockdown",title:"Lockdown",order:order::LOCKDOWN,casters:&[1],unit_target:true},
    Spell {action:"emp",title:"EMP shockwave",order:order::EMP_SHOCKWAVE,casters:&[9],unit_target:false},
    Spell {action:"matrix",title:"Defensive matrix",order:order::DEFENSIVE_MATRIX,casters:&[9],unit_target:true},
    Spell {action:"irradiate",title:"Irradiate",order:order::IRRADIATE,casters:&[9],unit_target:true},
    Spell {action:"yamato",title:"Yamato gun",order:order::YAMATO_GUN,casters:&[12],unit_target:true},
    Spell {action:"scan",title:"Scanner sweep",order:order::SCANNER_SWEEP,casters:&[107],unit_target:false},
    Spell {action:"mine",title:"Spider mine",order:order::PLACE_MINE,casters:&[2],unit_target:false},
    Spell {action:"heal",title:"Heal",order:order::MEDIC_HEAL,casters:&[34],unit_target:true},
    Spell {action:"restoration",title:"Restoration",order:order::RESTORATION,casters:&[34],unit_target:true},
    Spell {action:"flare",title:"Optical flare",order:order::OPTICAL_FLARE,casters:&[34],unit_target:true},
    Spell {action:"swarm",title:"Dark swarm",order:order::DARK_SWARM,casters:&[46],unit_target:false},
    Spell {action:"plague",title:"Plague",order:order::PLAGUE,casters:&[46],unit_target:false},
    Spell {action:"consume",title:"Consume",order:order::CONSUME,casters:&[46],unit_target:true},
    Spell {action:"parasite",title:"Parasite",order:order::PARASITE,casters:&[45],unit_target:true},
    Spell {action:"broodling",title:"Spawn broodling",order:order::SPAWN_BROODLING,casters:&[45],unit_target:true},
    Spell {action:"ensnare",title:"Ensnare",order:order::ENSNARE,casters:&[45],unit_target:false},
    Spell {action:"storm",title:"Psionic storm",order:order::PSI_STORM,casters:&[67],unit_target:false},
    Spell {action:"hallucination",title:"Hallucination",order:order::HALLUCINATION,casters:&[67],unit_target:true},
    Spell {action:"recall",title:"Recall",order:order::RECALL,casters:&[71],unit_target:false},
    Spell {action:"stasis",title:"Stasis field",order:order::STASIS_FIELD,casters:&[71],unit_target:false},
    Spell {action:"web",title:"Disruption web",order:order::DISRUPTION_WEB,casters:&[60],unit_target:false},
    Spell {action:"mind_control",title:"Mind control",order:order::MIND_CONTROL,casters:&[63],unit_target:true},
    Spell {action:"feedback",title:"Feedback",order:order::FEEDBACK,casters:&[63],unit_target:true},
    Spell {action:"maelstrom",title:"Maelstrom",order:order::MAELSTROM,casters:&[63],unit_target:false},
];

pub fn validate(action: &str, caster: u16, has_target: bool) -> Result<&'static Spell, &'static str> {
    let spell=SPELLS.iter().find(|spell| spell.action==action).ok_or("unsupported targeted spell")?;
    if !spell.casters.contains(&caster) {return Err("selected unit cannot cast this spell");}
    if spell.unit_target != has_target {return Err("spell target kind mismatch");}
    Ok(spell)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_caster_and_target_before_dispatch() {
        assert!(validate("storm",67,false).is_ok());
        assert!(validate("storm",7,false).is_err());
        assert!(validate("storm",67,true).is_err());
        assert!(validate("lockdown",1,false).is_err());
        assert!(validate("arbitrary",1,true).is_err());
    }
    #[test]
    fn public_actions_are_unique() {
        let names:std::collections::HashSet<_>=SPELLS.iter().map(|spell|spell.action).collect();
        assert_eq!(names.len(),SPELLS.len());
    }
}
